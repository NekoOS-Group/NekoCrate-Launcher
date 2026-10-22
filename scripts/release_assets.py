"""Collect native release bundles and reject mixed or incomplete release assets."""

import argparse
import hashlib
import json
import re
import shutil
import struct
import subprocess
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "artifacts" / "release"
TARGETS = {
    "aarch64-apple-darwin": ("mac-appsilicon", [".dmg", ".app.zip"]),
    "x86_64-pc-windows-msvc": ("win-amd64", ["-setup.exe"]),
    "x86_64-unknown-linux-gnu": ("linux-amd64", [".deb", ".AppImage"]),
}


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def version(tag):
    match = re.fullmatch(r"(dev)(0|[1-9]\d*)\.(0|[1-9]\d*)", tag)
    if not match:
        raise ValueError("Expected a preview tag such as dev0.1 or dev0.2")
    channel, major, minor = match.groups()
    expected_version = f"{major}.{minor}.0-{channel}.1"
    config = read_json(ROOT / "app/src-tauri/tauri.conf.json")
    package = read_json(ROOT / "app/package.json")
    lock = read_json(ROOT / "app/package-lock.json")
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    versions = {config["version"], package["version"], lock["version"],
                lock["packages"][""]["version"], cargo["workspace"]["package"]["version"]}
    if len(versions) != 1:
        raise ValueError(f"Package versions disagree: {versions}")
    app_version = versions.pop()
    if app_version != expected_version:
        raise ValueError(f"Tag {tag} requires {expected_version}, found {app_version}")
    if not (ROOT / "docs/releases" / f"{tag}.md").is_file():
        raise ValueError("Missing release notes")
    return app_version


def commit():
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()


def sha256(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def only(directory, pattern):
    matches = list(directory.glob(pattern))
    if len(matches) != 1:
        raise ValueError(f"Expected one {pattern} in {directory}, found {len(matches)}")
    return matches[0]


def verify_architecture(path, target):
    with path.open("rb") as binary:
        header = binary.read(64)
        if target == "aarch64-apple-darwin":
            valid = header[:4] == b"\xcf\xfa\xed\xfe" and struct.unpack_from("<I", header, 4)[0] == 0x0100000C
        elif target == "x86_64-unknown-linux-gnu":
            valid = header[:6] == b"\x7fELF\x02\x01" and struct.unpack_from("<H", header, 18)[0] == 62
        else:
            valid = False
            if header[:2] == b"MZ":
                binary.seek(struct.unpack_from("<I", header, 60)[0])
                valid = binary.read(6) == b"PE\x00\x00\x64\x86"
    if not valid:
        raise ValueError(f"Wrong executable architecture: {path} (expected {target})")


def collect(tag, target):
    app_version = version(tag)
    platform, _ = TARGETS[target]
    release = ROOT / "target" / target / "release"
    bundles = release / "bundle"
    OUTPUT.mkdir(parents=True, exist_ok=True)
    prefix = f"NekoCrate-Launcher-{tag}-{platform}"
    assets = []

    def copy(source, suffix):
        destination = OUTPUT / f"{prefix}{suffix}"
        shutil.copy2(source, destination)
        assets.append(destination)

    if platform == "mac-appsilicon":
        app = only(bundles / "macos", "*.app")
        verify_architecture(app / "Contents/MacOS/nekocrate-launcher", target)
        subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], check=True)
        copy(only(bundles / "dmg", "*.dmg"), ".dmg")
        archive = OUTPUT / f"{prefix}.app.zip"
        if archive.exists():
            archive.unlink()
        subprocess.run(["ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", str(app), str(archive)], check=True)
        assets.append(archive)
    elif platform == "win-amd64":
        verify_architecture(release / "nekocrate-launcher.exe", target)
        copy(only(bundles / "nsis", "*.exe"), "-setup.exe")
    else:
        verify_architecture(release / "nekocrate-launcher", target)
        copy(only(bundles / "deb", "*.deb"), ".deb")
        copy(only(bundles / "appimage", "*.AppImage"), ".AppImage")
    manifest = {
        "tag": tag, "version": app_version, "commit": commit(),
        "platform": platform, "target": target,
        "files": [{"name": path.name, "size": path.stat().st_size, "sha256": sha256(path)} for path in assets],
    }
    write_json(OUTPUT / f"build-{platform}.json", manifest)
    print(json.dumps(manifest, ensure_ascii=False))


def verify(tag):
    app_version, revision = version(tag), commit()
    builds, checksums, expected_assets = [], [], set()
    for target, (platform, suffixes) in TARGETS.items():
        manifest = read_json(OUTPUT / f"build-{platform}.json")
        expected = {"tag": tag, "version": app_version, "commit": revision, "platform": platform, "target": target}
        if any(manifest.get(key) != value for key, value in expected.items()):
            raise ValueError(f"Build metadata does not match release: {platform}")
        expected_names = {f"NekoCrate-Launcher-{tag}-{platform}{suffix}" for suffix in suffixes}
        if len(manifest["files"]) != len(expected_names) or {item["name"] for item in manifest["files"]} != expected_names:
            raise ValueError(f"Incomplete or unexpected assets: {platform}")
        for item in manifest["files"]:
            path = OUTPUT / item["name"]
            if item["size"] <= 0 or path.stat().st_size != item["size"] or sha256(path) != item["sha256"]:
                raise ValueError(f"Asset checksum mismatch: {path.name}")
            checksums.append(f"{item['sha256']}  {path.name}\n")
        expected_assets.update(expected_names)
        builds.append(manifest)
    if {path.name for path in OUTPUT.glob("NekoCrate-Launcher-*")} != expected_assets:
        raise ValueError("Unexpected assets in release directory")
    (OUTPUT / "SHA256SUMS").write_text("".join(sorted(checksums)), encoding="utf-8")
    write_json(OUTPUT / "BUILDINFO.json", {"tag": tag, "version": app_version, "commit": revision, "builds": builds})
    print(f"Verified {len(expected_assets)} assets from all three platforms at {revision}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=["check-version", "collect", "verify"])
    parser.add_argument("--tag", required=True)
    parser.add_argument("--target", choices=TARGETS)
    args = parser.parse_args()
    if args.operation == "collect":
        if args.target is None:
            parser.error("collect requires --target")
        collect(args.tag, args.target)
    elif args.operation == "verify":
        verify(args.tag)
    else:
        print(f"{args.tag}: {version(args.tag)}")
