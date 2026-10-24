import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from benchmark_linux_setup import compare


class LinuxSetupBenchmarkTests(unittest.TestCase):
    def samples(self, directory, apt, image):
        for method, timings in (("apt", apt), ("image", image)):
            for sample, seconds in enumerate(timings, 1):
                (directory / f"{method}-{sample}.json").write_text(json.dumps({
                    "method": method, "sample": sample, "seconds": seconds,
                    "image": "ghcr.io/example/builder@sha256:" + "a" * 64,
                }))

    def test_median_not_best_case_and_requires_ten_percent_improvement(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            self.samples(directory, [50, 55, 300], [10, 52, 60])
            self.assertFalse(compare(directory)["promote"])
            self.samples(directory, [50, 55, 300], [10, 40, 60])
            self.assertTrue(compare(directory)["promote"])

    def test_incomplete_or_mixed_images_cannot_promote(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            self.samples(directory, [50, 55, 60], [10, 12, 14])
            sample = directory / "image-3.json"
            result = json.loads(sample.read_text())
            result["image"] = "different"
            sample.write_text(json.dumps(result))
            with self.assertRaises(ValueError):
                compare(directory)
            sample.unlink()
            with self.assertRaises(ValueError):
                compare(directory)
