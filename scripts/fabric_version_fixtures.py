#!/usr/bin/env python3
"""Regenerate Rust conformance fixtures against a real Fabric Loader JAR.
Usage: python3 scripts/fabric_version_fixtures.py --java /path/to/java --loader /path/to/fabric-loader.jar
Requires JDK 11+ only when regenerating; cargo tests use the checked-in results.
"""
import argparse
import base64
import hashlib
import itertools
import json
from pathlib import Path
import subprocess
import zipfile

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "crates/nekocrate-core/tests/fixtures"
VERSIONS = [
    "", "0", "0.0", "0.1", "0.2.3", "0.9", "1", "1.0", "1.0.0", "1.0.1",
    "1.1", "1.2", "1.2.0", "1.2.3", "1.2.3.0", "1.2.3.4", "1.2.4-", "1.2.4-alpha",
    "1.2.4", "1.3-", "1.3-alpha", "1.3", "1.9", "2-", "2-alpha", "2", "2.1",
    "1.20.4", "1.20.5-", "1.20.5-0", "1.20.5-alpha", "1.20.5", "1.21", "1.21.1",
    "1.22-rc.1", "1.22", "17", "17.0.1", "21", "21.0.13+9-LTS", "26",
    "1-", "1-0", "1-00", "1-01", "1-1", "1-2", "1-10", "1-a", "1-a.1", "1-a.2", "1-a.10",
    "1-99999999999999999999999", "1-100000000000000000000000", "1+abc", "1+", "1+abc..xyz",
    "01.02.003", "2147483647", "2147483648", "1.2147483647.1", "snapshot", "25w01a", "custom",
    "x", "X", "*", "x.x", "1.x", "1.x.2", "1.x-alpha", "1..2", "v1.2.3", "1.2-rc..1",
    "1\t2", "1,2", "1.2.3.4.5.6", "1.2.3.4.5.7", "1.2.3.4.6-", "1.2.2147483647.1",
]
BOUNDS = [
    "0", "0.2.3", "1", "1.0", "1.2", "1.2.3", "1.2.3.4", "1.2.3.4.5.6",
    "1.20.5-", "1.21", "21", "1-", "1-0", "1-01", "1-a.2", "1-a.10",
    "1+abc", "01.02.003", "2147483648", "snapshot", "25w01a", "custom",
    "x", "X", "*", "x.x", "1.x", "1.X", "1.*", "1.x.x", "1.2.x", "1.2.3.x", "1.2.3.4.5.x",
    "1.2.2147483647.x", "1.x.2", "1.x-alpha", "1.x+build", "1..2", "1.2-rc..1",
]
PREDICATES = [op + bound for op, bound in itertools.product(["", "=", ">=", "<=", ">", "<", "~", "^"], BOUNDS)]
PREDICATES += ["", " ", "* *", " >1 <2 ", ">=1.20.5- <2", ">=1.21 <1.22", ">=1 <2 >invalid",
               "1 || 2", ">=1,<2", ">=1\t<2", "\t>=1\t <2\n", ">", ">=", "<=", "=", "~", "^", ">= 1",
               "* >invalid", "<0 >invalid", "\u00a0>=1", "1.2.* >1.2.3"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--java", required=True)
    parser.add_argument("--loader", required=True, type=Path)
    args = parser.parse_args()
    pairs = itertools.product(PREDICATES, VERSIONS)
    def encode(text):
        return base64.b64encode(text.encode()).decode()
    inputs = "".join(f"{encode(p)}\t{encode(v)}\n" for p, v in pairs)
    run = subprocess.run([args.java, "--class-path", str(args.loader), str(FIXTURES / "FabricVersionOracle.java")],
                         input=inputs, text=True, capture_output=True, check=True)
    outputs = run.stdout.splitlines()
    assert len(outputs) == len(PREDICATES) * len(VERSIONS), run.stderr
    assert all(x in ("T", "F", "E") for x in outputs)
    with zipfile.ZipFile(args.loader) as archive:
        loader_version = json.loads(archive.read("fabric.mod.json"))["version"]
    rows = [[p, "".join(outputs[i * len(VERSIONS):(i + 1) * len(VERSIONS)])] for i, p in enumerate(PREDICATES)]
    data = {"loader": loader_version, "sha256": hashlib.sha256(args.loader.read_bytes()).hexdigest(),
            "versions": VERSIONS, "predicates": rows}
    (FIXTURES / "fabric-version-predicates.json").write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")
    print(f"Fabric {loader_version}: {len(outputs)} conformance cases generated")

if __name__ == "__main__":
    main()
