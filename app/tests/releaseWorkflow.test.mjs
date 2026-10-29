import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const workflow = readFileSync(
  new URL("../../.github/workflows/release.yml", import.meta.url),
  "utf8",
);
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const commit = "tested-commit";
const repo = { owner: "NekoOS-Group", repo: "NekoCrate-Launcher" };
const run = {
  id: 42,
  head_sha: commit,
  head_repository: { full_name: `${repo.owner}/${repo.repo}` },
  head_branch: "neko_dev",
  event: "push",
  path: ".github/workflows/ci.yml",
  status: "in_progress",
  conclusion: null,
  html_url: "https://github.com/example/actions/runs/42",
};
const artifact = {
  id: 7,
  name: "ci-frontend",
  size_in_bytes: 12,
  expired: false,
};

function job(name) {
  const start = workflow.indexOf(`\n  ${name}:\n`);
  assert.ok(start >= 0);
  const tail = workflow.slice(start + 1);
  const next = tail.slice(1).search(/^  [a-z]+:\n/m);
  return next < 0 ? tail : tail.slice(0, next + 1);
}
async function execute(name, actions) {
  const body = job(name).split("          script: |\n")[1];
  assert.ok(body);
  const source = body
    .split("\n")
    .filter((s) => s.startsWith("            ") || !s.trim())
    .map((s) => s.slice(12))
    .join("\n");
  const outputs = {};
  await new AsyncFunction(
    "github",
    "context",
    "core",
    "process",
    "setTimeout",
    source,
  )(
    { rest: { actions } },
    { repo },
    {
      info() {},
      setOutput(name, value) {
        outputs[name] = value;
      },
    },
    { env: { RELEASE_COMMIT: commit, CI_RUN: "42" } },
    (callback) => callback(),
  );
  return outputs;
}
function prepareActions(runs, artifacts = [artifact]) {
  return {
    async listWorkflowRuns(args) {
      assert.equal(args.head_sha, commit);
      assert.equal(args.workflow_id, "ci.yml");
      return { data: { workflow_runs: runs } };
    },
    async listWorkflowRunArtifacts(args) {
      assert.equal(args.run_id, 42);
      return { data: { artifacts } };
    },
  };
}

test("release compilation can reuse checked frontend while full CI is running", async () => {
  assert.deepEqual(await execute("prepare", prepareActions([run])), {
    run: 42,
    artifact: 7,
  });
  assert.match(job("publish"), /needs: \[prepare, checks, build\]/);
});

test("early frontend reuse refuses failed, foreign and wrong-commit CI", async () => {
  await assert.rejects(
    execute(
      "prepare",
      prepareActions([{ ...run, status: "completed", conclusion: "failure" }]),
    ),
    /CI did not pass/,
  );
  for (const other of [
    { ...run, head_sha: "other" },
    { ...run, head_repository: { full_name: "fork/project" } },
    { ...run, event: "pull_request" },
  ]) {
    await assert.rejects(
      execute("prepare", prepareActions([other])),
      /No CI run/,
    );
  }
  await assert.rejects(
    execute(
      "prepare",
      prepareActions(
        [{ ...run, status: "completed", conclusion: "success" }],
        [{ ...artifact, expired: true }],
      ),
    ),
    /artifact expired or missing/,
  );
});

test("publish gate waits for complete CI success and rejects cancellation or changed identity", async () => {
  let reads = 0;
  await execute("checks", {
    async getWorkflowRun() {
      reads++;
      return {
        data:
          reads === 1
            ? run
            : { ...run, status: "completed", conclusion: "success" },
      };
    },
  });
  assert.equal(reads, 2);
  for (const bad of [
    { ...run, status: "completed", conclusion: "failure" },
    { ...run, status: "completed", conclusion: "cancelled" },
    { ...run, head_sha: "other" },
    { ...run, path: ".github/workflows/other.yml" },
    { ...run, head_repository: { full_name: "fork/project" } },
  ]) {
    await assert.rejects(
      execute("checks", {
        async getWorkflowRun() {
          return { data: bad };
        },
      }),
      /CI did not pass|identity does not match/,
    );
  }
});
