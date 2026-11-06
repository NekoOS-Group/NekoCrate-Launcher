export const homeToolLabels = {
  resources: "资源管理",
  settings: "全局设置",
  logs: "运行日志",
  store: "商城",
} as const;
export type HomeTool = keyof typeof homeToolLabels;
export const defaultHomeTools: HomeTool[] = ["resources", "settings", "logs"];
export const homeTools = Object.keys(homeToolLabels) as HomeTool[];
export function sanitizeHomeTools(value: unknown): HomeTool[] {
  if (!Array.isArray(value)) return [...defaultHomeTools];
  const tools = homeTools
    .filter((id) => value.includes(id))
    .sort((a, b) => value.indexOf(a) - value.indexOf(b));
  // Keep settings reachable, including layouts saved before it was mandatory.
  if (!tools.includes("settings")) tools.push("settings");
  return tools;
}
export function readHomeTools(value: string | null): HomeTool[] {
  try {
    const data = JSON.parse(value || "null");
    if (data?.version === 1) return sanitizeHomeTools(data.tools);
  } catch {
    /* Older or damaged layouts use the original toolbar. */
  }
  return [...defaultHomeTools];
}
