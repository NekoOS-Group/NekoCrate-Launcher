"""Cheap packaging checks, before Rust compilation and on the resulting binary."""

import argparse
from html.parser import HTMLParser
from pathlib import Path
import subprocess
import os
import shutil
import tempfile
from urllib.parse import unquote, urlsplit


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


def embedded(target, portable=False):
    root = Path(__file__).resolve().parents[1]
    suffix = ".exe" if "windows" in target else ""
    binary = root / "target" / target / "release" / f"{'nekocrate-portable' if portable else 'nekocrate-launcher'}{suffix}"
    subprocess.run([str(binary), "--verify-frontend"], check=True, timeout=30)
    print(f"Embedded frontend and IPC future size verified: {binary.name}")
    if not portable:
        with tempfile.TemporaryDirectory(prefix="NekoCrate-installed-") as temporary:
            base = Path(temporary)
            relative = Path("Launcher.app/Contents/MacOS/launcher") if "apple" in target else Path(binary.name)
            executable = base / relative
            executable.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(binary, executable)
            (base / ".minecraft").mkdir()
            environment = os.environ.copy()
            environment["NEKOCRATE_DATA_DIR"] = str(base / ".nekocrate")
            subprocess.run([str(executable), "--verify-installed"], check=True, timeout=30, env=environment)
        return
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
    print("Marker-free portable startup, rename, relocation and fresh directory verified")


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
