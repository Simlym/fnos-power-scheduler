"""Regression checks for package corruption, CPU mismatch and Windows permissions."""

import hashlib
from pathlib import Path
import struct
import tempfile
import unittest

import fpk


def fixture(machine=62):
    binary = bytearray(64)
    binary[:6] = b"\x7fELF\x02\x01"
    struct.pack_into("<H", binary, 18, machine)
    inner = {name: (0o600, content) for name, content in {
        "bin/server": bytes(binary), "bin/powerd": bytes(binary),
        "www/index.html": b"<html></html>", "ui/config": b"{}",
    }.items()}
    inner["bin"] = (0o700, None)
    payload = fpk.write_archive(inner, lambda _: False)
    outer = {name: (0o600, content) for name, content in {
        "manifest": ("appname=fnos-power-scheduler\nversion=0.1.0\nplatform=x86\n"
                     f"checksum={hashlib.md5(payload).hexdigest()}\n").encode(),
        "app.tgz": payload, "ICON.PNG": b"icon", "ICON_256.PNG": b"icon",
        "config/privilege": b"{}", "config/resource": b"{}",
        "wizard/install": b'[{"stepTitle":"Setup","items":[{"type":"tips","helpText":"Setup"}]}]',
        "wizard/uninstall": b'[{"stepTitle":"Uninstall","items":[{"type":"tips","helpText":"Uninstall"}]}]',
    }.items()}
    for name in ("main", "prepare_data", "install_init", "install_callback", "uninstall_init",
                 "uninstall_callback", "upgrade_init", "upgrade_callback",
                 "config_init", "config_callback"):
        outer[f"cmd/{name}"] = (0o600, b"#!/bin/bash\nexit 0\n")
    return fpk.write_archive(outer, lambda _: False)


class PackageTests(unittest.TestCase):
    def test_missing_database_permission_repair_is_rejected(self):
        outer = fpk.read_archive(fixture())
        del outer["cmd/prepare_data"]
        with self.assertRaisesRegex(ValueError, "Missing file: cmd/prepare_data"):
            fpk.validate(fpk.write_archive(outer, lambda name: name.startswith("cmd/")))

    def test_empty_wizard_is_rejected(self):
        for name in ("wizard/install", "wizard/uninstall", "wizard/upgrade", "wizard/config"):
            with self.subTest(name=name):
                outer = fpk.read_archive(fixture())
                outer[name] = (0o644, b"[]\n")
                with self.assertRaisesRegex(ValueError, "Empty wizard"):
                    fpk.validate(fpk.write_archive(outer, lambda path: path.startswith("cmd/")))

    def test_package_without_optional_wizards_is_valid(self):
        outer = fpk.read_archive(fixture())
        del outer["wizard/install"]
        del outer["wizard/uninstall"]
        with tempfile.TemporaryDirectory() as directory:
            package = Path(directory) / "app.fpk"
            package.write_bytes(fpk.write_archive(outer, lambda name: name.startswith("cmd/")))
            fpk.normalize(package)
            fpk.validate(package.read_bytes())

    def test_windows_permissions_are_fixed_and_checksum_is_updated(self):
        with tempfile.TemporaryDirectory() as directory:
            package = Path(directory) / "app.fpk"
            package.write_bytes(fixture())
            with self.assertRaisesRegex(ValueError, "Script must be executable"):
                fpk.validate(package.read_bytes())
            fpk.normalize(package)
            fpk.validate(package.read_bytes())
            outer = fpk.read_archive(package.read_bytes())
            inner = fpk.read_archive(outer["app.tgz"][1])
            self.assertEqual(outer["cmd/install_init"][0], 0o755)
            self.assertEqual(inner["bin/server"][0], 0o755)
            self.assertEqual(inner["www/index.html"][0], 0o644)
            self.assertEqual(inner["bin"][0], 0o755)
            # Permission changes to the payload require a different manifest MD5.
            before = fpk.manifest_values(fpk.read_archive(fixture())["manifest"][1])
            after = fpk.manifest_values(outer["manifest"][1])
            self.assertNotEqual(before["checksum"], after["checksum"])

    def test_corrupt_payload_is_rejected(self):
        outer = fpk.read_archive(fixture())
        outer["app.tgz"] = (0o644, b"corrupt")
        with self.assertRaisesRegex(ValueError, "checksum"):
            fpk.validate(fpk.write_archive(outer, lambda name: name.startswith("cmd/")))

    def test_arm_binary_cannot_be_shipped_in_x86_package(self):
        with tempfile.TemporaryDirectory() as directory:
            package = Path(directory) / "app.fpk"
            original = fixture(machine=183)
            package.write_bytes(original)
            with self.assertRaisesRegex(ValueError, "x86_64 ELF"):
                fpk.normalize(package)
            self.assertEqual(package.read_bytes(), original)


if __name__ == "__main__":
    unittest.main()
