; Installer hooks (bundle > windows > nsis > installerHooks).

; PresentMon runs from $INSTDIR\tools and outlives Veyl when Veyl exits for an update during a
; capture. Windows cannot replace or delete a running program's file, so stop it first. CIM sees
; the 64-bit process paths from this 32-bit installer; only programs in Veyl's tools folder are stopped.
!macro VEYL_STOP_TOOLS
  nsExec::Exec `"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -Command "Get-CimInstance Win32_Process | Where-Object { $$_.ExecutablePath -and $$_.ExecutablePath.StartsWith('$INSTDIR\tools\', 'OrdinalIgnoreCase') } | ForEach-Object { Stop-Process -Id $$_.ProcessId -Force -ErrorAction SilentlyContinue; Wait-Process -Id $$_.ProcessId -Timeout 5 -ErrorAction SilentlyContinue }"`
  Pop $0
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro VEYL_STOP_TOOLS
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro VEYL_STOP_TOOLS
!macroend
