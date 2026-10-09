param([string]$FnpackPath = '')
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskGo = Join-Path $taskRoot 'work\sdk\go\bin\go.exe'
if (-not (Test-Path -LiteralPath $taskGo)) { $taskGo = (Get-Command go -ErrorAction Stop).Source }
$taskPython = (Get-Command python -ErrorAction Stop).Source
& $taskPython -c 'import sys; sys.exit(sys.version_info < (3, 9))'
if ($LASTEXITCODE -ne 0) { throw 'Python 3.9+ is required for FPK validation' }
$taskFnpackPath = $FnpackPath
if (-not $taskFnpackPath) { $taskFnpackPath = Join-Path $taskRoot 'work\fnpack.exe' }
if (-not (Test-Path -LiteralPath $taskFnpackPath)) { $taskFnpackPath = (Get-Command fnpack -ErrorAction Stop).Source }
$taskFnpackPath = (Resolve-Path -LiteralPath $taskFnpackPath).Path
$taskFnpackHelp = & $taskFnpackPath --help
if ($LASTEXITCODE -ne 0 -or ($taskFnpackHelp -join "`n") -notmatch 'Version 1\.2\.3\b') { throw 'Use fnpack 1.2.3; inspect fnpack --help for its version' }
Push-Location $taskRoot
try {
 Push-Location web
 try { npm ci; if ($LASTEXITCODE -ne 0) { throw 'Frontend dependency install failed' }; npm run build; if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed' } } finally { Pop-Location }
 & $taskGo test ./...
 if ($LASTEXITCODE -ne 0) { throw 'Go tests failed' }
 $taskStage = Join-Path $taskRoot 'dist\fpk'
 if (Test-Path -LiteralPath $taskStage) { Remove-Item -LiteralPath $taskStage -Recurse -Force }
 New-Item -ItemType Directory -Force -Path $taskStage | Out-Null
 Copy-Item -Path 'fpk\*' -Destination $taskStage -Recurse -Force
 New-Item -ItemType Directory -Force -Path "$taskStage\app\bin","$taskStage\app\www" | Out-Null
 Copy-Item -Path 'web\dist\*' -Destination "$taskStage\app\www" -Recurse -Force
 $taskPreviousOS = $env:GOOS; $taskPreviousArch = $env:GOARCH; $taskPreviousCGO = $env:CGO_ENABLED
 try {
  $env:GOOS='linux'; $env:GOARCH='amd64'; $env:CGO_ENABLED='0'
  & $taskGo build -trimpath -ldflags '-s -w' -o "$taskStage\app\bin\server" ./cmd/server
  if ($LASTEXITCODE -ne 0) { throw 'Server build failed' }
  & $taskGo build -trimpath -ldflags '-s -w' -o "$taskStage\app\bin\powerd" ./cmd/powerd
  if ($LASTEXITCODE -ne 0) { throw 'Power helper build failed' }
 } finally { $env:GOOS=$taskPreviousOS; $env:GOARCH=$taskPreviousArch; $env:CGO_ENABLED=$taskPreviousCGO }
 $taskPackage = Join-Path $taskRoot 'dist\fnos-power-scheduler.fpk'
 Remove-Item -LiteralPath $taskPackage,"$taskPackage.sha256" -Force -ErrorAction SilentlyContinue
 Push-Location (Join-Path $taskRoot 'dist')
 try { & $taskFnpackPath build --directory $taskStage; if ($LASTEXITCODE -ne 0) { throw 'FPK packaging failed' } } finally { Pop-Location }
 & $taskPython scripts/fpk.py normalize $taskPackage
 if ($LASTEXITCODE -ne 0) { throw 'FPK validation failed; do not install this package' }
 Write-Output "可安装的 x86_64 FPK 已生成：$taskPackage"
} finally { Pop-Location }
