import { ref } from "vue";
import type { Instance } from "./types";
import type { WidgetRpc } from "./widgetClient";
export type VersionShortcut = { instance: string; version: string };
export type LaunchDestination =
  { server: string; world?: never } | { world: string; server?: never };
export type ServerShortcut = VersionShortcut & {
  id: string;
  name: string;
  address: string;
  icon?: string | null;
};
export type RecentWorld = VersionShortcut & {
  key: string;
  name: string;
  file: string;
  game_version: string | null;
  last_played: number;
  icon: string | null;
};
export type LocalWidgets = {
  servers: Omit<ServerShortcut, "id">[];
  worlds: RecentWorld[];
};
export type ServerStatus = {
  online: number;
  max: number;
  latency: number;
  motd: string;
  version: string;
  icon?: string | null;
};
export type Todo = { id: string; text: string; done: boolean };
export type Dashboard = {
  servers: ServerShortcut[];
  hiddenServers: string[];
  todos: Todo[];
};
export const DASHBOARD_KEY = "nekocrate.home-dashboard.v1";
export const shortcutKey = (value: VersionShortcut) =>
  JSON.stringify([value.instance, value.version]);
const text = (value: unknown, max: number) =>
  typeof value === "string" ? value.trim().slice(0, max) : "";
export function normalizeServerAddress(input: string): string | null {
  const value = input.trim().replace(/^minecraft:\/\//i, "");
  if (!value || value.length > 255 || /[\s\x00-\x1f\x7f/@?#\\]/.test(value))
    return null;
  try {
    const host =
      value.includes(":") &&
      !value.startsWith("[") &&
      (value.match(/:/g)?.length || 0) > 1
        ? `[${value}]`
        : value;
    const url = new URL(`http://${host}`);
    const explicit = /:(\d+)$/.exec(host)?.[1];
    if (host.endsWith(":")) return null;
    const port = explicit ? Number(explicit) : 25565;
    if (!url.hostname || port < 1 || port > 65535) return null;
    return `${url.hostname.toLowerCase()}${port === 25565 ? "" : `:${port}`}`;
  } catch {
    return null;
  }
}
export function readDashboard(raw: string | null): Dashboard {
  const empty: Dashboard = {
    servers: [],
    hiddenServers: [],
    todos: [],
  };
  try {
    const value = JSON.parse(raw || "null");
    if (value?.version !== 1) return empty;
    const unique = new Set<string>();
    empty.servers = (Array.isArray(value.servers) ? value.servers : [])
      .flatMap((item: any) => {
        const address = normalizeServerAddress(text(item?.address, 256));
        const id = text(item?.id, 80);
        if (!address || !id || unique.has(id)) return [];
        unique.add(id);
        return [
          {
            id,
            address,
            name: text(item.name, 80) || address,
            instance: text(item.instance, 80),
            version: text(item.version, 255),
          },
        ];
      })
      .slice(0, 32);
    empty.hiddenServers = (
      Array.isArray(value.hiddenServers) ? value.hiddenServers : []
    )
      .filter((s: unknown) => typeof s === "string")
      .slice(0, 256);
    empty.todos = (Array.isArray(value.todos) ? value.todos : [])
      .flatMap((item: any) => {
        const id = text(item?.id, 80),
          label = text(item?.text, 240);
        if (!id || !label || unique.has(id)) return [];
        unique.add(id);
        return [{ id, text: label, done: item.done === true }];
      })
      .slice(0, 100);
  } catch {
    /* Damaged optional widgets never prevent the launcher opening. */
  }
  return empty;
}
function stored() {
  try {
    return localStorage.getItem(DASHBOARD_KEY);
  } catch {
    return null;
  }
}
export const dashboard = ref(readDashboard(stored()));
export function saveDashboard(next: Dashboard) {
  // Validate, bound and detach data before publishing it to every widget.
  const value = readDashboard(JSON.stringify({ version: 1, ...next }));
  localStorage.setItem(DASHBOARD_KEY, JSON.stringify({ version: 1, ...value }));
  dashboard.value = value;
}
export function resolveShortcut(
  instances: Instance[],
  target: VersionShortcut,
) {
  const instance = instances.find((i) => i.id === target.instance);
  const version = instance?.versions.find((v) => v.id === target.version);
  return instance && version ? { instance, version } : undefined;
}
export const localWidgetData = ref<LocalWidgets>({ servers: [], worlds: [] });
let localLoaded = 0,
  localPending: Promise<void> | undefined;
export function loadLocalWidgets(rpc: WidgetRpc, fresh = false): Promise<void> {
  if (localPending) return localPending;
  if (
    !fresh &&
    Date.now() - localLoaded >= 0 &&
    Date.now() - localLoaded < 120_000
  )
    return Promise.resolve();
  const task = rpc<LocalWidgets>("home_widget_local").then((value) => {
    localWidgetData.value = value;
    localLoaded = Date.now();
  });
  localPending = task.finally(() => {
    localPending = undefined;
  });
  return localPending;
}
export const serverStatuses = ref<
  Record<string, { time: number; data?: ServerStatus; error?: string }>
>({});
const pendingServers = new Map<string, Promise<void>>();
export function loadServerStatus(
  rpc: WidgetRpc,
  address: string,
): Promise<void> {
  if (!serverStatuses.value[address]) {
    const entries = Object.entries(serverStatuses.value).sort(
      (a, b) => a[1].time - b[1].time,
    );
    for (const [old] of entries.slice(0, Math.max(0, entries.length - 127)))
      delete serverStatuses.value[old];
  }
  const cached = serverStatuses.value[address];
  if (
    cached &&
    Date.now() - cached.time >= 0 &&
    Date.now() - cached.time < 60_000
  )
    return Promise.resolve();
  const pending = pendingServers.get(address);
  if (pending) return pending;
  const task = rpc<ServerStatus>("home_server_status", { address })
    .then(
      (data) => {
        serverStatuses.value[address] = { time: Date.now(), data };
      },
      (error) => {
        serverStatuses.value[address] = {
          time: Date.now(),
          error: String(error),
        };
      },
    )
    .finally(() => pendingServers.delete(address));
  pendingServers.set(address, task);
  return task;
}

const digits = [
  "111101101101111",
  "010110010010111",
  "111001111100111",
  "111001111001111",
  "101101111001001",
  "111100111001111",
  "111100111101111",
  "111001010010010",
  "111101111101111",
  "111101111001111",
];
export function pixelTimePath(value: string) {
  let path = "";
  Array.from(value).forEach((char, index) => {
    const bits = char === ":" ? "000010000010000" : digits[Number(char)];
    if (!bits) return;
    Array.from(bits).forEach((on, cell) => {
      if (on === "1")
        path += `M${index * 4 + (cell % 3)} ${Math.floor(cell / 3)}h.82v.82h-.82z`;
    });
  });
  return path;
}
