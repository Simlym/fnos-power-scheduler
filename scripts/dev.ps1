$ErrorActionPreference='Stop'
$taskRoot=Split-Path -Parent $PSScriptRoot
$taskGo=Join-Path $taskRoot 'work\sdk\go\bin\go.exe'
if (-not (Test-Path -LiteralPath $taskGo)) { $taskGo=(Get-Command go -ErrorAction Stop).Source }
Push-Location $taskRoot
try { & $taskGo run ./cmd/server } finally { Pop-Location }
