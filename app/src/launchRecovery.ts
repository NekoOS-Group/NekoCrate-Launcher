import type { CrashReport } from "./types";

export type RecoveryAction =
  "java" | "memory" | "mods" | "repair" | "settings" | "report";
export interface LaunchRecovery {
  instanceId: string;
  instanceName: string;
  versionName: string;
  report: CrashReport;
  actions: RecoveryAction[];
}

export const recoveryLabels: Record<RecoveryAction, string> = {
  java: "选择 Java",
  memory: "调整内存",
  mods: "管理 Mods",
  repair: "修复游戏文件",
  settings: "检查实例设置",
  report: "打开日志文件",
};

// Only offer a repair when the installer can actually repair this version.
// Unknown exceptions retain access to the log without guessing a cause.
export function recoveryActions(report: CrashReport): RecoveryAction[] {
  const actions = new Set<RecoveryAction>();
  for (const { code } of report.findings) {
    if (["java", "launch_java", "bytecode"].includes(code)) actions.add("java");
    if (["heap", "native_memory"].includes(code)) actions.add("memory");
    if (
      ["dependencies", "duplicate", "config", "mixin", "bytecode"].includes(
        code,
      )
    )
      actions.add("mods");
    if (code === "files") actions.add(report.repairable ? "repair" : "mods");
    if (code === "module_conflict") actions.add("settings");
  }
  if (!actions.size) actions.add("settings");
  if (report.file) actions.add("report");
  return [...actions];
}
