import type { DownloadFile, LauncherEvent, WorkflowState } from "./types";
import {
  createFileOrder,
  indexFile,
  finishFileOrder,
  fileWindowBounds,
} from "./fileOrder.ts";
export { fileRowHeight } from "./fileOrder.ts";

export function createWorkflow(
  operation: string,
  title: string,
): WorkflowState {
  const plan =
    operation === "install"
      ? [
          ["metadata", "解析游戏版本"],
          ["files", "下载与校验文件"],
          ["natives", "准备原生库"],
          ["java", "准备 Java"],
          ["finalize", "完成安装"],
        ]
      : operation === "launch"
        ? [
            ["prepare", "检查启动配置"],
            ["java", "准备 Java"],
            ["account", "准备游戏身份"],
            ["start", "启动游戏"],
          ]
        : [["request", title]];
  return {
    steps: plan.map(([id, label], index) => ({
      id,
      label,
      status: index ? "pending" : "running",
      fileCount: 0,
      filesDone: 0,
    })),
    files: new Map(),
    fileOrder: new Map(),
  };
}

export function applyWorkflowEvent(
  workflow: WorkflowState,
  event: LauncherEvent,
) {
  if (event.kind === "workflow" && event.step) {
    const index = workflow.steps.findIndex((step) => step.id === event.step);
    if (index >= 0)
      workflow.steps.forEach((step, position) => {
        if (position < index) step.status = "done";
        else if (position === index) step.status = "running";
      });
  }
  const step =
    workflow.steps.find((step) => step.id === event.step) ||
    workflow.steps.find((step) => step.status === "running");
  if (!step) return;
  for (const update of event.files || []) {
    const previous = workflow.files.get(update.id);
    if (previous) {
      // A duplicate plan must never reset a started or completed transfer.
      if (event.kind === "file_plan") continue;
      if (previous.status === "done" && update.status !== "done")
        step.filesDone--;
      if (previous.status !== "done" && update.status === "done")
        step.filesDone++;
      const downloaded = ["error", "cancelled"].includes(update.status)
        ? Math.max(previous.downloaded, update.downloaded)
        : update.downloaded;
      Object.assign(previous, update, { downloaded, step: previous.step });
      indexFile(workflow.fileOrder.get(previous.step!)!, previous);
    } else {
      const file = { ...update, step: step.id };
      workflow.files.set(update.id, file);
      if (!workflow.fileOrder.has(step.id))
        workflow.fileOrder.set(step.id, createFileOrder());
      indexFile(workflow.fileOrder.get(step.id)!, file);
      step.fileCount++;
      if (update.status === "done") step.filesDone++;
    }
  }
}

export function finishWorkflow(
  workflow: WorkflowState,
  result: "done" | "error" | "cancelled",
) {
  for (const step of workflow.steps) {
    if (
      result === "done" ||
      step.status === "running" ||
      step.status === "paused"
    )
      step.status = result;
    if (result === "done") step.filesDone = step.fileCount;
  }
  for (const file of workflow.files.values()) {
    if (result === "done") {
      file.status = "done";
      file.downloaded = file.total;
    } else if (["downloading", "verifying"].includes(file.status))
      file.status = "cancelled";
  }
  for (const order of workflow.fileOrder.values()) finishFileOrder(order);
}

export function clearWorkflowFiles(workflow: WorkflowState) {
  workflow.files.clear();
  workflow.fileOrder.clear();
}

export function setWorkflowPaused(workflow: WorkflowState, paused: boolean) {
  for (const step of workflow.steps) {
    if (step.status === (paused ? "running" : "paused"))
      step.status = paused ? "paused" : "running";
  }
}

export function activeFilesFirst(files: readonly DownloadFile[]) {
  const active: DownloadFile[] = [];
  const remaining: DownloadFile[] = [];
  for (const file of files)
    (file.status === "downloading" ? active : remaining).push(file);
  return active.concat(remaining);
}

export function fileWindow(files: readonly DownloadFile[], scrollTop: number) {
  const { start, height } = fileWindowBounds(files.length, scrollTop);
  return {
    start,
    items: files.slice(start, start + 10),
    height,
  };
}
export function filePercent(file: DownloadFile) {
  return file.status === "done"
    ? 100
    : file.total > 0
      ? Math.max(
          0,
          Math.min(100, Math.round((file.downloaded / file.total) * 100)),
        )
      : 0;
}
export function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / 1024 ** 2).toFixed(1)} MiB`;
}
