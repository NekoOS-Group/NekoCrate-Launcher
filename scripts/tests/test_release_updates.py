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
        for name, value in [("version", "9.9.9"), ("commit", "fixture")]:
            patcher = patch.object(release, name, return_value=value)
            patcher.start()
            self.addCleanup(patcher.stop)

    def signature(self):
        return (FIXTURE / "test.app.tar.gz.sig").read_text().strip()

    def test_signature_must_match_the_embedded_key_and_version(self):
        signature = self.signature()
        release.updater_signature(signature, "9.9.9")
        with self.assertRaisesRegex(ValueError, "version"):
            release.updater_signature(signature, "9.9.10")
        altered = base64.b64decode(signature).decode().splitlines()
        raw = bytearray(base64.b64decode(altered[1]))
        raw[2] ^= 1
        altered[1] = base64.b64encode(raw).decode()
        wrong = base64.b64encode("\n".join(altered).encode()).decode()
        with self.assertRaisesRegex(ValueError, "key"):
            release.updater_signature(wrong, "9.9.9")

    def make_platform_manifests(self):
        for target, (platform, suffixes) in release.TARGETS.items():
            updater_target, updater_suffix, _ = release.UPDATERS[target]
            prefix = f"NekoCrate-Launcher-dev9.9.9-{platform}"
            files = []
            for suffix in set(suffixes) | {updater_suffix}:
                path = self.output / (prefix + suffix)
                path.write_bytes((FIXTURE / "test.app.tar.gz").read_bytes())
                files.append(release.file_info(path))
            manifest = {
                "tag": "dev9.9.9", "version": "9.9.9", "commit": "fixture", "platform": platform, "target": target,
                "files": files, "updater": {"target": updater_target, "file": prefix + updater_suffix, "signature": self.signature()},
            }
            if platform == "win-amd64":
                manifest["portable_updater"] = {"target": "windows-x86_64-portable", "file": prefix + "-portable.zip", "signature": self.signature()}
            release.write_json(self.output / f"build-{platform}.json", manifest)

    def test_collect_embeds_native_signature_without_copying_it_or_creating_app_zip(self):
        target = "aarch64-apple-darwin"
        bundles = self.root / "target" / target / "release/bundle"
        app = bundles / "macos/Test.app"
        app.mkdir(parents=True)
        (bundles / "dmg").mkdir()
        (bundles / "dmg/Test.dmg").write_bytes(b"dmg fixture")
        (bundles / "macos/Test.app.tar.gz").write_bytes((FIXTURE / "test.app.tar.gz").read_bytes())
        portable = release.portable_bundle.archive_path(bundles.parent, "dev9.9.9", "mac-appsilicon")
        portable.parent.mkdir()
        portable.write_bytes(b"portable fixture")
        signature = bundles / "macos/Test.app.tar.gz.sig"
        signature.write_text(self.signature())
        with patch.object(release, "verify_architecture"), patch.object(release.subprocess, "run") as command:
            release.collect("dev9.9.9", target)
            command.assert_called_once_with(["codesign", "--verify", "--deep", "--strict", str(app)], check=True)
        self.assertEqual({p.name for p in self.output.iterdir()}, {
            "build-mac-appsilicon.json", "NekoCrate-Launcher-dev9.9.9-mac-appsilicon.dmg",
            "NekoCrate-Launcher-dev9.9.9-mac-appsilicon.app.tar.gz",
            "NekoCrate-Launcher-dev9.9.9-mac-appsilicon-portable.zip",
        })
        manifest = release.read_json(self.output / "build-mac-appsilicon.json")
        self.assertEqual(manifest["updater"]["signature"], self.signature())
        self.assertTrue(signature.is_file())

    def test_complete_release_has_one_checksum_list_and_keeps_all_updater_signatures(self):
        self.make_platform_manifests()
        release.verify("dev9.9.9")
        manifest = release.read_json(self.output / "latest.json")
        self.assertEqual(set(manifest["platforms"]), {"darwin-aarch64", "windows-x86_64", "linux-x86_64", "windows-x86_64-portable"})
        self.assertEqual(manifest["version"], "9.9.9")
        for platform in manifest["platforms"].values():
            self.assertTrue(platform["url"].startswith(release.RELEASES_URL + "/dev9.9.9/"))
            self.assertEqual(platform["signature"], self.signature())
        info = release.read_json(self.output / "BUILDINFO.json")
        self.assertEqual(info["schema_version"], 2)
        self.assertEqual(info["commit"], "fixture")
        self.assertEqual(len(info["files"]), 9)  # Eight packages plus latest.json.
        self.assertEqual({item["name"] for item in info["files"]}, {
            p.name for p in self.output.iterdir() if p.name.startswith("NekoCrate-")
        } | {"latest.json"})
        for item in info["files"]:
            self.assertEqual(item, release.file_info(self.output / item["name"]))
        self.assertEqual({name for build in info["builds"] for name in build["files"]},
                         {item["name"] for item in info["files"]} - {"latest.json"})
        self.assertFalse((self.output / "SHA256SUMS").exists())
        self.assertFalse(list(self.output.glob("*.sig")))
        self.assertFalse(list(self.output.glob("*.app.zip")))

    def test_corrupt_package_is_rejected(self):
        self.make_platform_manifests()
        next(self.output.glob("*.dmg")).write_bytes(b"corrupt")
        with self.assertRaisesRegex(ValueError, "checksum"):
            release.verify("dev9.9.9")

    def test_missing_or_invalid_embedded_signature_is_rejected(self):
        self.make_platform_manifests()
        path = self.output / "build-mac-appsilicon.json"
        info = release.read_json(path)
        for signature in (None, "", "invalid"):
            with self.subTest(signature=signature):
                info["updater"]["signature"] = signature
                release.write_json(path, info)
                with self.assertRaises(ValueError):
                    release.verify("dev9.9.9")

    def test_stale_legacy_attachments_are_not_silently_published(self):
        self.make_platform_manifests()
        for name in ("SHA256SUMS", "NekoCrate-Launcher-dev9.9.9-mac-appsilicon.app.zip",
                     "NekoCrate-Launcher-dev9.9.9-mac-appsilicon.app.tar.gz.sig"):
            with self.subTest(name=name):
                stale = self.output / name
                stale.write_text("old attachment")
                with self.assertRaisesRegex(ValueError, "Unexpected assets"):
                    release.verify("dev9.9.9")
                stale.unlink()

    def test_wrong_commit_cannot_enter_the_release(self):
        self.make_platform_manifests()
        path = self.output / "build-win-amd64.json"
        info = release.read_json(path)
        info["commit"] = "different source"
        release.write_json(path, info)
        with self.assertRaisesRegex(ValueError, "metadata"):
            release.verify("dev9.9.9")
