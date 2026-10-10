import json
from pathlib import Path
import tempfile
import unittest

from check_version import check


class ReleaseVersionTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / "fpk").mkdir()
        (self.root / "web").mkdir()
        (self.root / "Cargo.toml").write_text('[package]\nversion = "0.2.1"\n')
        (self.root / "fpk/manifest").write_text('version=0.2.1\n')
        (self.root / "web/package.json").write_text(json.dumps({"version": "0.2.1"}))
        (self.root / "web/package-lock.json").write_text(json.dumps(
            {"version": "0.2.1", "packages": {"": {"version": "0.2.1"}}}))

    def test_matching_tag(self):
        self.assertEqual(check(self.root, "v0.2.1"), "0.2.1")

    def test_wrong_tag_blocks_release(self):
        with self.assertRaisesRegex(ValueError, "Release tag"):
            check(self.root, "v0.2.2")

    def test_stale_manifest_blocks_release(self):
        (self.root / "fpk/manifest").write_text('version=0.2.0\n')
        with self.assertRaisesRegex(ValueError, "fpk/manifest"):
            check(self.root)

    def test_stale_lock_root_blocks_release(self):
        (self.root / "web/package-lock.json").write_text(json.dumps(
            {"version": "0.2.1", "packages": {"": {"version": "0.2.0"}}}))
        with self.assertRaisesRegex(ValueError, "packages root"):
            check(self.root)


if __name__ == "__main__":
    unittest.main()
