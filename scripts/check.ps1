# Quality gate: formatting, lints, Rust tests, TypeScript types, frontend tests and build.
# Usage: powershell -ExecutionPolicy Bypass -File scripts/check.ps1
$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)

function Step($name, [scriptblock]$cmd) {
    Write-Host "==> $name" -ForegroundColor Cyan
    & $cmd
    if ($LASTEXITCODE -ne 0) { throw "$name failed (exit $LASTEXITCODE)" }
}

Step "cargo fmt" { cargo fmt --all -- --check }
Step "cargo clippy" { cargo clippy --workspace --all-targets -- -D warnings }
Step "cargo test" { cargo test --workspace }
Step "typecheck" { npx tsc --noEmit }
Step "vitest" { npx vitest run }
Step "vite build" { npx vite build }
Write-Host "All checks passed." -ForegroundColor Green
