export const quoteStyleLabels = { simple: "simple", splash: "splash" } as const;
export type QuoteStyle = keyof typeof quoteStyleLabels;
export const defaultQuoteAngles = { simple: 0, splash: -20 };
export const defaultQuoteScales = { simple: 100, splash: 100 };
export function readQuoteScales(
  value: string | null,
): Record<QuoteStyle, number> {
  const result = { ...defaultQuoteScales };
  try {
    const data = JSON.parse(value || "null");
    if (data?.version === 1)
      for (const style of ["simple", "splash"] as const) {
        const scale = data.quoteScales?.[style];
        if (typeof scale === "number" && Number.isFinite(scale))
          result[style] = Math.round(Math.max(60, Math.min(160, scale)));
      }
  } catch {
    /* Keep existing layouts at their original text size. */
  }
  return result;
}
export function readQuoteStyle(value: string | null): QuoteStyle {
  try {
    const data = JSON.parse(value || "null");
    if (
      data?.version === 1 &&
      ["splash", "minecraft"].includes(data.quoteStyle)
    )
      return "splash";
  } catch {
    /* Use simple text for older layouts. */
  }
  return "simple";
}
export function readQuoteAngles(
  value: string | null,
): Record<QuoteStyle, number> {
  const result = { ...defaultQuoteAngles };
  try {
    const data = JSON.parse(value || "null");
    if (data?.version === 1)
      for (const style of ["simple", "splash"] as const) {
        const angle = data.quoteAngles?.[style];
        if (typeof angle === "number" && Number.isFinite(angle))
          result[style] = Math.round(Math.max(-45, Math.min(45, angle)));
      }
  } catch {
    /* Older layouts keep the original angles. */
  }
  return result;
}
export const homeWidgetLabels = {
  quote: "一言",
  news: "Trends",
  servers: "服务器列表",
  favorites: "收藏版本",
  worlds: "最近存档",
  todos: "待办便签",
  clock: "像素时钟",
} as const;
export type HomeWidget = keyof typeof homeWidgetLabels;
export function readClockLayout(value: string | null) {
  let settings: any;
  try {
    const data = JSON.parse(value || "null");
    if (data?.version === 1) settings = data.clock;
  } catch {
    /* Keep defaults. */
  }
  const number = (
    value: unknown,
    min: number,
    max: number,
    fallback: number,
  ) =>
    typeof value === "number" && Number.isFinite(value)
      ? Math.round(Math.max(min, Math.min(max, value)))
      : fallback;
  return {
    scale: number(settings?.scale, 60, 160, 100),
    angle: number(settings?.angle, -45, 45, 0),
    date: settings?.date !== false,
  };
}
export type NewsDock = "left" | "right" | null;
export const defaultNewsHeight = 244;
export const minNewsHeight = 120;
export type NewsLayout = {
  align: "left" | "right";
  dock: NewsDock;
  height: number;
};
export function readNewsLayout(value: string | null): NewsLayout {
  try {
    const data = JSON.parse(value || "null");
    if (data?.version === 1)
      return {
        align: data.news?.align === "right" ? "right" : "left",
        dock: ["left", "right"].includes(data.news?.dock)
          ? data.news.dock
          : null,
        height:
          typeof data.news?.height === "number" &&
          Number.isFinite(data.news.height)
            ? Math.round(
                Math.max(minNewsHeight, Math.min(4096, data.news.height)),
              )
            : defaultNewsHeight,
      };
  } catch {
    /* Older layouts keep news visible and left aligned. */
  }
  return { align: "left", dock: null, height: defaultNewsHeight };
}
export const homeWidgets = Object.keys(homeWidgetLabels) as HomeWidget[];
export function readHomeWidgets(value: string | null): HomeWidget[] {
  try {
    const data = JSON.parse(value || "null");
    if (data?.version === 1 && Array.isArray(data.widgets))
      return homeWidgets.filter((id) => data.widgets.includes(id));
  } catch {
    /* Optional widgets never prevent startup. */
  }
  return [];
}
export type FeedSource = { url: string; name: string };
export type NewsItem = {
  title: string;
  url: string;
  date: string;
  source: string;
};
export type Quote = { text: string; source: string };
export const QUOTE_TYPES = ["a", "c", "d", "i"];
export const HITOKOTO_URL =
  "https://v1.hitokoto.cn/?encode=json&min_length=8&max_length=40&c=a&c=c&c=d&c=i";
export const presetFeeds: FeedSource[] = [
  {
    name: "Minecraft 官方动态",
    url: "https://launchercontent.mojang.com/v2/news.json",
  },
  {
    name: "猫咪盒子更新",
    url: "https://github.com/NekoOS-Group/NekoCrate-Launcher/releases.atom",
  },
];
export const NEWS_SOURCES_KEY = "nekocrate.news-sources.v1";
export const WIDGET_CACHE_KEY = "nekocrate.widget-cache.v1";
export function safeFeedUrl(input: string, base?: string): string | null {
  try {
    const url = new URL(input.trim(), base);
    if (
      !["http:", "https:"].includes(url.protocol) ||
      url.username ||
      url.password ||
      url.href.length > 2048
    )
      return null;
    url.hash = "";
    return url.href;
  } catch {
    return null;
  }
}
export function readFeedSources(value: string | null): FeedSource[] {
  try {
    const data = JSON.parse(value || "null");
    if (Array.isArray(data)) {
      const urls = new Set<string>();
      return data
        .flatMap((item) => {
          const url = typeof item?.url === "string" && safeFeedUrl(item.url);
          if (!url || urls.has(url)) return [];
          urls.add(url);
          return [
            {
              url,
              name:
                typeof item.name === "string"
                  ? item.name.slice(0, 80)
                  : new URL(url).hostname,
            },
          ];
        })
        .slice(0, 8);
    }
  } catch {
    /* Restore default subscriptions if their storage is damaged. */
  }
  return [{ ...presetFeeds[0] }];
}
function short(value: unknown, limit = 240): string {
  return typeof value === "string"
    ? value.replace(/\s+/g, " ").trim().slice(0, limit)
    : "";
}
export function parseQuote(text: string): Quote {
  const data = JSON.parse(text),
    quote = short(data.hitokoto, 500);
  if (
    Array.from(quote).length < 8 ||
    Array.from(quote).length > 40 ||
    !QUOTE_TYPES.includes(data.type)
  )
    throw new Error("一言不符合长度或题材范围");
  const author = short(data.from_who, 80),
    work = short(data.from, 100).replace(/^《|》$/g, "");
  return {
    text: quote,
    source: author || work ? `——${author}${work ? `《${work}》` : ""}` : "",
  };
}
function cleanText(value: string): string {
  // Read inert HTML as text only. Never insert remote markup into the document.
  return short(
    new DOMParser().parseFromString(value, "text/html").body.textContent,
  );
}
export function parseFeed(
  text: string,
  source: FeedSource,
  responseUrl = source.url,
): NewsItem[] {
  const items: NewsItem[] = [];
  const add = (
    title: string,
    link: string,
    date: string,
    base = responseUrl,
  ) => {
    const url = link && safeFeedUrl(link, base);
    title = cleanText(title);
    if (!url || !title || items.some((item) => item.url === url)) return;
    items.push({
      title,
      url,
      date: Number.isFinite(Date.parse(date))
        ? new Date(date).toISOString()
        : "",
      source: source.name,
    });
  };
  if (source.url === presetFeeds[0].url) {
    const data = JSON.parse(text);
    if (!Array.isArray(data.entries)) throw new Error("订阅源格式不受支持");
    for (const entry of data.entries)
      add(
        short(entry.title),
        short(entry.readMoreLink, 2048),
        short(entry.date),
      );
  } else {
    // RSS/Atom needs no additional XML dependency. Reject custom entities/DTDs.
    if (/<!DOCTYPE|<!ENTITY/i.test(text)) throw new Error("订阅源格式不受支持");
    const doc = new DOMParser().parseFromString(text, "application/xml");
    if (doc.querySelector("parsererror")) throw new Error("订阅源格式不受支持");
    const root = doc.documentElement;
    const atom = root.localName === "feed";
    if (!atom && !["rss", "RDF"].includes(root.localName))
      throw new Error("请输入 RSS 或 Atom 订阅地址");
    const child = (node: Element, name: string) =>
      Array.from(node.children).find((c) => c.localName === name);
    const rows = Array.from(
      doc.getElementsByTagNameNS("*", atom ? "entry" : "item"),
    ).slice(0, 100);
    for (const row of rows) {
      const links = Array.from(row.children).filter(
        (c) => c.localName === "link",
      );
      const link = atom
        ? links.find(
            (c) =>
              !c.getAttribute("rel") || c.getAttribute("rel") === "alternate",
          )
        : links[0];
      let base = responseUrl;
      for (const node of [root, row, link]) {
        const relative = node?.getAttribute("xml:base");
        if (relative) base = safeFeedUrl(relative, base) || base;
      }
      add(
        child(row, "title")?.textContent || "",
        atom ? link?.getAttribute("href") || "" : link?.textContent || "",
        (
          child(row, "published") ||
          child(row, "pubDate") ||
          child(row, "updated") ||
          child(row, "date")
        )?.textContent || "",
        base,
      );
    }
  }
  return items.slice(0, 30);
}
export function mergeNews(lists: NewsItem[][]): NewsItem[] {
  const seen = new Set<string>();
  return lists
    .flat()
    .sort((a, b) => (Date.parse(b.date) || 0) - (Date.parse(a.date) || 0))
    .filter((item) => {
      if (seen.has(item.url)) return false;
      seen.add(item.url);
      return true;
    })
    .slice(0, 60);
}
export type WidgetResponse = { text: string; url: string };
type Cached = { time: number; data: WidgetResponse };
export function readWidgetCache(value: string | null): Record<string, Cached> {
  try {
    const data = JSON.parse(value || "null");
    if (data && typeof data === "object")
      return Object.fromEntries(
        Object.entries(data)
          .filter(([url, value]) => {
            const c = value as Cached;
            return (
              safeFeedUrl(url) &&
              Number.isFinite(c?.time) &&
              typeof c?.data?.text === "string" &&
              c.data.text.length <= 600_000 &&
              safeFeedUrl(c.data.url)
            );
          })
          .slice(-10),
      ) as Record<string, Cached>;
  } catch {
    /* Fetch again when cache cannot be read. */
  }
  return {};
}
export function freshWidgetCache(
  cache: Cached | undefined,
  maxAge: number,
  now = Date.now(),
) {
  return !!cache && now >= cache.time && now - cache.time < maxAge;
}
