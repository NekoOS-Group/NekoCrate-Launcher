import type { ResourcePlatform, ResourceProject } from "./resources.ts";
export type ResourceRpc = <T>(
  op: string,
  data?: Record<string, unknown>,
) => Promise<T>;
export interface ProjectDetails {
  project: ResourceProject;
  body: string;
  body_format: "markdown" | "html";
  authors: string[];
  categories: string[];
  updated: string | null;
  license: string | null;
  license_url?: string | null;
  links: { label: string; url: string }[];
  gallery: { url: string; title: string }[];
}
const imageHosts = new Set([
  "cdn.modrinth.com",
  "media.forgecdn.net",
  "mediafilez.forgecdn.net",
  "raw.githubusercontent.com",
  "user-images.githubusercontent.com",
  "i.imgur.com",
]);
export function projectImageUrl(value: string): string | undefined {
  try {
    const url = new URL(value);
    if (
      value.length <= 2048 &&
      url.protocol === "https:" &&
      !url.port &&
      !url.username &&
      !url.password &&
      imageHosts.has(url.hostname)
    )
      return url.href;
  } catch {
    /* Invalid URLs stay non-interactive. */
  }
}
export function projectLink(value: string, base?: string): string | undefined {
  if (value.trim().startsWith("#")) return;
  try {
    const url = new URL(value, base);
    if (
      ["https:", "http:"].includes(url.protocol) &&
      !url.username &&
      !url.password
    )
      return url.href;
  } catch {
    /* No executable or local URLs from remote descriptions. */
  }
}

/** Share in-flight requests across lists/details and cap image concurrency/memory. */
export function createProjectMedia(rpc: ResourceRpc) {
  const images = new Map<string, string>();
  const pending = new Map<string, Promise<string>>();
  const projects = new Map<
    string,
    { time: number; value: Promise<ProjectDetails> }
  >();
  const waiters: (() => void)[] = [];
  let active = 0,
    memory = 0;
  async function image(url: string) {
    if (!projectImageUrl(url)) throw new Error("Unsupported project image");
    const cached = images.get(url);
    if (cached) {
      images.delete(url);
      images.set(url, cached);
      return cached;
    }
    const previous = pending.get(url);
    if (previous) return previous;
    const request = (async () => {
      if (active >= 4)
        await new Promise<void>((resolve) => waiters.push(resolve));
      else active++;
      try {
        const result = await rpc<string>("resource_image", { url });
        if (
          !/^data:image\/(?:png|jpeg|webp|gif);base64,[A-Za-z0-9+/]+=*$/.test(
            result,
          ) ||
          result.length > 6 * 1024 * 1024
        )
          throw new Error("Invalid project image");
        images.set(url, result);
        memory += result.length;
        while (memory > 16 * 1024 * 1024 || images.size > 128) {
          const key = images.keys().next().value!;
          memory -= images.get(key)!.length;
          images.delete(key);
        }
        return result;
      } finally {
        const next = waiters.shift();
        if (next) next();
        else active--;
      }
    })();
    pending.set(url, request);
    try {
      return await request;
    } finally {
      pending.delete(url);
    }
  }
  function details(
    platform: ResourcePlatform,
    project: string,
    refresh = false,
  ) {
    const key = `${platform}/${project}`;
    const previous = projects.get(key);
    if (!refresh && previous && Date.now() - previous.time < 600_000)
      return previous.value;
    const value = rpc<ProjectDetails>("resource_project", {
      platform,
      project,
    });
    projects.delete(key);
    projects.set(key, { time: Date.now(), value });
    if (projects.size > 48) projects.delete(projects.keys().next().value!);
    void value.catch(() => {
      if (projects.get(key)?.value === value) projects.delete(key);
    });
    return value;
  }
  return { image, details };
}
const stores = new WeakMap<
  ResourceRpc,
  ReturnType<typeof createProjectMedia>
>();
export function projectMedia(rpc: ResourceRpc) {
  let store = stores.get(rpc);
  if (!store) {
    store = createProjectMedia(rpc);
    stores.set(rpc, store);
  }
  return store;
}
