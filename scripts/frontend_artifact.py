"""Cheap packaging checks, before Rust compilation and on the resulting binary."""

import argparse
from html.parser import HTMLParser
from pathlib import Path
import subprocess
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


def embedded(target):
    root = Path(__file__).resolve().parents[1]
    suffix = ".exe" if "windows" in target else ""
    binary = root / "target" / target / "release" / f"nekocrate-launcher{suffix}"
    subprocess.run([str(binary), "--verify-frontend"], check=True, timeout=30)
    print(f"Embedded frontend verified: {binary.name}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=["verify", "embedded"])
    parser.add_argument("directory", nargs="?")
    parser.add_argument("--target")
    args = parser.parse_args()
    if args.operation == "verify":
        if not args.directory:
            parser.error("verify requires a frontend directory")
        verify(args.directory)
    else:
        if not args.target:
            parser.error("embedded requires --target")
        embedded(args.target)
