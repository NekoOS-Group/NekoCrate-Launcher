"""Save matching diagnostic symbols, then shrink the executable before signing.

This deliberately runs after Cargo and before both installed/portable packaging.
Do not use Cargo's `strip` option: that would discard symbols before we save them.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

TARGETS = {
    "aarch64-apple-darwin": "macos",
    "x86_64-pc-windows-msvc": "windows",
    "x86_64-unknown-linux-gnu": "linux",
}


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def run(*command):
    subprocess.run([str(part) for part in command], check=True)


def prepare(binary, target, output, revision):
    platform = TARGETS[target]
    binary = binary.resolve(strict=True)
    output = output.resolve()
    manifest = output / "symbols.json"
    if manifest.exists():
        report = json.loads(manifest.read_text(encoding="utf-8"))
        if (report["target"] == target and report["revision"] == revision
                and report["shipped_sha256"] == digest(binary)
                and all((output / item["name"]).is_file()
                        and digest(output / item["name"]) == item["sha256"]
                        for item in report["symbols"])):
            return report
        raise ValueError("Symbol output belongs to a different build; use a fresh output directory")
    output.mkdir(parents=True, exist_ok=True)
    original_size = binary.stat().st_size
    original_hash = digest(binary)
    symbols = []

    # Work on a copy, then replace. Cargo may hard-link the top-level executable
    # to deps/: stripping it in place would also corrupt the cached build input.
    with tempfile.TemporaryDirectory(prefix="nekocrate-symbols-", dir=binary.parent) as temporary:
        staged = Path(temporary) / binary.name
        shutil.copy2(binary, staged)
        if platform == "macos":
            saved = output / (binary.name + ".unstripped")
            shutil.copy2(binary, saved)
            symbols.append(saved)
            run("xcrun", "strip", "-x", staged)
            # Mach-O stripping invalidates its linker-generated ad hoc signature.
            # Restore it for smoke checks; app packaging performs its own signing.
            run("codesign", "--force", "--sign", "-", staged)
            run("codesign", "--verify", "--strict", staged)
        elif platform == "linux":
            saved = output / (binary.name + ".debug")
            run("objcopy", "--only-keep-debug", binary, saved)
            symbols.append(saved)
            # Keep dynamic symbols, relocation/unwind information and build ID.
            run("strip", "--strip-unneeded", staged)
            run("objcopy", f"--add-gnu-debuglink={saved}", staged)
        else:
            # MSVC already puts symbols in a separate PDB. Never strip the PE's
            # exception tables or resources; they are needed at runtime.
            # rustc normalizes crate names to underscores; accept the literal
            # binary stem too for toolchains that retain it.
            candidates = {binary.with_suffix(".pdb"),
                          binary.with_name(binary.stem.replace("-", "_") + ".pdb")}
            matches = [path for path in candidates if path.is_file() and path.stat().st_size]
            if len(matches) != 1:
                raise ValueError("Missing release PDB; build Windows with CARGO_PROFILE_RELEASE_DEBUG=1")
            pdb = matches[0]
            saved = output / pdb.name
            shutil.copy2(pdb, saved)
            symbols.append(saved)
        if any(not path.stat().st_size for path in symbols):
            raise ValueError("Empty diagnostic symbol file")
        if platform != "windows":
            os.replace(staged, binary)

    report = {
        "schema": 1,
        "target": target,
        "revision": revision,
        "binary": binary.name,
        "original_size": original_size,
        "original_sha256": original_hash,
        "shipped_size": binary.stat().st_size,
        "shipped_sha256": digest(binary),
        "symbols": [{"name": path.name, "size": path.stat().st_size,
                     "sha256": digest(path)} for path in symbols],
    }
    manifest.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True, choices=TARGETS)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    suffix = ".exe" if TARGETS[args.target] == "windows" else ""
    binary = root / "target" / args.target / "release" / f"nekocrate-launcher{suffix}"
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    report = prepare(binary, args.target, root / "artifacts/symbols" / args.target, revision)
    print(f"{args.target}: {report['original_size']:,} -> {report['shipped_size']:,} bytes")


if __name__ == "__main__":
    main()
