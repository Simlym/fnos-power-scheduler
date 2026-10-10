param([string]$FnpackPath = '', [string]$ZigPath = '')
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskCargo = (Get-Command cargo -ErrorAction Stop).Source
if (-not $ZigPath) { $ZigPath = Join-Path $taskRoot 'work\zig-sdk\ziglang\zig.exe' }
if (-not (Test-Path -LiteralPath $ZigPath)) { $ZigPath = (Get-Command zig -ErrorAction Stop).Source }
$ZigPath = (Resolve-Path -LiteralPath $ZigPath).Path
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
 & $taskCargo test --locked
 if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed' }
 $taskStage = Join-Path $taskRoot 'dist\fpk'
 $taskDist = [System.IO.Path]::GetFullPath((Join-Path $taskRoot 'dist'))
 if (-not ([System.IO.Path]::GetFullPath($taskStage)).StartsWith($taskDist + [System.IO.Path]::DirectorySeparatorChar)) { throw 'Unsafe staging path' }
 if (Test-Path -LiteralPath $taskStage) { Remove-Item -LiteralPath $taskStage -Recurse -Force }
 New-Item -ItemType Directory -Force -Path $taskStage | Out-Null
 Copy-Item -Path 'fpk\*' -Destination $taskStage -Recurse -Force
 New-Item -ItemType Directory -Force -Path "$taskStage\app\bin","$taskStage\app\www" | Out-Null
 Copy-Item -Path 'web\dist\*' -Destination "$taskStage\app\www" -Recurse -Force
 New-Item -ItemType Directory -Force -Path 'work' | Out-Null
 $taskLinker = Join-Path $taskRoot 'work\zig-cc.cmd'
 [System.IO.File]::WriteAllText($taskLinker, "@echo off`r`n`"$taskPython`" `"$taskRoot\scripts\zig-cc.py`" `"$ZigPath`" %*`r`n", [System.Text.Encoding]::ASCII)
 $taskPreviousCC = $env:CC_x86_64_unknown_linux_musl
 $taskPreviousAR = $env:AR_x86_64_unknown_linux_musl
 $taskPreviousLinker = $env:CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER
 $taskPreviousZigCache = $env:ZIG_GLOBAL_CACHE_DIR
 try {
  $env:CC_x86_64_unknown_linux_musl=$taskLinker
  $taskArchiver=Join-Path $taskRoot 'work\zig-ar.cmd'
  [System.IO.File]::WriteAllText($taskArchiver, "@echo off`r`n`"$ZigPath`" ar %*`r`n", [System.Text.Encoding]::ASCII)
  $env:AR_x86_64_unknown_linux_musl=$taskArchiver
  $env:CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=$taskLinker
  $env:ZIG_GLOBAL_CACHE_DIR=Join-Path $taskRoot 'work\zig-cache'
  & $taskCargo build --locked --release --target x86_64-unknown-linux-musl --bins
  if ($LASTEXITCODE -ne 0) { throw 'Rust Linux build failed; install rustup target x86_64-unknown-linux-musl and Zig' }
  Copy-Item -LiteralPath 'target\x86_64-unknown-linux-musl\release\server','target\x86_64-unknown-linux-musl\release\powerd' -Destination "$taskStage\app\bin"
 } finally { $env:CC_x86_64_unknown_linux_musl=$taskPreviousCC; $env:AR_x86_64_unknown_linux_musl=$taskPreviousAR; $env:CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=$taskPreviousLinker; $env:ZIG_GLOBAL_CACHE_DIR=$taskPreviousZigCache }
 $taskPackage = Join-Path $taskRoot 'dist\fnos-power-scheduler.fpk'
 Remove-Item -LiteralPath $taskPackage,"$taskPackage.sha256" -Force -ErrorAction SilentlyContinue
 Push-Location (Join-Path $taskRoot 'dist')
 try { & $taskFnpackPath build --directory $taskStage; if ($LASTEXITCODE -ne 0) { throw 'FPK packaging failed' } } finally { Pop-Location }
 & $taskPython scripts/fpk.py normalize $taskPackage
 if ($LASTEXITCODE -ne 0) { throw 'FPK validation failed; do not install this package' }
 Write-Output "可安装的 x86_64 FPK 已生成：$taskPackage"
} finally { Pop-Location }
