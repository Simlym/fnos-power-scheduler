#!/usr/bin/env bash
set -euo pipefail
task_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$task_root"
task_go=${GO_BIN:-go}
task_fnpack=${FNPACK_BIN:-fnpack}
for task_tool in "$task_go" "$task_fnpack" node npm python3; do
    command -v "$task_tool" >/dev/null || { echo "Missing build tool: $task_tool" >&2; exit 1; }
done
task_fnpack=$(python3 -c 'import os, shutil, sys; print(os.path.abspath(shutil.which(sys.argv[1])))' "$task_fnpack")
task_fnpack_help=$("$task_fnpack" --help)
[[ "$task_fnpack_help" == *"Version 1.2.3"* ]] || { echo "Use fnpack 1.2.3; inspect fnpack --help for its version." >&2; exit 1; }
(cd web && npm ci && npm run build)
"$task_go" test ./cmd/... ./internal/...
task_stage="$task_root/dist/fpk"
rm -rf -- "$task_stage"
mkdir -p "$task_stage"
cp -R fpk/. "$task_stage/"
mkdir -p "$task_stage/app/bin" "$task_stage/app/www"
cp -R web/dist/. "$task_stage/app/www/"
for task_binary in server powerd; do
    GOOS=linux GOARCH=amd64 CGO_ENABLED=0 "$task_go" build -trimpath -ldflags '-s -w' -o "$task_stage/app/bin/$task_binary" "./cmd/$task_binary"
done
find "$task_stage" -type d -exec chmod 0755 {} +
find "$task_stage" -type f -exec chmod 0644 {} +
chmod 0755 "$task_stage"/cmd/* "$task_stage"/app/bin/*
task_package="$task_root/dist/fnos-power-scheduler.fpk"
rm -f -- "$task_package" "$task_package.sha256"
(cd dist && "$task_fnpack" build --directory "$task_stage")
python3 scripts/fpk.py normalize "$task_package"
echo "Installable package: $task_package"
