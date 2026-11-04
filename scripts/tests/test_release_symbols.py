import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from release_symbols import desktop_binary_name, digest, prepare


class ReleaseSymbolsTests(unittest.TestCase):
    def test_renamed_windows_executable_uses_cargo_target_pdb(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "nekocrate-launcher.exe"
            binary.write_bytes(b"PE fixture")
            (root / "nekocrate_desktop.pdb").write_bytes(b"matching PDB")
            (root / "nekocrate_launcher.pdb").write_bytes(b"stale PDB")
            report = prepare(binary, "x86_64-pc-windows-msvc", root / "symbols", "revision",
                             cargo_binary_name="nekocrate-desktop")
            self.assertEqual(report["original_sha256"], report["shipped_sha256"])
            self.assertEqual(report["symbols"][0]["name"], "nekocrate_desktop.pdb")
            self.assertEqual((root / "symbols/nekocrate_desktop.pdb").read_bytes(), b"matching PDB")

    def test_desktop_binary_target_comes_from_cargo_metadata(self):
        root = Path(__file__).resolve().parents[2]
        self.assertEqual(desktop_binary_name(root), "nekocrate-desktop")

    def test_windows_keeps_executable_bytes_and_saves_pdb(self):
        for name in ("nekocrate-launcher.pdb", "nekocrate_launcher.pdb"):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                binary = root / "nekocrate-launcher.exe"
                binary.write_bytes(b"PE fixture")
                (root / name).write_bytes(b"PDB fixture")
                with patch("release_symbols.run") as command:
                    report = prepare(binary, "x86_64-pc-windows-msvc", root / "symbols", "revision")
                    command.assert_not_called()
                self.assertEqual(report["original_sha256"], report["shipped_sha256"])
                self.assertEqual((root / "symbols" / name).read_bytes(), b"PDB fixture")

    def test_missing_pdb_does_not_modify_executable(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "launcher.exe"
            binary.write_bytes(b"unchanged")
            with self.assertRaisesRegex(ValueError, "Missing release PDB"):
                prepare(binary, "x86_64-pc-windows-msvc", root / "symbols", "revision")
            self.assertEqual(binary.read_bytes(), b"unchanged")

    @unittest.skipUnless(sys.platform in ("darwin", "linux") and shutil.which("cc"), "native compiler required")
    def test_native_strip_keeps_program_and_cached_hardlink_working(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "main.c"
            source.write_text("__attribute__((noinline)) static int diagnostic_function(void) { return 0; }\n"
                              "int main(void) { return diagnostic_function(); }\n")
            binary = root / "launcher"
            subprocess.run(["cc", "-O0", str(source), "-o", str(binary)], check=True)
            cached = root / "cached"
            os.link(binary, cached)
            original = digest(cached)
            output = root / "symbols"
            target = "aarch64-apple-darwin" if sys.platform == "darwin" else "x86_64-unknown-linux-gnu"
            report = prepare(binary, target, output, "revision")
            self.assertEqual(digest(cached), original)
            self.assertEqual(report["original_sha256"], original)
            self.assertEqual(report["shipped_sha256"], digest(binary))
            subprocess.run([str(binary)], check=True)
            symbols = subprocess.check_output(["nm", str(output / report["symbols"][0]["name"])], text=True)
            self.assertIn("diagnostic_function", symbols)
            self.assertEqual(prepare(binary, target, output, "revision"), report)
            with self.assertRaisesRegex(ValueError, "different build"):
                prepare(binary, target, output, "different-revision")
            (output / report["symbols"][0]["name"]).unlink()
            with self.assertRaisesRegex(ValueError, "different build"):
                prepare(binary, target, output, "revision")

    def test_tool_failure_leaves_original_intact(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "launcher"
            binary.write_bytes(b"unchanged")
            with patch("release_symbols.run", side_effect=subprocess.CalledProcessError(1, "strip")):
                with self.assertRaises(subprocess.CalledProcessError):
                    prepare(binary, "aarch64-apple-darwin", root / "symbols", "revision")
            self.assertEqual(binary.read_bytes(), b"unchanged")
            self.assertFalse((root / "symbols/symbols.json").exists())
