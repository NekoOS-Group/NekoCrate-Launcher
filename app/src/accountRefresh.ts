interface AccountRefreshOptions {
  selected: () => { id: string; expires_at: number } | null;
  enabled: () => boolean;
  refresh: (id: string) => Promise<boolean>;
  failed: (error: unknown) => void;
  now?: () => number;
  wallNow?: () => number;
}

const REFRESH_MARGIN = 60_000;

/** Events only inspect local expiry; authorization refresh starts near expiry. */
export function createAccountRefresher(options: AccountRefreshOptions) {
  const now = options.now || (() => performance.now());
  const wallNow = options.wallNow || Date.now;
  const nextCheck = new Map<string, number>();
  const pending = new Map<string, Promise<void>>();
  const deadlines = new Map<string, { expires: number; monotonic: number }>();
  let disposed = false;
  return {
    fresh(id: string) {
      nextCheck.delete(id);
    },
    check(): Promise<void> {
      const account = options.selected();
      if (disposed || !account || !options.enabled()) return Promise.resolve();
      const { id } = account;
      const running = pending.get(id);
      if (running) return running;
      const expires = Number.isFinite(account.expires_at)
        ? account.expires_at * 1000
        : 0;
      let deadline = deadlines.get(id);
      if (!deadline || deadline.expires !== expires) {
        deadline = {
          expires,
          monotonic: now() + Math.max(0, expires - wallNow() - REFRESH_MARGIN),
        };
        deadlines.set(id, deadline);
      }
      // Wall time notices sleep/forward jumps; monotonic time prevents a clock
      // rollback from postponing a refresh beyond the observed token lifetime.
      if (wallNow() < expires - REFRESH_MARGIN && now() < deadline.monotonic)
        return Promise.resolve();
      if (now() < (nextCheck.get(id) ?? 0)) return Promise.resolve();
      const request = Promise.resolve()
        .then(() =>
          disposed || !options.enabled() || options.selected()?.id !== id
            ? false
            : options.refresh(id),
        )
        .then((updated) => {
          // A normal successful refresh supplies a new expiry. Guard short-lived
          // responses too, without marking an unchanged profile fresh for hours.
          nextCheck.set(id, now() + (updated ? 60_000 : 15_000));
        })
        .catch((error: unknown) => {
          nextCheck.set(id, now() + 60_000);
          if (!disposed) options.failed(error);
        })
        .finally(() => pending.delete(id));
      pending.set(id, request);
      return request;
    },
    dispose() {
      disposed = true;
      nextCheck.clear();
      deadlines.clear();
    },
  };
}
