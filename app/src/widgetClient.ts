import {
  WIDGET_CACHE_KEY,
  freshWidgetCache,
  readWidgetCache,
  type WidgetResponse,
} from "./homeWidgets.ts";
export type WidgetRpc = <T = unknown>(
  op: string,
  data?: Record<string, unknown>,
) => Promise<T>;
const pending = new Map<string, Promise<WidgetResponse>>();
function cache() {
  try {
    return readWidgetCache(localStorage.getItem(WIDGET_CACHE_KEY));
  } catch {
    return {};
  }
}
export function cachedWidget(url: string): WidgetResponse | undefined {
  return cache()[url]?.data;
}
export async function fetchWidget(
  rpc: WidgetRpc,
  url: string,
  maxAge: number,
  validate: (data: WidgetResponse) => void,
  force = false,
): Promise<WidgetResponse> {
  // A startup refresh may already be replacing a fresh disk entry. Remounts
  // and background callers should share that request instead of the old value.
  const existing = pending.get(url);
  if (existing) return existing;
  const cached = cache()[url];
  if (!force && freshWidgetCache(cached, maxAge)) {
    try {
      validate(cached.data);
      return cached.data;
    } catch {
      /* Replace malformed cached responses. */
    }
  }
  const task = (async () => {
    const data = await rpc<WidgetResponse>("home_widget_feed", { url });
    validate(data);
    try {
      const previous = cache();
      // Bound storage and discard subscriptions that have not been read recently.
      delete previous[url];
      const entries = [
        ...Object.entries(previous),
        [url, { time: Date.now(), data }],
      ].slice(-10);
      let encoded = JSON.stringify(Object.fromEntries(entries));
      while (encoded.length > 2_000_000 && entries.length > 1) {
        entries.shift();
        encoded = JSON.stringify(Object.fromEntries(entries));
      }
      localStorage.setItem(WIDGET_CACHE_KEY, encoded);
    } catch {
      /* A full/disabled cache must not hide successfully fetched content. */
    }
    return data;
  })();
  pending.set(url, task);
  try {
    return await task;
  } finally {
    pending.delete(url);
  }
}
