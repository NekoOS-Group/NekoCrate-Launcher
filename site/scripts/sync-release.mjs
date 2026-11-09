import { writeFile } from "node:fs/promises";
import { latestRelease, releaseApi } from "../src/releases.mjs";

const headers = { Accept: "application/vnd.github+json" };
if (process.env.GH_TOKEN)
  headers.Authorization = `Bearer ${process.env.GH_TOKEN}`;
const response = await fetch(releaseApi, {
  headers,
  signal: AbortSignal.timeout(15000),
});
if (!response.ok)
  throw new Error(`Release lookup failed: HTTP ${response.status}`);
const release = latestRelease(await response.json());
if (!release)
  throw new Error("No published release with desktop downloads found");
await writeFile(
  new URL("../src/release.json", import.meta.url),
  `${JSON.stringify(release, null, 2)}\n`,
);
console.log(
  `Download snapshot: ${release.tag} (${release.assets.length} assets)`,
);
