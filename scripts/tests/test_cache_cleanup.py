import copy
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from cache_cleanup import GIB, plan, prune


def cache(id, family="v0-rust-checks-Linux-x64", ref="refs/heads/neko_dev", size=GIB):
    return dict(id=id, key=f"{family}-aaaaaaaa-{id:08x}", ref=ref, size_in_bytes=size,
                created_at=f"2026-10-{id:02d}T00:00:00Z", last_accessed_at=f"2026-10-{id:02d}T00:00:00Z")


class CachePolicyTests(unittest.TestCase):
    def test_retains_two_rust_generations_per_platform_and_one_npm_generation(self):
        entries = [cache(1), cache(2), cache(3), cache(4, "v0-rust-checks-Darwin-arm64")]
        for id in (5, 6):
            entries.append(dict(cache(id), key="node-cache-Linux-x64-npm-" + f"{id:064x}"))
        self.assertEqual({c["id"] for c in plan(entries, "neko_dev")}, {1, 5})

    def test_only_known_old_dev_tag_scopes_are_removed(self):
        entries = [cache(1, ref="refs/tags/dev0.3"), cache(2, ref="refs/heads/refs/tags/dev0.3.1"),
                   cache(3, ref="refs/heads/feature"), cache(4, ref="refs/pull/12/merge"),
                   dict(cache(5, ref="refs/tags/dev0.3"), key="unrelated-cache"),
                   cache(6, ref="refs/tags/v1.0")]
        self.assertEqual({c["id"] for c in plan(entries, "neko_dev")}, {1, 2})

    def test_tool_cache_keeps_latest_per_platform_without_touching_other_scopes(self):
        entries = [
            dict(cache(1), key="nekocrate-tools-v1-Windows-X64-node22-2.10.0"),
            dict(cache(2), key="nekocrate-tools-v1-Windows-X64-node22-2.10.1"),
            dict(cache(3), key="nekocrate-tools-v1-macOS-ARM64-node22-2.10.0"),
            dict(cache(4, ref="refs/pull/12/merge"), key="nekocrate-tools-v1-Windows-X64-node22-2.10.2"),
            dict(cache(5), key="unrelated-tools-v1-Windows-X64-node22-2.10.0"),
        ]
        self.assertEqual({c["id"] for c in plan(entries, "neko_dev")}, {1})

    def test_budget_removes_spares_but_never_the_only_cache_of_a_family(self):
        entries = [cache(1, size=5 * GIB), cache(2, size=5 * GIB),
                   cache(3, "v0-rust-release-Windows-x64", size=5 * GIB)]
        self.assertEqual([c["id"] for c in plan(entries, "neko_dev")], [1])

    def test_active_workflow_prevents_all_mutations(self):
        api = FakeAPI([cache(1), cache(2), cache(3)], busy=True)
        self.assertIn("skipped", prune(api, "neko_dev", apply=True))
        self.assertFalse(api.deleted)

    def test_dry_run_never_deletes_and_apply_rechecks_recent_access(self):
        api = FakeAPI([cache(1), cache(2), cache(3)])
        prune(api, "neko_dev")
        self.assertFalse(api.deleted)
        api = FakeAPI([cache(1), cache(2), cache(3)], accessed=True)
        prune(api, "neko_dev", apply=True)
        self.assertFalse(api.deleted)

    def test_apply_deletes_only_candidates_and_stops_for_a_new_run(self):
        api = FakeAPI([cache(1), cache(2), cache(3), cache(4)])
        self.assertEqual(prune(api, "neko_dev", apply=True)["deleted"], [1, 2])
        api = FakeAPI([cache(1), cache(2), cache(3), cache(4)], start_after=2)
        self.assertEqual(prune(api, "neko_dev", apply=True)["deleted"], [1])


class FakeAPI:
    def __init__(self, entries, busy=False, accessed=False, start_after=999):
        self.entries, self.busy, self.accessed = entries, busy, accessed
        self.start_after, self.reads, self.busy_reads, self.deleted = start_after, 0, 0, []

    def caches(self):
        self.reads += 1
        result = copy.deepcopy([c for c in self.entries if c["id"] not in self.deleted])
        if self.reads > 1 and self.accessed:
            result[0]["last_accessed_at"] = "2026-10-31T00:00:00Z"
        return result

    def busy_runs(self):
        self.busy_reads += 1
        return [{"id": 1}] if self.busy or self.busy_reads > self.start_after else []

    def request(self, path, method):
        assert method == "DELETE"
        self.deleted.append(int(path.rsplit("/", 1)[1]))
