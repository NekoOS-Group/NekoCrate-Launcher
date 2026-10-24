"""Collect native release bundles and reject mixed or incomplete release assets."""

import argparse
import base64
from datetime import datetime, timezone
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
UPDATERS = {
    "aarch64-apple-darwin": ("darwin-aarch64", ".app.tar.gz", "macos"),
    "x86_64-pc-windows-msvc": ("windows-x86_64", "-setup.exe", "nsis"),
    "x86_64-unknown-linux-gnu": ("linux-x86_64", ".AppImage", "appimage"),
}
RELEASES_URL = "https://github.com/NekoOS-Group/NekoCrate-Launcher/releases/download"


def updater_signature(path, app_version):
    """Reject missing/wrong-key/version signatures before publishing.

    Cryptographic verification is performed by the native updater before install.
    These structural checks also catch signing configuration mistakes in CI.
    """
    signature = path.read_text(encoding="utf-8").strip()
    lines = base64.b64decode(signature, validate=True).decode().splitlines()
    public = read_json(ROOT / "app/src-tauri/tauri.conf.json")["plugins"]["updater"]["pubkey"]
    key_lines = base64.b64decode(public, validate=True).decode().splitlines()
    if len(lines) != 4 or not lines[2].startswith("trusted comment: "):
        raise ValueError("Invalid updater signature format")
    key = base64.b64decode(key_lines[1], validate=True)
    signed = base64.b64decode(lines[1], validate=True)
    if len(key) != 42 or len(signed) != 74 or key[2:10] != signed[2:10]:
        raise ValueError("Updater signing key does not match the embedded public key")
    if f"version:{app_version}" not in lines[2].split():
        raise ValueError("Updater signature is not bound to this release version")
    if len(base64.b64decode(lines[3], validate=True)) != 64:
        raise ValueError("Invalid updater global signature")
    return signature


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def version(tag):
    match = re.fullmatch(r"(dev)(0|[1-9]\d*)\.(0|[1-9]\d*)(?:\.(0|[1-9]\d*))?", tag)
    if not match:
        raise ValueError("Expected a preview tag such as dev0.3 or dev0.3.1")
    channel, major, minor, patch = match.groups()
    expected_version = f"{major}.{minor}.{patch or '0'}-{channel}.1"
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
    updater_target, updater_suffix, updater_directory = UPDATERS[target]
    updater_source = only(bundles / updater_directory, "*" + updater_suffix.removeprefix("-setup"))
    if updater_suffix not in TARGETS[target][1]:
        copy(updater_source, updater_suffix)
    copy(Path(str(updater_source) + ".sig"), updater_suffix + ".sig")
    signature = updater_signature(OUTPUT / f"{prefix}{updater_suffix}.sig", app_version)
    manifest = {
        "tag": tag, "version": app_version, "commit": commit(),
        "platform": platform, "target": target,
        "updater": {"target": updater_target, "file": f"{prefix}{updater_suffix}", "signature": signature},
        "files": [{"name": path.name, "size": path.stat().st_size, "sha256": sha256(path)} for path in assets],
    }
    write_json(OUTPUT / f"build-{platform}.json", manifest)
    print(json.dumps(manifest, ensure_ascii=False))


def verify(tag):
    app_version, revision = version(tag), commit()
    builds, checksums, expected_assets = [], [], set()
    platforms = {}
    for target, (platform, suffixes) in TARGETS.items():
        manifest = read_json(OUTPUT / f"build-{platform}.json")
        expected = {"tag": tag, "version": app_version, "commit": revision, "platform": platform, "target": target}
        if any(manifest.get(key) != value for key, value in expected.items()):
            raise ValueError(f"Build metadata does not match release: {platform}")
        expected_names = {f"NekoCrate-Launcher-{tag}-{platform}{suffix}" for suffix in suffixes}
        updater_target, updater_suffix, _ = UPDATERS[target]
        updater_file = f"NekoCrate-Launcher-{tag}-{platform}{updater_suffix}"
        expected_names.update({updater_file, updater_file + ".sig"})
        if len(manifest["files"]) != len(expected_names) or {item["name"] for item in manifest["files"]} != expected_names:
            raise ValueError(f"Incomplete or unexpected assets: {platform}")
        for item in manifest["files"]:
            path = OUTPUT / item["name"]
            if item["size"] <= 0 or path.stat().st_size != item["size"] or sha256(path) != item["sha256"]:
                raise ValueError(f"Asset checksum mismatch: {path.name}")
            checksums.append(f"{item['sha256']}  {path.name}\n")
        signature = updater_signature(OUTPUT / (updater_file + ".sig"), app_version)
        if manifest.get("updater") != {"target": updater_target, "file": updater_file, "signature": signature}:
            raise ValueError(f"Updater metadata does not match release: {platform}")
        platforms[updater_target] = {"signature": signature, "url": f"{RELEASES_URL}/{tag}/{updater_file}"}
        expected_assets.update(expected_names)
        builds.append(manifest)
    if {path.name for path in OUTPUT.glob("NekoCrate-Launcher-*")} != expected_assets:
        raise ValueError("Unexpected assets in release directory")
    write_json(OUTPUT / "latest.json", {
        "version": app_version,
        "notes": (ROOT / "docs/releases" / f"{tag}.md").read_text(encoding="utf-8"),
        "pub_date": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "platforms": platforms,
    })
    checksums.append(f"{sha256(OUTPUT / 'latest.json')}  latest.json\n")
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
