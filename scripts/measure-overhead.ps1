# Measures PC Health Console's own resource use (process + its WebView2 processes).
# Usage: powershell -ExecutionPolicy Bypass -File scripts/measure-overhead.ps1 [-Exe path] [-Seconds 120]
# Runs the release build unelevated (RunAsInvoker compatibility layer) with a temporary
# data folder, then samples two scenarios: window visible on Overview, and minimized to tray.
param(
    [string]$Exe = "$PSScriptRoot\..\target\release\pc-health-console.exe",
    [int]$Seconds = 120,
    [int]$WarmupSeconds = 30
)
$ErrorActionPreference = "Stop"
Add-Type @"
using System; using System.Runtime.InteropServices;
public class MW {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, System.Text.StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  public static IntPtr Find(uint pid) { IntPtr r = IntPtr.Zero; EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p); var sb = new System.Text.StringBuilder(256); GetWindowText(h, sb, 256); if (p == pid && sb.ToString() == "PC Health Console") r = h; return true; }, IntPtr.Zero); return r; }
}
"@

function Tree([int]$root) {
    $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name, WriteTransferCount
    $ids = @($root); $added = $true
    while ($added) {
        $added = $false
        foreach ($p in $all) { if ($ids -contains $p.ParentProcessId -and -not ($ids -contains $p.ProcessId)) { $ids += $p.ProcessId; $added = $true } }
    }
    $all | Where-Object { $ids -contains $_.ProcessId }
}

function Snapshot([int]$root) {
    $tree = Tree $root
    $cpu = 0.0; $ws = 0; $priv = 0; $write = 0
    foreach ($t in $tree) {
        $p = Get-Process -Id $t.ProcessId -ErrorAction SilentlyContinue
        if ($p) { $cpu += $p.TotalProcessorTime.TotalSeconds; $ws += $p.WorkingSet64; $priv += $p.PrivateMemorySize64 }
        $write += [double]$t.WriteTransferCount
    }
    $own = Get-Process -Id $root
    [pscustomobject]@{ Cpu = $cpu; Ws = $ws; Priv = $priv; Write = $write; OwnCpu = $own.TotalProcessorTime.TotalSeconds; OwnWs = $own.WorkingSet64; Count = $tree.Count; At = Get-Date }
}

function Measure-Scenario([int]$root, [string]$name) {
    $a = Snapshot $root; Start-Sleep -Seconds $Seconds; $b = Snapshot $root
    $wall = ($b.At - $a.At).TotalSeconds
    $cores = [Environment]::ProcessorCount
    [pscustomobject]@{
        Scenario = $name
        "Total CPU % of machine" = [math]::Round(($b.Cpu - $a.Cpu) / $wall / $cores * 100, 3)
        "Total CPU % of one core" = [math]::Round(($b.Cpu - $a.Cpu) / $wall * 100, 2)
        "Backend CPU % of one core" = [math]::Round(($b.OwnCpu - $a.OwnCpu) / $wall * 100, 2)
        "Backend working set MB" = [math]::Round($b.OwnWs / 1MB, 1)
        "Total working set MB" = [math]::Round($b.Ws / 1MB, 1)
        "Total private MB" = [math]::Round($b.Priv / 1MB, 1)
        "Writes KB/s (all I/O)" = [math]::Round(($b.Write - $a.Write) / $wall / 1KB, 1)
        Processes = $b.Count
    }
}

$data = Join-Path $env:TEMP "pchc-overhead-$PID"
New-Item -ItemType Directory -Force $data | Out-Null
$env:PCHC_DATA_DIR = $data
$env:__COMPAT_LAYER = "RunAsInvoker"
$proc = Start-Process -FilePath (Resolve-Path $Exe) -PassThru
try {
    Start-Sleep -Seconds $WarmupSeconds
    $results = @(Measure-Scenario $proc.Id "Window visible (Overview)")
    $h = [MW]::Find([uint32]$proc.Id); [MW]::ShowWindow($h, 6) | Out-Null   # minimize -> hidden to tray
    Start-Sleep -Seconds 5
    $results += Measure-Scenario $proc.Id "Minimized to tray"
    $results | Format-List
    $db = Get-Item (Join-Path $data "history.db") -ErrorAction SilentlyContinue
    if ($db) { "History database after run: {0:N0} KB" -f ($db.Length / 1KB) }
} finally {
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
}
