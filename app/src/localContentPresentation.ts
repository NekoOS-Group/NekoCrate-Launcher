import type { ContentKind, LocalContentFile } from "./types.ts";
import type { ResourcePlatform, ResourceProject } from "./resources.ts";
import type { ResourceRpc } from "./projectPresentation.ts";

export interface ContentProject {
  platform: ResourcePlatform;
  id: string;
}
export interface ContentPresentation {
  icon?: string | null;
  project?: ContentProject | null;
}
export interface ContentScope {
  id: string;
  version: string;
  kind: ContentKind;
}
export function declaredProject(
  file: LocalContentFile,
): ContentProject | undefined {
  const declaration = file.declaration;
  if (
    declaration?.project &&
    declaration.source.toLowerCase().includes("curseforge")
  )
    return { platform: "curseforge", id: String(declaration.project) };
}

/** Only visible rows read archives. Share work across the grid/detail and cap decoded data. */
export function createLocalContentMedia(rpc: ResourceRpc) {
  const online = new Map<
    string,
    { time: number; value: Promise<string | undefined> }
  >();
  const onlineWaiters: (() => void)[] = [];
  let onlineActive = 0;
  function onlineIcon(project: ContentProject): Promise<string | undefined> {
    const key = `${project.platform}/${project.id}`;
    const cached = online.get(key);
    if (cached && Date.now() - cached.time < 600_000) return cached.value;
    const value = (async () => {
      if (onlineActive >= 4)
        await new Promise<void>((resolve) => onlineWaiters.push(resolve));
      else onlineActive++;
      try {
        const result = await rpc<ResourceProject>("resource_project_summary", {
          platform: project.platform,
          project: project.id,
        });
        return result.icon_url || undefined;
      } finally {
        const next = onlineWaiters.shift();
        if (next) next();
        else onlineActive--;
      }
    })();
    online.set(key, { time: Date.now(), value });
    if (online.size > 256) online.delete(online.keys().next().value!);
    void value.catch(() => {
      if (online.get(key)?.value === value) online.delete(key);
    });
    return value;
  }
  const cache = new Map<string, { time: number; value: ContentPresentation }>();
  const pending = new Map<string, Promise<ContentPresentation>>();
  const waiters: (() => void)[] = [];
  let active = 0,
    memory = 0;
  async function load(
    scope: ContentScope,
    file: LocalContentFile,
  ): Promise<ContentPresentation> {
    if (file.declaration && file.declaration.status !== "local")
      return { project: declaredProject(file) };
    const key = JSON.stringify([
      scope.id,
      scope.version,
      scope.kind,
      file.file_name,
      file.size,
    ]);
    const cached = cache.get(key);
    if (cached && Date.now() - cached.time < 60_000) return cached.value;
    const previous = pending.get(key);
    if (previous) return previous;
    const request = (async () => {
      if (active >= 4)
        await new Promise<void>((resolve) => waiters.push(resolve));
      else active++;
      try {
        const result = await rpc<ContentPresentation>(
          "version_content_presentation",
          { ...scope, file: file.file_name },
        );
        const icon =
          result.icon &&
          result.icon.length <= 720_000 &&
          /^data:image\/(?:png|jpeg|webp|gif);base64,[A-Za-z0-9+/]+=*$/.test(
            result.icon,
          )
            ? result.icon
            : null;
        const value = {
          icon,
          project: result.project || declaredProject(file),
        };
        const old = cache.get(key);
        memory -= old?.value.icon?.length || 0;
        cache.delete(key);
        cache.set(key, { time: Date.now(), value });
        memory += icon?.length || 0;
        while (memory > 12 * 1024 * 1024 || cache.size > 256) {
          const first = cache.keys().next().value!;
          memory -= cache.get(first)?.value.icon?.length || 0;
          cache.delete(first);
        }
        return value;
      } finally {
        const next = waiters.shift();
        if (next) next();
        else active--;
      }
    })();
    pending.set(key, request);
    try {
      return await request;
    } finally {
      pending.delete(key);
    }
  }
  return { load, onlineIcon };
}
const stores = new WeakMap<
  ResourceRpc,
  ReturnType<typeof createLocalContentMedia>
>();
export function localContentMedia(rpc: ResourceRpc) {
  let store = stores.get(rpc);
  if (!store) {
    store = createLocalContentMedia(rpc);
    stores.set(rpc, store);
  }
  return store;
}
