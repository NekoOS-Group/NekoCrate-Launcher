import assert from "node:assert/strict";
import test from "node:test";
import { computed, reactive } from "vue";
import {
  createFileOrder,
  indexFile,
  indexedFileWindow,
} from "../src/fileOrder.ts";
import {
  createWorkflow,
  applyWorkflowEvent,
  finishWorkflow,
  clearWorkflowFiles,
  resumeWorkflow,
  setWorkflowPaused,
  fileWindow,
} from "../src/downloadWorkflow.ts";

const file = (id, status = "queued") => ({
  id: String(id),
  name: String(id),
  status,
  downloaded: 0,
  total: 100,
});
const event = (kind, step, files = []) => ({
  kind,
  step,
  files,
  message: "",
  instance_id: "a",
  total: 0,
  completed: 0,
  bytes: 0,
});
const processing = (status) =>
  ["downloading", "verifying", "reusing", "installing"].includes(status);

// Independent chronological reference model; deliberately sorts the full history.
function history() {
  const entries = new Map();
  let clock = 0;
  return {
    update(row) {
      const old = entries.get(row.id);
      entries.set(row.id, {
        row,
        status: row.status,
        planned: old?.planned ?? entries.size,
        started:
          processing(row.status) && !processing(old?.status)
            ? ++clock
            : old?.started,
        completed:
          row.status === "done" && old?.status !== "done"
            ? ++clock
            : old?.completed,
      });
    },
    files() {
      const rows = [...entries.values()];
      return [
        ...rows
          .filter((e) => processing(e.status))
          .sort((a, b) => a.started - b.started),
        ...rows
          .filter((e) => e.status === "done")
          .sort((a, b) => b.completed - a.completed),
        ...rows
          .filter((e) => !processing(e.status) && e.status !== "done")
          .sort((a, b) => a.planned - b.planned),
      ].map((e) => e.row);
    },
  };
}

test("active start order and newest completion first survive growth, retries and arbitrary scroll positions", () => {
  const order = reactive(createFileOrder()),
    expected = history();
  let seed = 7123;
  const random = () =>
    (seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) / 4294967296;
  const statuses = [
    "queued",
    "downloading",
    "verifying",
    "reusing",
    "installing",
    "done",
    "error",
    "cancelled",
  ];
  for (let i = 0; i < 1600; i++) {
    const row =
      i < 550
        ? file(i)
        : order.files[Math.floor(random() * order.files.length)];
    row.status = statuses[Math.floor(random() * statuses.length)];
    row.downloaded = Math.floor(random() * 101);
    expected.update(row);
    indexFile(order, row);
    assert.equal(
      order.active.count,
      order.files.filter((f) => processing(f.status)).length,
    );
    assert.equal(
      order.active.count + order.completed.count + order.pending.count,
      order.files.length,
    );
    for (const scroll of [
      0,
      Math.floor(random() * order.files.length) * 48,
      order.files.length * 48,
    ])
      assert.deepEqual(
        indexedFileWindow(order, scroll),
        fileWindow(expected.files(), scroll),
      );
  }
});

test("bytes, processing phases and duplicate completion never reorder or recompute the window", () => {
  const order = reactive(createFileOrder());
  for (let i = 0; i < 100; i++) indexFile(order, file(i));
  const update = (id, status) => {
    order.files[id].status = status;
    indexFile(order, order.files[id]);
  };
  update(99, "downloading");
  update(0, "downloading");
  let renders = 0;
  const window = computed(() => {
    renders++;
    return indexedFileWindow(order, 0);
  });
  assert.deepEqual(
    window.value.items.slice(0, 2).map((f) => f.id),
    ["99", "0"],
  );
  const bytes = computed(() => window.value.items[0].downloaded);
  assert.equal(bytes.value, 0);
  order.files[99].downloaded = 55;
  for (const status of ["verifying", "reusing", "installing"])
    update(99, status);
  assert.equal(bytes.value, 55);
  assert.equal(renders, 1);
  update(99, "done");
  assert.deepEqual(
    window.value.items.slice(0, 2).map((f) => f.id),
    ["0", "99"],
  );
  update(0, "done");
  assert.deepEqual(
    window.value.items.slice(0, 2).map((f) => f.id),
    ["0", "99"],
  );
  const previousRenders = renders;
  update(99, "done");
  assert.deepEqual(
    window.value.items.slice(0, 2).map((f) => f.id),
    ["0", "99"],
  );
  assert.equal(renders, previousRenders);
});

test("workflow preserves completion order through duplicate plans, pause, resume and retry", () => {
  const workflow = reactive(createWorkflow("install", ""));
  applyWorkflowEvent(
    workflow,
    event("file_plan", "files", [file("a"), file("b"), file("c"), file("d")]),
  );
  const ids = () =>
    indexedFileWindow(workflow.fileOrder.get("files"), 0).items.map(
      (f) => f.id,
    );
  const update = (id, status) =>
    applyWorkflowEvent(workflow, event("file", "files", [file(id, status)]));
  update("b", "downloading");
  update("a", "downloading");
  assert.deepEqual(ids(), ["b", "a", "c", "d"]);
  update("b", "verifying");
  assert.deepEqual(ids(), ["b", "a", "c", "d"]);
  update("b", "done");
  update("c", "done");
  assert.deepEqual(ids(), ["a", "c", "b", "d"]);
  update("b", "done");
  applyWorkflowEvent(
    workflow,
    event("file_plan", "files", [file("a"), file("b"), file("c")]),
  );
  setWorkflowPaused(workflow, true);
  setWorkflowPaused(workflow, false);
  resumeWorkflow(workflow);
  assert.deepEqual(ids(), ["a", "c", "b", "d"]);
  update("a", "done");
  assert.deepEqual(ids(), ["a", "c", "b", "d"]);
  update("b", "downloading");
  assert.deepEqual(ids(), ["b", "a", "c", "d"]);
  update("b", "done");
  assert.deepEqual(ids(), ["b", "a", "c", "d"]);
  update("d", "done");
  const before = ids();
  finishWorkflow(workflow, "done");
  assert.deepEqual(ids(), before);
});

test("terminal workflow updates preserve existing completion ranks and separate stages", () => {
  for (const outcome of ["done", "error", "cancelled"]) {
    const workflow = reactive(createWorkflow("install", ""));
    applyWorkflowEvent(
      workflow,
      event("file_plan", "files", [file(1), file(2), file(3)]),
    );
    applyWorkflowEvent(
      workflow,
      event("file", "files", [
        file(3, "done"),
        file(1, "done"),
        { ...file(2, "downloading"), downloaded: 60 },
      ]),
    );
    applyWorkflowEvent(
      workflow,
      event("file", "java", [file(4, "downloading")]),
    );
    const orders = [...workflow.fileOrder.values()],
      models = orders.map((order) => {
        const expected = history();
        for (const row of [...order.files].sort((a, b) =>
          a.id === "3" ? -1 : b.id === "3" ? 1 : 0,
        ))
          expected.update(row);
        return expected;
      });
    const files = workflow.fileOrder.get("files");
    assert.equal(indexedFileWindow(files, 0).items[0], workflow.files.get("2"));
    assert.equal(indexedFileWindow(files, 0).items[0].downloaded, 60);
    finishWorkflow(workflow, outcome);
    orders.forEach((order, i) => {
      order.files.forEach((row) => models[i].update(row));
      assert.deepEqual(
        indexedFileWindow(order, 0),
        fileWindow(models[i].files(), 0),
      );
      assert.equal(order.active.count, 0);
    });
    clearWorkflowFiles(workflow);
    assert.equal(workflow.files.size, 0);
    assert.equal(workflow.fileOrder.size, 0);
  }
});

test("30,000 files select just the visible ten rows in logarithmic work without reading statuses", () => {
  const order = createFileOrder(),
    expected = history();
  assert.deepEqual(indexedFileWindow(order, 900), {
    start: 0,
    height: 0,
    items: [],
  });
  for (let i = 0; i < 30000; i++) {
    const row = file(i, i % 17 === 0 ? "downloading" : "done");
    expected.update(row);
    indexFile(order, row);
  }
  const window = fileWindow(expected.files(), 700000);
  let reads = 0;
  for (const index of [order.pending, order.active, order.completed])
    index.counts = new Proxy(index.counts, {
      get(target, key) {
        if (typeof key === "string" && !Number.isNaN(Number(key))) {
          assert.ok(Number.isInteger(Number(key)));
          reads++;
        }
        return Reflect.get(target, key, target);
      },
    });
  for (const row of order.files)
    Object.defineProperty(row, "status", {
      get() {
        throw Error("must not scan statuses when rendering");
      },
    });
  const actual = indexedFileWindow(order, 700000);
  assert.deepEqual(
    actual.items.map((f) => f.id),
    window.items.map((f) => f.id),
  );
  assert.equal(actual.start, window.start);
  assert.equal(actual.items.length, 10);
  assert.ok(reads <= 10 * Math.ceil(Math.log2(order.files.length)));
});
