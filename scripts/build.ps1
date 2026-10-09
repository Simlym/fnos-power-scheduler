$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskGo = Join-Path $taskRoot 'work\sdk\go\bin\go.exe'
if (-not (Test-Path -LiteralPath $taskGo)) { $taskGo = (Get-Command go -ErrorAction Stop).Source }
Push-Location $taskRoot
try {
 Push-Location web
 try { npm ci; if ($LASTEXITCODE -ne 0) { throw 'Frontend dependency install failed' }; npm run build; if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed' } } finally { Pop-Location }
 & $taskGo test ./...
 if ($LASTEXITCODE -ne 0) { throw 'Go tests failed' }
 $taskStage = Join-Path $taskRoot 'dist\fpk'
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
 $taskFnpackPath = Join-Path $taskRoot 'work\fnpack.exe'
 if (-not (Test-Path -LiteralPath $taskFnpackPath)) { $taskFnpack = Get-Command fnpack -ErrorAction SilentlyContinue; if ($taskFnpack) { $taskFnpackPath = $taskFnpack.Source } }
 if (Test-Path -LiteralPath $taskFnpackPath) {
  Push-Location (Join-Path $taskRoot 'dist')
  try { & $taskFnpackPath build --directory $taskStage; if ($LASTEXITCODE -ne 0) { throw 'FPK packaging failed' } } finally { Pop-Location }
 }
 else { Write-Output "Linux x86_64 应用已生成：$taskStage；安装官方 fnpack 后执行 fnpack build --directory 此目录。" }
} finally { Pop-Location }
