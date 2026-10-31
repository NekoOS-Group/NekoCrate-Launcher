export type IsolationMode = "shared" | "version";
export interface LaunchDirectory {
  path: string;
  mode: IsolationMode;
  changed: boolean;
  protected_pack: boolean;
}
export interface InstanceLaunchOptions {
  isolation: IsolationMode | null;
  log_behavior: "crash" | "startup" | "manual";
  game_args: string[];
  environment: string[];
  window_mode: "default" | "windowed" | "fullscreen";
  width: number;
  height: number;
  minimize_launcher: boolean;
  jvm_args: string[];
}
export function instanceLaunchOptions(
  value?: Partial<InstanceLaunchOptions>,
): InstanceLaunchOptions {
  return {
    isolation: null,
    log_behavior: "crash",
    game_args: [],
    environment: [],
    window_mode: "default",
    width: 1280,
    height: 720,
    minimize_launcher: false,
    jvm_args: [],
    ...value,
  };
}
export function validLaunchSize(value: InstanceLaunchOptions) {
  return (
    value.window_mode !== "windowed" ||
    (Number.isInteger(value.width) &&
      value.width >= 320 &&
      value.width <= 7680 &&
      Number.isInteger(value.height) &&
      value.height >= 240 &&
      value.height <= 4320)
  );
}

export function shouldOpenGameLogs(
  kind: string,
  options?: Partial<InstanceLaunchOptions>,
) {
  const behavior = options?.log_behavior || "crash";
  return (
    (kind === "game_started" && behavior === "startup") ||
    (kind === "game_crash" && behavior !== "manual")
  );
}

interface LauncherWindow {
  isMinimized(): Promise<boolean>;
  minimize(): Promise<void>;
  unminimize(): Promise<void>;
  setFocus(): Promise<void>;
}
export function createLaunchWindowBehavior(
  window: () => LauncherWindow,
  error: (problem: unknown) => void,
) {
  const active = new Set<string>();
  let owned = false,
    queue = Promise.resolve();
  return (kind: string, id: string, options?: InstanceLaunchOptions) => {
    // Keep exit behind minimize, even if a JVM fails before its window appears.
    queue = queue
      .then(async () => {
        if (kind === "game_started") {
          if (shouldOpenGameLogs(kind, options)) {
            await window().unminimize();
            await window().setFocus();
            return;
          }
          if (!options?.minimize_launcher || active.has(id)) return;
          active.add(id);
          if (!(await window().isMinimized())) {
            await window().minimize();
            owned = true;
          }
        } else if (kind === "game_exit" || kind === "game_crash") {
          active.delete(id);
          if (shouldOpenGameLogs(kind, options) || (!active.size && owned)) {
            owned = false;
            await window().unminimize();
            await window().setFocus();
          } else if (!active.size) {
            owned = false;
          }
        }
      })
      .catch(error);
    return queue;
  };
}
