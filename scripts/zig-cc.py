"""Translate Cargo's LLVM triple for Zig's Windows-to-Linux compiler."""
import os
import subprocess
import sys
import shlex
from pathlib import Path

startup_objects = {"rcrt1.o", "crt1.o", "crti.o", "crtn.o", "crtbeginS.o", "crtendS.o"}
expanded = []
for arg in sys.argv[2:]:
    if arg.startswith("@"):
        expanded.extend(shlex.split(Path(arg[1:]).read_text(encoding="utf-8")))
    else:
        expanded.append(arg)
args = [arg for arg in expanded
        if not arg.startswith("--target=")
        and os.path.basename(arg.replace("\\", "/")) not in startup_objects
        and arg != "-nostartfiles"]
# Rust's object list can exceed Windows' process argument limit. Zig accepts GNU
# response files, so preserve long invocations after translating their contents.
import tempfile
with tempfile.NamedTemporaryFile(mode="w", suffix=".rsp", encoding="utf-8", delete=False) as output:
    response_path = output.name
    for arg in args:
        output.write('"' + arg.replace("\\", "\\\\").replace('"', '\\"') + '"\n')
try:
    raise SystemExit(subprocess.call([sys.argv[1], "cc", "-target", "x86_64-linux-musl", "@" + response_path]))
finally:
    os.unlink(response_path)
