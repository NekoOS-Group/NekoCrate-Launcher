import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { mockIPC, mockWindows, clearMocks } from "@tauri-apps/api/mocks";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";

test("native UI operations and drag listeners work with the reduced permissions", async () => {
  const capabilities = JSON.parse(
    readFileSync(
      new URL("../src-tauri/capabilities/default.json", import.meta.url),
    ),
  );
  const permissions = new Set(
    capabilities.permissions.map((p) =>
      typeof p === "string" ? p : p.identifier,
    ),
  );
  const previous = globalThis.window;
  globalThis.window = { crypto: globalThis.crypto };
  try {
    const calls = [];
    mockIPC((command, args) => {
      const [, plugin, action] = /^plugin:([^|]+)\|(.+)$/.exec(command);
      const prefix = ["event", "window"].includes(plugin) ? "core:" : "";
      const permission = `${prefix}${plugin}:allow-${action.replaceAll("_", "-")}`;
      assert.ok(
        permissions.has(permission),
        `${command} requires ${permission}`,
      );
      calls.push({ command, args });
      return command === "plugin:event|listen" ? calls.length : null;
    });
    mockWindows("main");
    const win = getCurrentWindow();
    await win.setTitle("猫咪盒子启动器");
    await win.isMinimized();
    await win.minimize();
    await win.unminimize();
    await win.setFocus();
    const unlisten = await listen("launcher-event", () => {});
    const stopDrop = await win.onDragDropEvent(() => {});
    await stopDrop();
    await unlisten();
    assert.equal(
      calls.filter((c) => c.command === "plugin:event|unlisten").length,
      5,
    );
    await open({ directory: true });
    await open({
      multiple: true,
      filters: [{ name: "Mods", extensions: ["jar"] }],
    });
    await save({ defaultPath: "instance.zip" });
    await openUrl("https://github.com/NekoOS-Group/NekoCrate-Launcher");
  } finally {
    clearMocks();
    if (previous === undefined) delete globalThis.window;
    else globalThis.window = previous;
  }
});
