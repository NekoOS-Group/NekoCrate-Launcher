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
  activeFilesFirst,
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

test("增量排序逐次匹配原版稳定分组：跨扩容、重试、计划重复和不同滚动位置", () => {
  const order = reactive(createFileOrder());
  let seed = 7123;
  const random = () =>
    (seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) / 4294967296;
  const statuses = [
    "queued",
    "downloading",
    "verifying",
    "done",
    "error",
    "cancelled",
  ];
  for (let i = 0; i < 1200; i++) {
    if (i < 550)
      indexFile(
        order,
        file(i, statuses[Math.floor(random() * statuses.length)]),
      );
    else {
      const row = order.files[Math.floor(random() * order.files.length)];
      row.status = statuses[Math.floor(random() * statuses.length)];
      row.downloaded = Math.floor(random() * 101);
      indexFile(order, row);
    }
    const expected = activeFilesFirst(order.files);
    assert.equal(
      order.activeCount,
      order.files.filter((f) => f.status === "downloading").length,
    );
    for (const scroll of [
      0,
      Math.floor(random() * order.files.length) * 48,
      order.files.length * 48,
    ]) {
      assert.deepEqual(
        indexedFileWindow(order, scroll),
        fileWindow(expected, scroll),
      );
    }
  }
});

test("字节与非下载状态变化不重建窗口，行内容仍响应；真正换位才更新顺序", () => {
  const order = reactive(createFileOrder());
  for (let i = 0; i < 100; i++) indexFile(order, file(i));
  let renders = 0;
  const window = computed(() => {
    renders++;
    return indexedFileWindow(order, 0);
  });
  const row = computed(() => window.value.items[0].downloaded);
  assert.equal(row.value, 0);
  order.files[0].downloaded = 55;
  order.files[0].status = "verifying";
  indexFile(order, order.files[0]);
  assert.equal(row.value, 55);
  assert.equal(renders, 1);
  order.files[99].status = "downloading";
  indexFile(order, order.files[99]);
  assert.equal(window.value.items[0].id, "99");
  assert.equal(renders, 2);
});

test("索引与真实 workflow 共享文件对象，阶段互不干扰，结束和丢弃任务释放索引", () => {
  for (const outcome of ["done", "error", "cancelled"]) {
    const workflow = reactive(createWorkflow("install", ""));
    applyWorkflowEvent(
      workflow,
      event("file_plan", "files", [file(1), file(2)]),
    );
    applyWorkflowEvent(
      workflow,
      event("file", "files", [{ ...file(2, "downloading"), downloaded: 60 }]),
    );
    applyWorkflowEvent(workflow, event("file_plan", "files", [file(2)]));
    applyWorkflowEvent(
      workflow,
      event("file", "java", [file(3, "downloading")]),
    );
    const files = workflow.fileOrder.get("files"),
      java = workflow.fileOrder.get("java");
    assert.equal(indexedFileWindow(files, 0).items[0], workflow.files.get("2"));
    assert.equal(indexedFileWindow(files, 0).items[0].downloaded, 60);
    assert.deepEqual(
      indexedFileWindow(java, 0).items.map((f) => f.id),
      ["3"],
    );
    finishWorkflow(workflow, outcome);
    for (const [step, order] of workflow.fileOrder) {
      const expected = activeFilesFirst(
        [...workflow.files.values()].filter((f) => f.step === step),
      );
      assert.deepEqual(indexedFileWindow(order, 0), fileWindow(expected, 0));
      assert.equal(order.activeCount, 0);
    }
    clearWorkflowFiles(workflow);
    assert.equal(workflow.files.size, 0);
    assert.equal(workflow.fileOrder.size, 0);
  }
});

test("空列表与超大列表均只读取可见十行，不遍历文件状态", () => {
  const order = createFileOrder();
  assert.deepEqual(indexedFileWindow(order, 900), {
    start: 0,
    height: 0,
    items: [],
  });
  for (let i = 0; i < 30000; i++)
    indexFile(order, file(i, i % 17 === 0 ? "downloading" : "done"));
  const expected = fileWindow(activeFilesFirst(order.files), 700000);
  let reads = 0;
  order.counts = new Proxy(order.counts, {
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
        throw new Error("must not scan statuses when rendering");
      },
    });
  const actual = indexedFileWindow(order, 700000);
  assert.deepEqual(
    actual.items.map((f) => f.id),
    expected.items.map((f) => f.id),
  );
  assert.equal(actual.start, expected.start);
  assert.equal(actual.items.length, 10);
  assert.ok(reads <= 10 * Math.ceil(Math.log2(order.files.length)));
});
