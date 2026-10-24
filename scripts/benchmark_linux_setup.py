"""Compare native apt setup and a cold image pull on separate fresh runners."""

import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
VERIFY = "pkg-config --modversion webkit2gtk-4.1 openssl && command -v patchelf && python3 --version"


def run_sample(method, sample, image, output):
    started = time.perf_counter()
    if method == "apt":
        packages = (ROOT / "scripts/linux_build_packages.txt").read_text().split()
        subprocess.run(["sudo", "bash", str(ROOT / "scripts/install_linux_dependencies.sh"), *packages], check=True)
        subprocess.run(["bash", "-ec", VERIFY], check=True)
    else:
        # A cache hit would invalidate this measurement. Every sample has its
        # own fresh runner, and this guard catches accidental local reuse.
        if subprocess.run(["docker", "image", "inspect", image], stdout=subprocess.DEVNULL,
                          stderr=subprocess.DEVNULL).returncode == 0:
            raise RuntimeError("Image already present: a cold-pull benchmark requires a fresh runner")
        subprocess.run(["docker", "pull", image], check=True)
        subprocess.run(["docker", "run", "--rm", image, "bash", "-ec", VERIFY], check=True)
    result = {"method": method, "sample": sample, "seconds": round(time.perf_counter() - started, 3), "image": image}
    output.mkdir(parents=True, exist_ok=True)
    (output / f"{method}-{sample}.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


def compare(directory):
    samples = [json.loads(p.read_text()) for p in sorted(directory.glob("*.json"))]
    expected = {(method, sample) for method in ("apt", "image") for sample in (1, 2, 3)}
    if len(samples) != 6 or {(s["method"], s["sample"]) for s in samples} != expected:
        raise ValueError("Expected three distinct samples per method")
    if len({s["image"] for s in samples}) != 1 or "@sha256:" not in samples[0]["image"]:
        raise ValueError("All samples must test the same immutable image")
    times = {method: [s["seconds"] for s in samples if s["method"] == method] for method in ("apt", "image")}
    if any(not isinstance(t, (int, float)) or not 0 < t < 3600 for values in times.values() for t in values):
        raise ValueError("Invalid setup timing")
    medians = {method: statistics.median(values) for method, values in times.items()}
    report = {
        "image": samples[0]["image"], "samples_seconds": times, "median_seconds": medians,
        "saved_seconds": round(medians["apt"] - medians["image"], 3),
        # Require a margin, rather than promoting based on timing noise.
        "promote": medians["image"] <= medians["apt"] * 0.9,
    }
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--method", choices=("apt", "image"))
    parser.add_argument("--sample", type=int, choices=(1, 2, 3))
    parser.add_argument("--image")
    parser.add_argument("--output", type=Path, default=Path("artifacts/linux-setup"))
    parser.add_argument("--compare", type=Path)
    args = parser.parse_args()
    if args.compare:
        report = compare(args.compare)
        rendered = json.dumps(report, indent=2)
        print(rendered)
        args.output.mkdir(parents=True, exist_ok=True)
        (args.output / "comparison.json").write_text(rendered + "\n")
        if os.environ.get("GITHUB_OUTPUT"):
            with open(os.environ["GITHUB_OUTPUT"], "a") as output:
                output.write(f"promote={str(report['promote']).lower()}\n")
        if os.environ.get("GITHUB_STEP_SUMMARY"):
            with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as summary:
                summary.write("## Linux environment setup: cold runners\n\n```json\n" + rendered + "\n```\n")
    elif args.method and args.sample and args.image and "@sha256:" in args.image:
        run_sample(args.method, args.sample, args.image, args.output)
    else:
        parser.error("Use --compare, or provide --method, --sample and an image digest")


if __name__ == "__main__":
    main()
