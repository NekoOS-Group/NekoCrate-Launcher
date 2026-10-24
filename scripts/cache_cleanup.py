"""Prune obsolete Actions caches without dropping the newest build of a family.

Only this repository's Rust/npm cache namespaces are managed. Unknown caches and
feature/PR branch caches are left to GitHub's normal retention policy.
"""

import argparse
from collections import defaultdict
import json
import os
from pathlib import Path
import re
from urllib.error import HTTPError
from urllib.parse import urlencode
from urllib.request import Request, urlopen

GIB = 1024**3
MAINTENANCE_PATH = ".github/workflows/cache-maintenance.yml"


def family(key):
    # The two Rust suffixes hash the toolchain/environment and dependency graph.
    match = re.fullmatch(r"(v0-rust-.+)-[0-9a-f]{8}-[0-9a-f]{8}", key)
    if match:
        return match[1], 2
    match = re.fullmatch(r"(node-cache-.+-npm)-[0-9a-f]{64}", key)
    if match:
        return match[1], 1
    return None


def plan(caches, default_branch, budget_bytes=8 * GIB):
    """Keep two Rust generations / one npm generation; trim spares above 8 GiB."""
    groups, removal, protected = defaultdict(list), {}, set()
    for cache in caches:
        group = family(cache["key"])
        if not group:
            continue
        ref = cache["ref"]
        if re.fullmatch(r"refs/(?:heads/refs/)?tags/dev\d+\.\d+(?:\.\d+)?", ref):
            removal[cache["id"]] = "old release tag scope"
        elif ref == f"refs/heads/{default_branch}":
            groups[group].append(cache)
    for (_, keep), entries in groups.items():
        entries.sort(key=lambda c: (c["created_at"], c["id"]), reverse=True)
        protected.add(entries[0]["id"])
        for cache in entries[keep:]:
            removal[cache["id"]] = "superseded cache generation"
    remaining = sum(c["size_in_bytes"] for c in caches if c["id"] not in removal)
    spares = sorted(
        (c for entries in groups.values() for c in entries
         if c["id"] not in removal and c["id"] not in protected),
        key=lambda c: (c["last_accessed_at"], c["created_at"], c["id"]),
    )
    for cache in spares:
        if remaining <= budget_bytes:
            break
        removal[cache["id"]] = "8 GiB soft budget (newest generation preserved)"
        remaining -= cache["size_in_bytes"]
    return [dict(c, reason=removal[c["id"]]) for c in caches if c["id"] in removal]


class GitHub:
    def __init__(self, repo, token):
        if not re.fullmatch(r"[\w.-]+/[\w.-]+", repo):
            raise ValueError("Expected owner/repository")
        self.base = f"https://api.github.com/repos/{repo}"
        self.token = token

    def request(self, path, method="GET"):
        request = Request(self.base + path, method=method, headers={
            "Authorization": f"Bearer {self.token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "NekoCrate-cache-maintenance",
        })
        try:
            with urlopen(request, timeout=30) as response:
                data = response.read()
                return json.loads(data) if data else None
        except HTTPError as error:
            if method == "DELETE" and error.code == 404:
                return None  # GitHub may already have evicted it.
            raise

    def pages(self, path, field, **query):
        result = []
        for page in range(1, 1001):
            data = self.request(path + "?" + urlencode({**query, "per_page": 100, "page": page}))[field]
            result.extend(data)
            if len(data) < 100:
                return result
        raise RuntimeError("Pagination limit reached; refusing to prune a partial inventory")

    def caches(self):
        return self.pages("/actions/caches", "actions_caches")

    def busy_runs(self):
        runs = []
        for status in ("queued", "in_progress", "waiting", "pending", "requested"):
            runs.extend(self.pages("/actions/runs", "workflow_runs", status=status))
        return [r for r in runs if r.get("path", "").split("@")[0] != MAINTENANCE_PATH]


def prune(api, default_branch, apply=False):
    busy = api.busy_runs()
    if busy:
        return {"skipped": "Other workflows are active", "run_ids": sorted({r["id"] for r in busy})}
    caches = api.caches()
    candidates = plan(caches, default_branch)
    result = {
        "apply": apply,
        "before_gib": round(sum(c["size_in_bytes"] for c in caches) / GIB, 3),
        "planned_reclaim_gib": round(sum(c["size_in_bytes"] for c in candidates) / GIB, 3),
        "candidates": [{k: c[k] for k in ("id", "key", "ref", "size_in_bytes", "reason")} for c in candidates],
        "deleted": [],
    }
    if not apply:
        return result
    # Refresh the inventory and plan: caches may have been read or saved since
    # the first request. Never delete a cache promoted to the protected set.
    current = api.caches()
    eligible = {c["id"] for c in plan(current, default_branch)}
    current = {c["id"]: c for c in current}
    for candidate in candidates:
        if api.busy_runs():
            result["stopped"] = "A workflow started; remaining deletions skipped"
            break
        cache = current.get(candidate["id"])
        if (cache is None or cache["id"] not in eligible
                or cache["last_accessed_at"] != candidate["last_accessed_at"]):
            continue
        api.request(f"/actions/caches/{cache['id']}", method="DELETE")
        result["deleted"].append(cache["id"])
    result["after_gib"] = round(sum(c["size_in_bytes"] for c in api.caches()) / GIB, 3)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY"))
    parser.add_argument("--default-branch")
    parser.add_argument("--snapshot", type=Path, help="Plan from a saved API response, without credentials")
    parser.add_argument("--apply", action="store_true", help="Delete the planned caches (default: dry run)")
    args = parser.parse_args()
    if args.snapshot:
        if args.apply or not args.default_branch:
            parser.error("Snapshots require --default-branch and cannot be applied")
        result = plan(json.loads(args.snapshot.read_text())["actions_caches"], args.default_branch)
    else:
        if not args.repo or not os.environ.get("GH_TOKEN"):
            parser.error("Live access requires --repo/GITHUB_REPOSITORY and GH_TOKEN")
        api = GitHub(args.repo, os.environ["GH_TOKEN"])
        branch = args.default_branch or api.request("")["default_branch"]
        result = prune(api, branch, apply=args.apply)
    rendered = json.dumps(result, indent=2)
    print(rendered)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as summary:
            summary.write("## Cache maintenance\n\n```json\n" + rendered + "\n```\n")


if __name__ == "__main__":
    main()
