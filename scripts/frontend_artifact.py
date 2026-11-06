"""Cheap packaging checks, before Rust compilation and on the resulting binary."""

import argparse
from html.parser import HTMLParser
from pathlib import Path
import subprocess
import os
import shutil
import tempfile
from urllib.parse import unquote, urlsplit
from portable_bundle import RUNTIME_CONFIG


class EntryAssets(HTMLParser):
    def __init__(self):
        super().__init__()
        self.references = []

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "script" and attrs.get("src"):
            self.references.append(attrs["src"])
        if tag == "link" and attrs.get("rel") in {"stylesheet", "modulepreload"}:
            self.references.append(attrs.get("href", ""))


def verify(directory):
    root = Path(directory).resolve()
    entry = root / "index.html"
    if not entry.is_file():
        raise ValueError(f"Missing {entry}; extract ci-frontend directly into dist (merge-multiple: true)")
    parser = EntryAssets()
    parser.feed(entry.read_text(encoding="utf-8"))
    if not parser.references:
        raise ValueError("index.html has no built frontend resources")
    for reference in parser.references:
        url = urlsplit(reference)
        if url.scheme or url.netloc:
            raise ValueError(f"Expected a bundled frontend resource: {reference}")
        resource = (root / unquote(url.path).lstrip("/")).resolve()
        if not resource.is_relative_to(root) or not resource.is_file() or not resource.stat().st_size:
            raise ValueError(f"Missing or invalid frontend resource: {reference}")
    print(f"Frontend entry and {len(parser.references)} resources verified in {root}")


def verify_compressed_window_icon(binary):
    # The full-resolution icon must be embedded only as compressed data.
    references = list((binary.parent / "build").glob("nekocrate-desktop-*/out/window-icon.rgba"))
    if not references:
        raise ValueError("Missing build-time window icon reference")
    executable = binary.read_bytes()
    for reference in references:
        rgba = reference.read_bytes()
        if rgba and rgba in executable:
            raise ValueError("Uncompressed window icon is still embedded in the executable")


def embedded(target, portable=False):
    root = Path(__file__).resolve().parents[1]
    suffix = ".exe" if "windows" in target else ""
    binary = root / "target" / target / "release" / f"nekocrate-launcher{suffix}"
    if "windows" not in target:
        verify_compressed_window_icon(binary)
    if "linux" in target:
        dynamic = subprocess.check_output(["readelf", "-d", str(binary)], text=True)
        if "Shared library: [libdbus-1.so.3]" not in dynamic:
            raise ValueError("Linux releases must use the system libdbus shared library")
    subprocess.run([str(binary), "--verify-frontend"], check=True, timeout=30)
    print(f"Embedded frontend and IPC future size verified: {binary.name}")
    if not portable:
        with tempfile.TemporaryDirectory(prefix="NekoCrate-installed-") as temporary:
            base = Path(temporary)
            if "apple" in target:
                relative = Path("Launcher.app/Contents/MacOS/launcher")
                configuration = base / "Launcher.app/Contents/Resources" / RUNTIME_CONFIG
            elif "linux" in target:
                relative = Path("usr/bin/nekocrate-launcher")
                configuration = base / "usr/lib/nekocrate-launcher" / RUNTIME_CONFIG
            else:
                relative = Path(binary.name)
                configuration = base / RUNTIME_CONFIG
            executable = base / relative
            executable.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(binary, executable)
            configuration.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(root / "app/src-tauri/resources" / RUNTIME_CONFIG, configuration)
            (base / ".minecraft").mkdir()
            environment = os.environ.copy()
            environment["NEKOCRATE_DATA_DIR"] = str(base / "system-data")
            subprocess.run([str(executable), "--verify-installed"], check=True, timeout=30, env=environment)
            assert not (base / ".nekocrate").exists()
            # The same bytes switch mode solely by removing the configuration.
            configuration.unlink()
            environment.pop("NEKOCRATE_DATA_DIR", None)
            subprocess.run([str(executable), "--verify-portable"], check=True, timeout=30, env=environment)
    with tempfile.TemporaryDirectory(prefix="NekoCrate-portable-") as temporary:
        base = Path(temporary) / "NekoCrate 测试"
        base.mkdir()
        relative = Path("NekoCrate Portable.app/Contents/MacOS/nekocrate-launcher") if "apple" in target else Path(binary.name)
        executable = base / relative
        executable.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(binary, executable)
        environment = os.environ.copy()
        environment.pop("NEKOCRATE_DATA_DIR", None)
        subprocess.run([str(executable), "--verify-portable"], check=True, timeout=30, env=environment)
        identity = (base / ".nekocrate/portable.json").read_bytes()
        moved = base.with_name("NekoCrate moved 测试")
        base.rename(moved)
        subprocess.run([str(moved / relative), "--verify-portable"], check=True, timeout=30, env=environment)
        assert (moved / ".nekocrate/portable.json").read_bytes() == identity
        # Moving only the application into a new folder selects that new folder.
        fresh = Path(temporary) / "Fresh 游戏"
        renamed = Path("User renamed.app/Contents/MacOS/launcher") if "apple" in target else Path("User renamed" + suffix)
        another = fresh / renamed
        another.parent.mkdir(parents=True)
        shutil.copy2(binary, another)
        subprocess.run([str(another), "--verify-portable"], check=True, timeout=30, env=environment)
        assert (fresh / ".nekocrate/portable.json").read_bytes() != identity
        assert not (fresh / "nekocrate.portable.json").exists()
    print("Shared executable: configuration-selected mode, portable rename, relocation and fresh directory verified")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=["verify", "embedded"])
    parser.add_argument("directory", nargs="?")
    parser.add_argument("--target")
    parser.add_argument("--portable", action="store_true")
    args = parser.parse_args()
    if args.operation == "verify":
        if not args.directory:
            parser.error("verify requires a frontend directory")
        verify(args.directory)
    else:
        if not args.target:
            parser.error("embedded requires --target")
        embedded(args.target, args.portable)
