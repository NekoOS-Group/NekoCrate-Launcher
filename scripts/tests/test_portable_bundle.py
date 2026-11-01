import os
from pathlib import Path
import plistlib
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import portable_bundle


class PortableBundleTests(unittest.TestCase):
    def test_windows_and_linux_are_unmodified_single_executables(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "compiled-portable"
            binary.write_bytes(b"already compiled portable executable")
            for platform in ("win-amd64", "linux-amd64"):
                output = portable_bundle.archive_path(root, "dev9.9.9", platform)
                portable_bundle.package(binary, output)
                self.assertEqual(output.read_bytes(), binary.read_bytes())
                if os.name != "nt":
                    self.assertTrue(output.stat().st_mode & 0o111)
            self.assertEqual(len(list((root / "bundle/portable").iterdir())), 2)
            self.assertFalse(list(root.rglob("*.json")))

    @unittest.skipIf(os.name == "nt", "requires POSIX symlinks")
    def test_mac_contains_only_app_with_portable_binary_and_validated_symlinks(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            app = root / "App.app"
            binary = app / "Contents/MacOS/launcher"
            binary.parent.mkdir(parents=True)
            binary.write_bytes(b"installed executable")
            (app / "Contents/Info.plist").write_bytes(plistlib.dumps({"CFBundleExecutable": "launcher"}))
            link = app / "current"
            link.symlink_to("Contents/MacOS/launcher")
            portable = root / "portable"
            portable.write_bytes(b"portable executable")
            destination = root / "portable.app.tar.gz"
            with patch.object(portable_bundle.subprocess, "run") as sign:
                portable_bundle.package(portable, destination, app)
            self.assertEqual(sign.call_count, 2)
            self.assertEqual(binary.read_bytes(), b"installed executable")
            with tarfile.open(destination) as archive:
                self.assertTrue(all(n.startswith("NekoCrate Portable.app") for n in archive.getnames()))
                self.assertEqual(archive.extractfile("NekoCrate Portable.app/Contents/MacOS/launcher").read(), b"portable executable")
                self.assertTrue(archive.getmember("NekoCrate Portable.app/current").issym())
            link.unlink()
            link.symlink_to(root / "outside")
            with self.assertRaisesRegex(ValueError, "leaves application"):
                portable_bundle.package(portable, destination, app)


if __name__ == "__main__":
    unittest.main()
