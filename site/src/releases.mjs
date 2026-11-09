export const repository = "NekoOS-Group/NekoCrate-Launcher";
export const repositoryUrl = `https://github.com/${repository}`;
export const releaseApi = `https://api.github.com/repos/${repository}/releases?per_page=20`;
const kinds = {
  windows: { installed: "-win-amd64-setup.exe", portable: "-win-amd64.exe" },
  macos: {
    installed: "-mac-appsilicon.dmg",
    portable: "-mac-appsilicon.app.tar.gz",
  },
  linux: { installed: "-linux-amd64.deb", portable: "-linux-amd64.elf" },
};

function safeReleaseUrl(value, kind) {
  if (typeof value !== "string") return false;
  try {
    const url = new URL(value);
    return (
      url.origin === "https://github.com" &&
      !url.username &&
      !url.password &&
      url.pathname.startsWith(`/${repository}/releases/${kind}/`)
    );
  } catch {
    return false;
  }
}

export function normalizeRelease(raw) {
  if (!raw || raw.draft || !Array.isArray(raw.assets)) return null;
  const tag = raw.tag_name ?? raw.tag;
  const url = raw.html_url ?? raw.url;
  const publishedAt = raw.published_at ?? raw.publishedAt;
  if (
    typeof tag !== "string" ||
    !/^[\w.+-]{1,80}$/.test(tag) ||
    !safeReleaseUrl(url, "tag") ||
    !Number.isFinite(Date.parse(publishedAt))
  )
    return null;
  const assets = raw.assets.flatMap((asset) => {
    const address = asset.browser_download_url ?? asset.url;
    if (
      !safeReleaseUrl(address, "download") ||
      typeof asset.name !== "string" ||
      !Number.isFinite(asset.size) ||
      asset.size <= 0 ||
      !new URL(address).pathname.startsWith(
        `/${repository}/releases/download/${encodeURIComponent(tag)}/`,
      )
    )
      return [];
    return [{ name: asset.name, url: address, size: asset.size }];
  });
  if (
    !Object.values(kinds).some((platform) =>
      Object.values(platform).some((suffix) =>
        assets.some(
          (asset) => asset.name === `NekoCrate-Launcher-${tag}${suffix}`,
        ),
      ),
    )
  )
    return null;
  return {
    tag,
    name: typeof raw.name === "string" ? raw.name : tag,
    url,
    publishedAt,
    prerelease: raw.prerelease === true,
    assets,
  };
}

export function latestRelease(raw) {
  if (!Array.isArray(raw)) return null;
  return (
    raw
      .map(normalizeRelease)
      .filter(Boolean)
      .sort(
        (a, b) => Date.parse(b.publishedAt) - Date.parse(a.publishedAt),
      )[0] || null
  );
}

export function releaseAsset(release, platform, mode) {
  const suffix = kinds[platform]?.[mode];
  return suffix
    ? release.assets.find(
        (asset) => asset.name === `NekoCrate-Launcher-${release.tag}${suffix}`,
      )
    : undefined;
}

export function detectPlatform(agent = "") {
  if (/Android|iPhone|iPad|iPod/i.test(agent)) return null;
  if (/Windows/i.test(agent)) return "windows";
  if (/Macintosh|Mac OS X/i.test(agent)) return "macos";
  if (/Linux|X11/i.test(agent)) return "linux";
  return null;
}

export function formatSize(bytes) {
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
