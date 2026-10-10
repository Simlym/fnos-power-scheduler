#!/usr/bin/env bash
set -euo pipefail
task_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$task_root"
task_cargo=${CARGO_BIN:-cargo}
task_fnpack=${FNPACK_BIN:-fnpack}
for task_tool in "$task_cargo" "$task_fnpack" node npm python3; do
    command -v "$task_tool" >/dev/null || { echo "Missing build tool: $task_tool" >&2; exit 1; }
done
task_fnpack=$(python3 -c 'import os, shutil, sys; print(os.path.abspath(shutil.which(sys.argv[1])))' "$task_fnpack")
task_fnpack_help=$("$task_fnpack" --help)
[[ "$task_fnpack_help" == *"Version 1.2.3"* ]] || { echo "Use fnpack 1.2.3; inspect fnpack --help for its version." >&2; exit 1; }
(cd web && npm ci && npm run build)
"$task_cargo" test --locked
task_stage="$task_root/dist/fpk"
rm -rf -- "$task_stage"
mkdir -p "$task_stage"
cp -R fpk/. "$task_stage/"
mkdir -p "$task_stage/app/bin" "$task_stage/app/www"
cp -R web/dist/. "$task_stage/app/www/"
task_target=${RUST_TARGET:-x86_64-unknown-linux-musl}
"$task_cargo" build --locked --release --target "$task_target" --bins
for task_binary in server powerd; do
    cp "target/$task_target/release/$task_binary" "$task_stage/app/bin/$task_binary"
done
find "$task_stage" -type d -exec chmod 0755 {} +
find "$task_stage" -type f -exec chmod 0644 {} +
chmod 0755 "$task_stage"/cmd/* "$task_stage"/app/bin/*
task_package="$task_root/dist/fnos-power-scheduler.fpk"
rm -f -- "$task_package" "$task_package.sha256"
(cd dist && "$task_fnpack" build --directory "$task_stage")
python3 scripts/fpk.py normalize "$task_package"
echo "Installable package: $task_package"
