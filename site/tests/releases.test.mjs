import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  normalizeRelease,
  latestRelease,
  releaseAsset,
  detectPlatform,
} from "../src/releases.mjs";
const snapshot = JSON.parse(
  readFileSync(new URL("../src/release.json", import.meta.url)),
);

test("published snapshot maps all six formats without confusing installers and standalone binaries", () => {
  const release = normalizeRelease(snapshot);
  assert.ok(release);
  const urls = [];
  for (const platform of ["windows", "macos", "linux"]) {
    for (const mode of ["installed", "portable"]) {
      const asset = releaseAsset(release, platform, mode);
      assert.ok(asset, `${platform}/${mode}`);
      urls.push(asset.url);
    }
  }
  assert.equal(new Set(urls).size, 6);
  assert.match(
    releaseAsset(release, "windows", "installed").name,
    /-setup\.exe$/,
  );
  assert.match(
    releaseAsset(release, "macos", "portable").name,
    /\.app\.tar\.gz$/,
  );
  assert.equal(releaseAsset(release, "arm-windows", "portable"), undefined);
});

test("release discovery includes previews and ignores drafts, empty releases and input ordering", () => {
  const old = {
    ...snapshot,
    publishedAt: "2025-01-01T00:00:00Z",
    prerelease: false,
  };
  const preview = {
    ...snapshot,
    publishedAt: "2026-11-09T00:00:00Z",
    prerelease: true,
  };
  const draft = {
    ...preview,
    publishedAt: "2027-01-01T00:00:00Z",
    draft: true,
  };
  const empty = { ...draft, draft: false, assets: [] };
  assert.deepEqual(
    latestRelease([old, draft, empty, preview]),
    normalizeRelease(preview),
  );
  assert.equal(latestRelease({ message: "API rate limit exceeded" }), null);
});

test("GitHub API payloads normalize to the same checked snapshot format", () => {
  const raw = {
    ...snapshot,
    tag: undefined,
    url: undefined,
    publishedAt: undefined,
    tag_name: snapshot.tag,
    html_url: snapshot.url,
    published_at: snapshot.publishedAt,
    assets: snapshot.assets.map((a) => ({
      ...a,
      url: undefined,
      browser_download_url: a.url,
    })),
  };
  assert.deepEqual(normalizeRelease(raw), normalizeRelease(snapshot));
});

test("download links cannot point outside the project or to another release", () => {
  for (const url of [
    "javascript:alert(1)",
    "https://example.com/file.exe",
    "https://github.com/other/repo/releases/download/x/file.exe",
    `https://github.com/NekoOS-Group/NekoCrate-Launcher/releases/download/other/${snapshot.assets[0].name}`,
  ]) {
    assert.equal(
      normalizeRelease({
        ...snapshot,
        assets: [
          {
            name: `NekoCrate-Launcher-${snapshot.tag}-win-amd64.exe`,
            url,
            size: 100,
          },
        ],
      }),
      null,
    );
  }
  assert.equal(normalizeRelease({ ...snapshot, tag: "../release" }), null);
  assert.equal(
    normalizeRelease({
      ...snapshot,
      url: "https://example.com/releases/tag/x",
    }),
    null,
  );
});

test("mobile browsers are not mistaken for supported desktop platforms", () => {
  assert.equal(detectPlatform("Mozilla/5.0 (Linux; Android 15)"), null);
  assert.equal(detectPlatform("iPhone; CPU iPhone OS like Mac OS X"), null);
  assert.equal(detectPlatform("Windows NT 10.0; Win64; x64"), "windows");
  assert.equal(detectPlatform("Macintosh; Intel Mac OS X 10_15_7"), "macos");
  assert.equal(detectPlatform("X11; Linux x86_64"), "linux");
  assert.equal(detectPlatform(""), null);
});

test("a missing architecture stays unavailable instead of silently using a different format", () => {
  const release = normalizeRelease({
    ...snapshot,
    assets: snapshot.assets.filter((a) => !a.name.endsWith(".elf")),
  });
  assert.equal(releaseAsset(release, "linux", "portable"), undefined);
  assert.ok(releaseAsset(release, "linux", "installed"));
});
