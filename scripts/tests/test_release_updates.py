import base64
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import release_assets as release

FIXTURE = Path(__file__).resolve().parents[2] / "app/src-tauri/tests/fixtures/updater"


class ReleaseUpdateTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.output = self.root / "artifacts"
        self.output.mkdir()
        config = self.root / "app/src-tauri/tauri.conf.json"
        config.parent.mkdir(parents=True)
        config.write_text(json.dumps({"plugins": {"updater": {"pubkey": (FIXTURE / "test.pub").read_text().strip()}}}))
        notes = self.root / "docs/releases/dev9.9.9.md"
        notes.parent.mkdir(parents=True)
        notes.write_text("Signed updater fixture")
        for name, value in [("ROOT", self.root), ("OUTPUT", self.output)]:
            patcher = patch.object(release, name, value)
            patcher.start()
            self.addCleanup(patcher.stop)

    def test_signature_must_match_the_embedded_key_and_version(self):
        signature = FIXTURE / "test.app.tar.gz.sig"
        release.updater_signature(signature, "9.9.9")
        with self.assertRaisesRegex(ValueError, "version"):
            release.updater_signature(signature, "9.9.10")
        altered = base64.b64decode(signature.read_text()).decode().splitlines()
        raw = bytearray(base64.b64decode(altered[1]))
        raw[2] ^= 1
        altered[1] = base64.b64encode(raw).decode()
        wrong = self.root / "wrong.sig"
        wrong.write_text(base64.b64encode("\n".join(altered).encode()).decode())
        with self.assertRaisesRegex(ValueError, "key"):
            release.updater_signature(wrong, "9.9.9")

    def test_complete_release_generates_platform_manifest_and_rejects_missing_signature(self):
        for target, (platform, suffixes) in release.TARGETS.items():
            updater_target, updater_suffix, _ = release.UPDATERS[target]
            prefix = f"NekoCrate-Launcher-dev9.9.9-{platform}"
            files = []
            for suffix in set(suffixes) | {updater_suffix, updater_suffix + ".sig"}:
                path = self.output / (prefix + suffix)
                source = FIXTURE / ("test.app.tar.gz.sig" if suffix.endswith(".sig") else "test.app.tar.gz")
                path.write_bytes(source.read_bytes())
                files.append({"name": path.name, "size": path.stat().st_size, "sha256": release.sha256(path)})
            manifest = {
                "tag": "dev9.9.9", "version": "9.9.9", "commit": "fixture", "platform": platform, "target": target,
                "files": files, "updater": {"target": updater_target, "file": prefix + updater_suffix, "signature": (FIXTURE / "test.app.tar.gz.sig").read_text().strip()},
            }
            release.write_json(self.output / f"build-{platform}.json", manifest)
        with patch.object(release, "version", return_value="9.9.9"), patch.object(release, "commit", return_value="fixture"):
            release.verify("dev9.9.9")
            manifest = release.read_json(self.output / "latest.json")
            self.assertEqual(set(manifest["platforms"]), {"darwin-aarch64", "windows-x86_64", "linux-x86_64"})
            self.assertEqual(manifest["version"], "9.9.9")
            for platform in manifest["platforms"].values():
                self.assertTrue(platform["url"].startswith(release.RELEASES_URL + "/dev9.9.9/"))
            self.assertIn("latest.json", (self.output / "SHA256SUMS").read_text())
            signature = next(self.output.glob("*.sig"))
            signature.write_text("corrupt")
            with self.assertRaisesRegex(ValueError, "checksum"):
                release.verify("dev9.9.9")
