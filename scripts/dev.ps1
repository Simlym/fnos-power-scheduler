$ErrorActionPreference='Stop'
$taskRoot=Split-Path -Parent $PSScriptRoot
Push-Location $taskRoot
try { cargo run --locked --bin server } finally { Pop-Location }
