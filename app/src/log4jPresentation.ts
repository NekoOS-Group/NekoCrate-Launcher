import type { LogLine, LogLevel, ParsedLog } from "./logPresentation.ts";

export type DisplayLog = {
  line: LogLine;
  parsed?: ParsedLog;
  originals?: LogLine[];
};
type Pending = {
  lines: LogLine[];
  size: number;
  cdata: boolean;
  section: boolean;
};
const start = /^\s*<(?:log4j:)?event\b/i;
const wrapper =
  /^\s*(?:(?:<\/?log4j:[\w]+\b[^>]*>|<\/(?:Event|Message|Throwable)\s*>|\]\]>)\s*)+$/i;
const validLevels = new Set([
  "TRACE",
  "DEBUG",
  "INFO",
  "WARN",
  "ERROR",
  "FATAL",
]);

function entities(text: string) {
  return text.replace(
    /&(#x[\da-f]+|#\d+|lt|gt|amp|quot|apos);/gi,
    (original, code: string) => {
      const named: Record<string, string> = {
        lt: "<",
        gt: ">",
        amp: "&",
        quot: '"',
        apos: "'",
      };
      if (!code.startsWith("#")) return named[code.toLowerCase()] || original;
      const value =
        code[1].toLowerCase() === "x"
          ? parseInt(code.slice(2), 16)
          : Number(code.slice(1));
      return value > 0 &&
        value <= 0x10ffff &&
        !(value >= 0xd800 && value <= 0xdfff)
        ? String.fromCodePoint(value)
        : original;
    },
  );
}

/** Read only the known log envelope. Never interpret DTDs, external entities or HTML. */
function event(pending: Pending): DisplayLog | undefined {
  const text = pending.lines.map((line) => line.message).join("\n");
  const header = text.match(/^\s*<(?:log4j:)?event\b([^>]*)>/i);
  if (!header) return;
  const attrs: Record<string, string> = {};
  for (const attribute of header[1].matchAll(/([\w:]+)\s*=\s*(["'])(.*?)\2/g))
    attrs[attribute[1].toLowerCase()] = entities(attribute[3]);
  const level = (attrs.level || "").toUpperCase().replace(/^WARNING$/, "WARN");
  let section = "",
    position = header[0].length;
  const parts: string[] = [];
  let throwable = false;
  while (position < text.length) {
    if (text.startsWith("<![CDATA[", position)) {
      const end = text.indexOf("]]>", position + 9);
      if (section)
        parts.push(text.slice(position + 9, end < 0 ? text.length : end));
      position = end < 0 ? text.length : end + 3;
    } else if (text[position] === "<") {
      const end = text.indexOf(">", position);
      if (end < 0) break;
      const tag = text
        .slice(position, end + 1)
        .match(/^<(\/?)(?:log4j:)?(message|throwable)\b/i);
      if (tag) {
        if (tag[1]) section = "";
        else {
          section = tag[2].toLowerCase();
          if (parts.length) parts.push("\n");
          throwable ||= section === "throwable";
        }
      }
      position = end + 1;
    } else {
      const next = text.indexOf("<", position);
      const end = next < 0 ? text.length : next;
      if (section) parts.push(entities(text.slice(position, end)));
      position = end;
    }
  }
  const message = parts.join("").replace(/^\r?\n|\r?\n$/g, "");
  if (!message) return;
  return {
    line: pending.lines[0],
    originals: pending.lines,
    parsed: {
      level: validLevels.has(level) ? (level as LogLevel) : "",
      source: [attrs.logger, attrs.thread].filter(Boolean).join(" · "),
      message,
      detail: throwable ? "cause" : "",
    },
  };
}

/** XML events can span IPC batches; keep independent envelopes per game. */
export function createLog4jDecoder() {
  const pending = new Map<string, Pending>();
  function finish(scope: string): DisplayLog[] {
    const value = pending.get(scope);
    pending.delete(scope);
    if (!value) return [];
    const decoded = event(value);
    // Incomplete envelopes with no body yet are presentation-only structure.
    return decoded ? [decoded] : [];
  }
  return {
    clear: () => pending.clear(),
    push(line: LogLine): DisplayLog[] {
      const scope = line.instanceId || "launcher";
      let current = pending.get(scope);
      if (line.kind !== "log") return [...finish(scope), { line }];
      const output: DisplayLog[] = [];
      if (!current?.cdata && start.test(line.message)) {
        output.push(...finish(scope));
        current = { lines: [], size: 0, cdata: false, section: false };
        pending.set(scope, current);
      }
      if (!current) return wrapper.test(line.message) ? [] : [{ line }];
      if (
        !current.cdata &&
        !current.section &&
        !/^\s*</.test(line.message) &&
        line.message.trim()
      ) {
        return [...output, ...finish(scope), { line }];
      }
      current.lines.push(line);
      current.size += line.message.length;
      let closed = false;
      for (const token of line.message.matchAll(
        /<!\[CDATA\[|\]\]>|<\/(?:log4j:)?event\s*>|<\/?(?:log4j:)?(?:message|throwable)\b[^>]*>/gi,
      )) {
        if (token[0] === "<![CDATA[") current.cdata = true;
        else if (token[0] === "]]>") current.cdata = false;
        else if (!current.cdata) {
          if (/<\/(?:log4j:)?event/i.test(token[0])) closed = true;
          else
            current.section =
              !token[0].startsWith("</") && !token[0].endsWith("/>");
        }
      }
      // A broken stream cannot hold an unbounded pending event. Contents already
      // received are displayed, and the immutable raw store still retains all lines.
      if (closed || current.size > 1024 * 1024 || current.lines.length >= 2048)
        output.push(...finish(scope));
      return output;
    },
  };
}
