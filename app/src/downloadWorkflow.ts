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
    operation === "download_declared_mod"
      ? [
          ["mod_resolve", "解析 Mod 文件"],
          ["mod_files", "下载与校验 Mod"],
          ["mod_commit", "安装 Mod"],
        ]
      : operation === "download_modpack"
        ? [
            ["pack_archive_resolve", "解析整合包文件"],
            ["pack_archive_files", "下载整合包文件"],
            ["pack_archive_preview", "读取整合包信息"],
          ]
        : operation === "install_resources"
          ? [
              ["resource_check", "检查资源与目标目录"],
              ["resource_files", "下载与暂存资源"],
              ["resource_commit", "校验并安装资源"],
              ["finalize", "资源安装完成"],
            ]
          : operation === "modpack"
            ? [
                ["pack_resolve", "解析整合包依赖"],
                ["pack_files", "下载整合包文件"],
                ["pack_overrides", "写入整合包配置"],
                ["pack_game", "安装游戏与加载器"],
                ["java", "准备 Java"],
                ["finalize", "完成整合包安装"],
              ]
            : operation === "loader"
              ? [
                  ["metadata", "解析游戏版本"],
                  ["loader", "安装加载器"],
                  ["files", "下载与校验文件"],
                  ["natives", "准备原生库"],
                  ["java", "准备 Java"],
                  ["finalize", "完成安装"],
                ]
              : operation === "install"
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
    workflow.steps.find((step) => step.status === "running") ||
    workflow.steps.find((step) =>
      (event.files || []).some(
        (file) => workflow.files.get(file.id)?.step === step.id,
      ),
    );
  if (!step) return;
  for (const update of event.files || []) {
    const previous = workflow.files.get(update.id);
    const owner =
      workflow.steps.find((item) => item.id === previous?.step) || step;
    owner.activity ||= {};
    const count = (file: DownloadFile, delta: number) => {
      const key =
        file.status === "done" && file.cached ? "reused" : file.status;
      owner.activity![key] = Math.max(0, (owner.activity![key] || 0) + delta);
    };
    if (previous) {
      // A duplicate plan must never reset a started or completed transfer.
      if (event.kind === "file_plan") continue;
      if (previous.status === "done" && update.status !== "done")
        owner.filesDone--;
      if (previous.status !== "done" && update.status === "done")
        owner.filesDone++;
      const downloaded = ["error", "cancelled"].includes(update.status)
        ? Math.max(previous.downloaded, update.downloaded)
        : update.downloaded;
      count(previous, -1);
      Object.assign(previous, update, {
        downloaded,
        step: previous.step,
        cached: !!update.cached,
        retryable: !!update.retryable,
        error: update.error,
      });
      count(previous, 1);
      indexFile(workflow.fileOrder.get(previous.step!)!, previous);
    } else {
      const file = { ...update, step: step.id };
      workflow.files.set(update.id, file);
      count(file, 1);
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
  for (const step of workflow.steps) step.activity = {};
  for (const file of workflow.files.values()) {
    if (result === "done") {
      file.status = "done";
      file.downloaded = file.total;
    } else if (
      ["downloading", "verifying", "reusing", "installing"].includes(
        file.status,
      )
    )
      file.status = "cancelled";
    const owner = workflow.steps.find((step) => step.id === file.step);
    if (owner) {
      const key =
        file.status === "done" && file.cached ? "reused" : file.status;
      owner.activity![key] = (owner.activity![key] || 0) + 1;
    }
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

export function resumeWorkflow(workflow: WorkflowState) {
  for (const step of workflow.steps) {
    if (["error", "cancelled", "paused", "running"].includes(step.status))
      step.status = "pending";
  }
  if (workflow.steps[0]) workflow.steps[0].status = "running";
}
