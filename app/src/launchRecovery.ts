import type { CrashReport } from "./types";

export type RecoveryAction =
  | "java"
  | "memory"
  | "mods"
  | "dependencies"
  | "repair"
  | "settings"
  | "report";
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
  dependencies: "修复缺失前置",
  repair: "修复游戏文件",
  settings: "检查实例设置",
  report: "打开日志文件",
};

export type AutomaticRepair = "dependencies" | "repair";
export function automaticRepairs(report: CrashReport): AutomaticRepair[] {
  const actions: AutomaticRepair[] = [];
  if (report.repairable && report.findings.some((f) => f.code === "files"))
    actions.push("repair");
  if (report.findings.some((f) => f.code === "missing_dependencies"))
    actions.push("dependencies");
  return actions;
}

// Only offer a repair when the installer can actually repair this version.
// Unknown exceptions retain access to the log without guessing a cause.
export function recoveryActions(report: CrashReport): RecoveryAction[] {
  const actions = new Set<RecoveryAction>();
  for (const { code } of report.findings) {
    if (code === "missing_dependencies") {
      actions.add("dependencies");
      actions.add("mods");
    }
    if (["java", "launch_java", "bytecode"].includes(code)) actions.add("java");
    if (["heap", "native_memory"].includes(code)) actions.add("memory");
    if (
      [
        "dependencies",
        "duplicate",
        "config",
        "mixin",
        "bytecode",
        "registry",
        "pack_disabled",
        "pack_missing",
        "pack_changed",
        "component_versions",
        "component_dependencies",
        "component_duplicate",
        "component_corrupt",
      ].includes(code)
    )
      actions.add("mods");
    if (code === "files") actions.add(report.repairable ? "repair" : "mods");
    if (code === "module_conflict") actions.add("settings");
  }
  if (!actions.size) actions.add("settings");
  if (report.file) actions.add("report");
  return [...actions];
}
