export interface LoaderVersion {
  version: string;
  stable: boolean;
  recommended?: boolean;
}

/** Small per-game/per-loader cache; concurrent prefetch and selection share a request. */
export function createLoaderCatalog(
  fetchVersions: (game: string, kind: string) => Promise<LoaderVersion[]>,
  now: () => number = () => performance.now(),
) {
  const entries = new Map<
    string,
    { versions: LoaderVersion[]; expires: number }
  >();
  const requests = new Map<string, Promise<LoaderVersion[]>>();
  const selections = new Map<string, string>();
  const key = (game: string, kind: string) => JSON.stringify([game, kind]);
  return {
    peek(game: string, kind: string) {
      return entries.get(key(game, kind));
    },
    remember(game: string, kind: string, version: string) {
      if (version) selections.set(key(game, kind), version);
    },
    selected(game: string, kind: string, versions: readonly LoaderVersion[]) {
      const previous = selections.get(key(game, kind));
      return (
        versions.find((v) => v.version === previous)?.version ||
        versions.find((v) => v.recommended)?.version ||
        versions.find((v) => v.stable)?.version ||
        versions[0]?.version ||
        ""
      );
    },
    load(
      game: string,
      kind: string,
      refresh = false,
    ): Promise<LoaderVersion[]> {
      const id = key(game, kind);
      const cached = entries.get(id);
      if (!refresh && cached && cached.expires > now())
        return Promise.resolve(cached.versions);
      const pending = requests.get(id);
      if (pending) return pending;
      const request = fetchVersions(game, kind)
        .then((versions) => {
          entries.set(id, { versions, expires: now() + 60 * 60 * 1000 });
          return versions;
        })
        .finally(() => requests.delete(id));
      requests.set(id, request);
      return request;
    },
  };
}
