"""Normalize and check the installable archive produced by fnpack (Python 3.9+)."""

import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import struct
import tarfile
import tempfile


def read_archive(data):
    entries = {}
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        for member in archive:
            name = member.name.removeprefix("./").rstrip("/")
            if name in ("", "."):
                continue
            path = PurePosixPath(name)
            if path.is_absolute() or ".." in path.parts or name in entries:
                raise ValueError(f"Unsafe or duplicate archive path: {name}")
            if not (member.isfile() or member.isdir()):
                raise ValueError(f"Unsupported archive entry: {name}")
            content = archive.extractfile(member).read() if member.isfile() else None
            entries[name] = (member.mode, content)
    return entries


def write_archive(entries, executable):
    result = io.BytesIO()
    with tarfile.open(fileobj=result, mode="w:gz", format=tarfile.USTAR_FORMAT) as archive:
        for name, (_, content) in sorted(entries.items()):
            member = tarfile.TarInfo(name)
            member.uid = member.gid = 0
            member.uname = member.gname = "root"
            member.mode = 0o755 if content is None or executable(name) else 0o644
            if content is None:
                member.type = tarfile.DIRTYPE
            else:
                member.size = len(content)
            archive.addfile(member, io.BytesIO(content) if content is not None else None)
    return result.getvalue()


def required(entries, name):
    if name not in entries or entries[name][1] is None:
        raise ValueError(f"Missing file: {name}")
    return entries[name][1]


def manifest_values(data):
    text = data.decode("utf-8-sig")
    values = {}
    for line in text.splitlines():
        if line.strip() and not line.lstrip().startswith("#"):
            key, value = line.split("=", 1)
            key = key.strip()
            if key in values:
                raise ValueError(f"Duplicate manifest key: {key}")
            values[key] = value.strip()
    return values


def validate(data):
    outer = read_archive(data)
    values = manifest_values(required(outer, "manifest"))
    if values.get("appname") != "fnos-power-scheduler" or values.get("platform") != "x86":
        raise ValueError("Expected fnos-power-scheduler with platform=x86")
    for name in ("ICON.PNG", "ICON_256.PNG", "config/privilege", "config/resource",
                 "wizard/install", "wizard/uninstall"):
        required(outer, name)
    payload = required(outer, "app.tgz")
    if values.get("checksum") != hashlib.md5(payload).hexdigest():
        raise ValueError("manifest checksum does not match app.tgz")
    for name in ("config/privilege", "config/resource", "wizard/install", "wizard/uninstall"):
        json.loads(required(outer, name))
    for name in ("cmd/main", "cmd/install_init", "cmd/install_callback",
                 "cmd/uninstall_init", "cmd/uninstall_callback", "cmd/upgrade_init",
                 "cmd/upgrade_callback", "cmd/config_init", "cmd/config_callback"):
        script = required(outer, name)
        if outer[name][0] & 0o111 != 0o111:
            raise ValueError(f"Script must be executable before installation: {name}")
        if not script.startswith(b"#!/bin/bash\n") or b"\r" in script:
            raise ValueError(f"Expected LF bash script: {name}")
    inner = read_archive(payload)
    required(inner, "www/index.html")
    json.loads(required(inner, "ui/config"))
    for name in ("bin/server", "bin/powerd"):
        binary = required(inner, name)
        if len(binary) < 64 or binary[:6] != b"\x7fELF\x02\x01" or struct.unpack_from("<H", binary, 18)[0] != 62:
            raise ValueError(f"Expected Linux x86_64 ELF executable: {name}")
        if inner[name][0] & 0o111 != 0o111:
            raise ValueError(f"Binary must be executable: {name}")
    for entries in (outer, inner):
        for name, (mode, content) in entries.items():
            if content is None and mode & 0o555 != 0o555:
                raise ValueError(f"Directory is not traversable: {name}")
            if content is not None and mode & 0o444 != 0o444:
                raise ValueError(f"File is not readable by the app user: {name}")
            if mode & 0o7022:
                raise ValueError(f"Unexpected special bits or writable permissions: {name}")
    return values


def normalize(path):
    outer = read_archive(path.read_bytes())
    inner = read_archive(required(outer, "app.tgz"))
    payload = write_archive(inner, lambda name: name in ("bin/server", "bin/powerd"))
    outer["app.tgz"] = (0o644, payload)
    values = manifest_values(required(outer, "manifest"))
    values["checksum"] = hashlib.md5(payload).hexdigest()
    outer["manifest"] = (0o644, "".join(f"{key}={value}\n" for key, value in values.items()).encode("utf-8"))
    data = write_archive(outer, lambda name: name.startswith("cmd/"))
    validate(data)
    descriptor, temporary = tempfile.mkstemp(dir=path.parent, suffix=".tmp")
    try:
        with os.fdopen(descriptor, "wb") as target:
            target.write(data)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("normalize", "verify"))
    parser.add_argument("package", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "normalize":
            normalize(args.package)
        data = args.package.read_bytes()
        values = validate(data)
        digest = hashlib.sha256(data).hexdigest()
        args.package.with_suffix(".fpk.sha256").write_text(f"{digest}  {args.package.name}\n", encoding="utf-8")
        print(f"Verified {args.package}: version={values['version']}, platform=x86, os_min_version={values.get('os_min_version', 'unspecified')}")
        print(f"SHA256: {digest}")
    except (ValueError, OSError, tarfile.TarError) as error:
        parser.exit(1, f"FPK check failed: {error}\n")


if __name__ == "__main__":
    main()
