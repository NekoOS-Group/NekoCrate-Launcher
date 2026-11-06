import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from frontend_artifact import verify, verify_compressed_window_icon


class FrontendArtifactTests(unittest.TestCase):
    def test_rejects_an_icon_left_embedded_after_runtime_replacement(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "nekocrate-launcher"
            binary.write_bytes(b"native-code-compressed-icon")
            with self.assertRaisesRegex(ValueError, "Missing build-time"):
                verify_compressed_window_icon(binary)
            reference = root / "build/nekocrate-desktop-fixture/out/window-icon.rgba"
            reference.parent.mkdir(parents=True)
            reference.write_bytes(bytes(range(256)))
            verify_compressed_window_icon(binary)
            binary.write_bytes(binary.read_bytes() + reference.read_bytes())
            with self.assertRaisesRegex(ValueError, "Uncompressed window icon"):
                verify_compressed_window_icon(binary)

    def test_rejects_dev031_nested_artifact(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            nested = root / "ci-frontend"
            nested.mkdir()
            (nested / "index.html").write_text("<html></html>")
            with self.assertRaisesRegex(ValueError, "merge-multiple"):
                verify(root)

    def test_accepts_built_entry_and_rejects_missing_resource(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "assets").mkdir()
            (root / "index.html").write_text('<script src="/assets/main.js"></script><link rel="stylesheet" href="/assets/main.css">')
            (root / "assets/main.js").write_text("export {}")
            (root / "assets/main.css").write_text("body {}")
            verify(root)
            (root / "assets/main.css").unlink()
            with self.assertRaisesRegex(ValueError, "main.css"):
                verify(root)
