import type { ResourcePlatform, ResourceProject } from "./resources.ts";
export type ResourceRpc = <T>(
  op: string,
  data?: Record<string, unknown>,
) => Promise<T>;
export interface ProjectDetails {
  refresh_required?: boolean;
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
export function createProjectMedia(rpc: ResourceRpc, now = Date.now) {
  const images = new Map<string, string>();
  const pending = new Map<string, Promise<string>>();
  const projects = new Map<
    string,
    {
      time: number;
      attempted?: number;
      refreshing: boolean;
      value: Promise<ProjectDetails>;
      fallback?: Promise<ProjectDetails>;
    }
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
    known?: Pick<ResourceProject, "updated" | "latest_version">,
  ) {
    const key = `${platform}/${project}`;
    const previous = projects.get(key);
    // The ten-minute memory window only rechecks the local disk cache. The
    // backend keeps descriptions seven days; installation metadata has its own TTL.
    let entry = previous;
    if (
      !entry ||
      (!entry.refreshing && (refresh || now() - entry.time >= 600_000))
    ) {
      if (refresh && previous) previous.attempted = now();
      const value = rpc<ProjectDetails>("resource_project", {
        platform,
        project,
        refresh,
      });
      entry = {
        time: now(),
        attempted: refresh ? now() : previous?.attempted,
        refreshing: refresh,
        value,
        fallback: refresh ? previous?.value : undefined,
      };
      const current = entry;
      projects.delete(key);
      projects.set(key, current);
      if (projects.size > 48) projects.delete(projects.keys().next().value!);
      void value.then(
        () => {
          current.refreshing = false;
          current.fallback = undefined;
        },
        () => {
          if (projects.get(key) === current) {
            if (previous) projects.set(key, previous);
            else projects.delete(key);
          }
        },
      );
    }
    const current = entry;
    const readable =
      !refresh && current.refreshing && current.fallback
        ? current.fallback
        : current.value;
    return readable.then((value) => {
      // Keep old content during offline failures, with a retry cooldown rather
      // than re-requesting on every expand/collapse or from two visible views.
      const cooling =
        current.attempted !== undefined && now() - current.attempted < 60_000;
      const changed = newerProject(value, known);
      return {
        ...value,
        refresh_required: !cooling && (!!value.refresh_required || changed),
      };
    });
  }
  return { image, details };
}

function newerProject(
  cached: ProjectDetails,
  known?: Pick<ResourceProject, "updated" | "latest_version">,
) {
  if (!known) return false;
  const updated = Date.parse(known.updated || "");
  const previous = Date.parse(cached.project.updated || cached.updated || "");
  if (Number.isFinite(updated)) {
    if (!Number.isFinite(previous) || updated > previous) return true;
    if (updated < previous) return false; // An older search result cannot roll back a newer detail.
  }
  return (
    !!known.latest_version &&
    !!cached.project.latest_version &&
    known.latest_version !== cached.project.latest_version
  );
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
