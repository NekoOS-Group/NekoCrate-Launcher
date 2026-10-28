import { createLog4jDecoder } from "./log4jPresentation.ts";

export type LogLine = {
  time: string;
  kind: string;
  message: string;
  instanceId?: string | null;
};
export type LogLevel = "TRACE" | "DEBUG" | "INFO" | "WARN" | "ERROR" | "FATAL";
export type ParsedLog = {
  level: LogLevel | "";
  source: string;
  message: string;
  detail: "stack" | "cause" | "";
};
export type LogRow = ParsedLog & {
  id: number;
  time: string;
  lastTime: string;
  count: number;
  scope: string;
  filterLevel: LogLevel | "";
  group: number;
  search: string;
  originals: LogLine[];
};
export type LogFilter = { scope: string; level: string; query: string };

const levels = "TRACE|DEBUG|INFO|WARN(?:ING)?|ERROR|FATAL";
const threadLevel = new RegExp(`^\\[([^\\]\\n]+)\/(${levels})\\]\\s*:? ?`, "i");
const levelPrefix = new RegExp(
  `^(?:\\[(${levels})\\]|(${levels})(?=[:\\s]|$))[: ]*`,
  "i",
);
const timestamp =
  /^(?:\[(?:\d{4}-\d{2}-\d{2}[ T])?\d{2}:\d{2}:\d{2}(?:[.,]\d+)?(?:Z|[+-]\d{2}:?\d{2})?\]|(?:\d{4}-\d{2}-\d{2}[ T])?\d{2}:\d{2}:\d{2}(?:[.,]\d+)?(?:Z|[+-]\d{2}:?\d{2})?(?=\s))\s*/;
const eventLevels: Record<string, LogLevel> = {
  info: "INFO",
  stage: "INFO",
  complete: "INFO",
  game_started: "INFO",
  game_exit: "INFO",
  warning: "WARN",
  retry: "WARN",
  error: "ERROR",
  game_crash: "ERROR",
};

/** Presentation only: the original stream remains untouched for copying/debugging. */
export function parseLog(message: string, kind = "log"): ParsedLog {
  let body = message
    // Terminal colours, OSC links/titles, and non-printing control characters.
    .replace(/\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g, "")
    .replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, "")
    .replace(/[\x00-\x08\x0b-\x1f\x7f]/g, "")
    .replace(timestamp, "");
  let level: LogLevel | "" = eventLevels[kind] || "";
  let source = "";
  const thread = body.match(threadLevel);
  const prefix = thread || body.match(levelPrefix);
  if (prefix) {
    const token = (thread ? thread[2] : prefix[1] || prefix[2]).toUpperCase();
    level = (token === "WARNING" ? "WARN" : token) as LogLevel;
    body = body.slice(prefix[0].length);
    const logger = body.match(/^\[([^\]\n]+)\]:? ?/);
    source = [logger?.[1], thread?.[1]].filter(Boolean).join(" · ");
    if (logger) body = body.slice(logger[0].length);
    // Forge wraps stderr/stdout in another call-site prefix, even on stack frames.
    if (logger && /^(STDERR|STDOUT)\/?$/.test(logger[1])) {
      const callsite = body.match(/^\[([^\]\n]+:\d+)\]: ?/);
      if (callsite) {
        source += ` · ${callsite[1]}`;
        body = body.slice(callsite[0].length);
      }
    }
  }
  const detail =
    /^\s*(?:at\s+[\w.$/]+\([^\n]*\)|\.\.\. \d+ (?:more|common frames omitted))/.test(
      body,
    )
      ? "stack"
      : /^\s*(?:Caused by:|Suppressed:|Exception in thread |(?:[\w$]+\.)*[\w$]*(?:Exception|Error)(?=:|$))/.test(
            body,
          )
        ? "cause"
        : "";
  return { level, source, message: body, detail };
}

let nextId = 0;
const identity = (message: string) => message;
const cache = new WeakMap<
  LogLine,
  { id: number; parsed: ParsedLog; search: string }
>();

/** Cache parsing per immutable stream entry; only adjacent equal entries collapse. */
export function presentLogs(
  lines: readonly LogLine[],
  translate: (message: string) => string = identity,
): LogRow[] {
  return createLogPresenter().update(lines, translate);
}

/** Append batches without rebuilding historical rows; reset after clear or locale changes. */
export function createLogPresenter() {
  let rows: LogRow[] = [];
  const xml = createLog4jDecoder();
  const contexts = new Map<string, { level: LogLevel | ""; group: number }>();
  let previous: LogLine | undefined;
  let first: LogLine | undefined;
  let processed = 0;
  let translationKey: unknown;
  return {
    update(
      lines: readonly LogLine[],
      translate: (message: string) => string = identity,
      key: unknown = translate,
    ): LogRow[] {
      if (
        key !== translationKey ||
        lines.length < processed ||
        (processed > 0 &&
          (lines[0] !== first || lines[processed - 1] !== previous))
      ) {
        rows = [];
        contexts.clear();
        xml.clear();
        processed = 0;
        previous = undefined;
      }
      translationKey = key;
      first = lines[0];
      for (let i = processed; i < lines.length; i++) {
        for (const display of xml.push(lines[i])) {
          const { line } = display;
          let entry = cache.get(line);
          if (!entry || display.parsed) {
            const plain = parseLog(
              display.parsed?.message ?? line.message,
              line.kind,
            );
            const parsed = display.parsed
              ? {
                  ...display.parsed,
                  message: plain.message,
                  detail: display.parsed.detail || plain.detail,
                }
              : plain;
            entry = {
              id: entry?.id ?? nextId++,
              parsed,
              search: `${parsed.source}\n${parsed.message}`.toLowerCase(),
            };
            cache.set(line, entry);
          }
          const parsed =
            line.kind === "log"
              ? entry.parsed
              : { ...entry.parsed, message: translate(entry.parsed.message) };
          const last = rows.at(-1);
          const scope = line.instanceId || "launcher";
          const context = contexts.get(scope);
          const filterLevel =
            parsed.detail === "cause"
              ? "ERROR"
              : parsed.detail === "stack"
                ? context?.level || "ERROR"
                : parsed.level;
          const group =
            parsed.detail &&
            context &&
            ["WARN", "ERROR", "FATAL"].includes(context.level)
              ? context.group
              : entry.id;
          if (
            last &&
            previous?.kind === line.kind &&
            previous.instanceId === line.instanceId &&
            parsed.message !== "" &&
            last.message === parsed.message &&
            last.level === parsed.level &&
            last.source === parsed.source
          ) {
            last.count++;
            last.lastTime = line.time;
            last.originals.push(...(display.originals || [line]));
          } else {
            rows.push({
              ...parsed,
              id: entry.id,
              time: line.time,
              lastTime: line.time,
              count: 1,
              scope,
              filterLevel,
              group,
              search:
                line.kind === "log"
                  ? entry.search
                  : `${parsed.source}\n${parsed.message}`.toLowerCase(),
              originals: display.originals || [line],
            });
          }
          contexts.set(scope, {
            level: filterLevel,
            group: rows.at(-1)!.group,
          });
          previous = line;
        }
      }
      processed = lines.length;
      previous = lines.at(-1);
      return rows.slice();
    },
  };
}

/** Preserve matching exception blocks, including their otherwise unlabelled frames. */
export function filterLogs(rows: readonly LogRow[], filter: LogFilter) {
  const query = filter.query.trim().toLowerCase();
  if (filter.scope === "all" && filter.level === "all" && !query) return rows;
  const scoped =
    filter.scope === "all"
      ? rows
      : rows.filter((row) => row.scope === filter.scope);
  const queryGroups = query
    ? new Set(
        scoped
          .filter((row) => row.search.includes(query))
          .map((row) => row.group),
      )
    : null;
  return scoped.filter((row) => {
    if (queryGroups && !queryGroups.has(row.group)) return false;
    switch (filter.level) {
      case "warning":
        return ["WARN", "ERROR", "FATAL"].includes(row.filterLevel);
      case "error":
        return ["ERROR", "FATAL"].includes(row.filterLevel);
      case "info":
        return row.filterLevel === "INFO";
      case "debug":
        return ["DEBUG", "TRACE"].includes(row.filterLevel);
      default:
        return true;
    }
  });
}

export function copyLogRows(rows: readonly LogRow[]) {
  return rows
    .flatMap((row) =>
      row.originals.map((line) => `[${line.time}] ${line.message}`),
    )
    .join("\n");
}
