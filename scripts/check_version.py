"""Check release versions before building or publishing (Python 3.11+)."""

import argparse
import json
from pathlib import Path
import re
import tomllib


def check(root, tag=None):
    version = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", version):
        raise ValueError(f"Unsupported release version: {version}")
    manifest = dict(line.split("=", 1) for line in
                    (root / "fpk/manifest").read_text(encoding="utf-8-sig").splitlines()
                    if line.strip() and not line.lstrip().startswith("#"))
    package = json.loads((root / "web/package.json").read_text(encoding="utf-8"))
    lock = json.loads((root / "web/package-lock.json").read_text(encoding="utf-8"))
    versions = {"fpk/manifest": manifest.get("version"),
                "web/package.json": package["version"],
                "web/package-lock.json": lock["version"],
                "web/package-lock.json packages root": lock["packages"][""]["version"]}
    for source, actual in versions.items():
        if actual != version:
            raise ValueError(f"{source}: {actual!r} does not match Cargo.toml {version!r}")
    if tag is not None and tag != f"v{version}":
        raise ValueError(f"Release tag {tag!r} must be v{version}")
    return version


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag")
    args = parser.parse_args()
    print(check(Path(__file__).resolve().parent.parent, args.tag))
