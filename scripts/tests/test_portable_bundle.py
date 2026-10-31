import json
import os
from pathlib import Path
import stat
import sys
import tempfile
import unittest
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import portable_bundle


class PortableBundleTests(unittest.TestCase):
    def test_portable_archive_has_fresh_marker_and_no_machine_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            executable = root / "launcher"
            executable.write_bytes(b"already compiled executable")
            executable.chmod(0o755)
            path = portable_bundle.package(executable, root / "portable.zip", "NekoCrate.AppImage")
            with zipfile.ZipFile(path) as archive:
                self.assertEqual(json.loads(archive.read("NekoCrate/nekocrate.portable.json")), {"schema": 1})
                self.assertEqual(archive.read("NekoCrate/NekoCrate.AppImage"), executable.read_bytes())
                self.assertTrue(archive.getinfo("NekoCrate/NekoCrate.AppImage").external_attr >> 16 & stat.S_IXUSR)
                self.assertIn("NekoCrate/.minecraft/", archive.namelist())
                self.assertFalse(any("/.nekocrate/" in name for name in archive.namelist()))
                self.assertIsNone(archive.testzip())

    @unittest.skipIf(os.name == "nt", "requires POSIX symlinks")
    def test_mac_bundle_preserves_internal_links_and_rejects_external_ones(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            app = root / "App.app"
            binary = app / "Contents/MacOS/launcher"
            binary.parent.mkdir(parents=True)
            binary.write_bytes(b"signed executable")
            link = app / "current"
            link.symlink_to("Contents/MacOS/launcher")
            path = portable_bundle.package(app, root / "portable.zip", "NekoCrate Portable.app")
            with zipfile.ZipFile(path) as archive:
                name = "NekoCrate/NekoCrate Portable.app/current"
                self.assertEqual(archive.read(name), b"Contents/MacOS/launcher")
                self.assertTrue(stat.S_ISLNK(archive.getinfo(name).external_attr >> 16))
            link.unlink()
            link.symlink_to(root / "outside")
            with self.assertRaisesRegex(ValueError, "leaves application"):
                portable_bundle.package(app, root / "invalid.zip", "NekoCrate Portable.app")


if __name__ == "__main__":
    unittest.main()
