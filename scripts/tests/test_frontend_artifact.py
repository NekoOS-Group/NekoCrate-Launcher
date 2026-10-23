import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from frontend_artifact import verify


class FrontendArtifactTests(unittest.TestCase):
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
