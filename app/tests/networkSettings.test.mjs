import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import {
  proxyKind,
  proxyAddressForKind,
  validProxySettings,
} from "../src/networkSettings.ts";

test("旧手动代理识别协议，切换时保留 IPv6 和端口，拒绝协议不匹配或凭据", () => {
  assert.equal(proxyKind("custom", "socks5h://localhost:1080"), "socks");
  assert.equal(proxyKind("custom", "https://localhost:7890"), "http");
  assert.equal(
    proxyAddressForKind("http://[::1]:7890", "socks"),
    "socks5h://[::1]:7890",
  );
  assert.equal(
    proxyAddressForKind("socks5://[::1]:1080", "http"),
    "http://[::1]:1080",
  );
  assert.equal(
    proxyAddressForKind("https://proxy.example", "socks"),
    "socks5h://proxy.example:443",
  );
  assert.equal(
    proxyAddressForKind("https://proxy.example", "http"),
    "https://proxy.example",
  );
  assert.equal(validProxySettings("http", "socks5://localhost:1080"), false);
  assert.equal(validProxySettings("socks", "http://localhost:8080"), false);
  assert.equal(
    validProxySettings("http", "http://user:password@localhost:8080"),
    false,
  );
  assert.equal(validProxySettings("socks", "socks5h://localhost:1080"), true);
});

test("关于页实际声明的外链具有桌面 opener 权限", () => {
  const about = readFileSync(
    new URL("../src/components/SettingsAbout.vue", import.meta.url),
    "utf8",
  );
  const capability = JSON.parse(
    readFileSync(
      new URL("../src-tauri/capabilities/default.json", import.meta.url),
      "utf8",
    ),
  );
  const allowed = new Set(
    capability.permissions
      .find((entry) => entry.identifier === "opener:allow-open-url")
      .allow.map((entry) => entry.url),
  );
  const links = [...about.matchAll(/https:[^'"\s]+/g)].map((match) => match[0]);
  assert.ok(links.length >= 2);
  for (const link of links)
    assert.ok(allowed.has(link), `Missing opener permission: ${link}`);
});
