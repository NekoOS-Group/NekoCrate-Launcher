import type { ConfigAnalysis, ConfigEdit, ConfigIssue } from "./configSyntax";

export type CfgKind =
  | "blank"
  | "comment"
  | "section"
  | "property"
  | "directive"
  | "category-open"
  | "category-empty"
  | "category-close"
  | "list-open"
  | "list-close"
  | "list-value"
  | "unknown";
export interface CfgToken {
  from: number;
  to: number;
  style: string;
}
export interface CfgLine {
  kind: CfgKind;
  tokens: CfgToken[];
  valueType?: string;
}

// Forge 1.12 Configuration.load treats the entire RHS as a value, including
// spaces and #. Inside < ... > lists, #, quotes and braces are literal data.
// Keep those boundaries shared by highlighting, indentation and formatting.
// https://github.com/MinecraftForge/MinecraftForge/blob/1.12.x/src/main/java/net/minecraftforge/common/config/Configuration.java
export function readCfgLine(
  line: string,
  listType: string | null = null,
): CfgLine {
  const start = line.search(/\S/u);
  if (start < 0) return { kind: "blank", tokens: [] };
  const text = line.slice(start);
  const tokens: CfgToken[] = [];
  const token = (from: number, to: number, style: string) => {
    if (to > from) tokens.push({ from: start + from, to: start + to, style });
  };
  const value = (from: number, valueType?: string) => {
    const raw = text.slice(from).trim();
    const style =
      valueType === "S"
        ? "string"
        : /^(true|false)$/i.test(raw)
          ? "bool"
          : /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?$/i.test(raw)
            ? "number"
            : "string";
    token(from, text.length, style);
  };
  if (listType !== null) {
    if (/^>[ \t]*$/.test(text)) {
      token(0, 1, "bracket");
      return { kind: "list-close", tokens };
    }
    value(0, listType);
    return { kind: "list-value", tokens };
  }
  if (/^[#;]/.test(text)) {
    token(0, text.length, "comment");
    return { kind: "comment", tokens };
  }
  if (/^(?:~CONFIG_VERSION:|(?:START|END):\s*")/.test(text)) {
    token(0, text.length, "meta");
    return { kind: "directive", tokens };
  }
  const section = /^\[[^\]\r\n]+\]/.exec(text);
  if (section && /^[ \t]*(?:[#;].*)?$/.test(text.slice(section[0].length))) {
    token(0, section[0].length, "heading");
    token(section[0].length, text.length, "comment");
    return { kind: "section", tokens };
  }
  const emptyCategory =
    /^("[^"]+"|[\p{L}\p{N}_.-]+)[ \t]*\{[ \t]*\}[ \t]*(?:#.*)?$/u.exec(text);
  if (emptyCategory) {
    const open = text.indexOf("{", emptyCategory[1]!.length);
    const close = text.indexOf("}", open + 1);
    token(0, emptyCategory[1]!.length, "heading");
    token(open, open + 1, "bracket");
    token(close, close + 1, "bracket");
    token(close + 1, text.length, "comment");
    return { kind: "category-empty", tokens };
  }
  const category = /^("[^"]+"|[\p{L}\p{N}_.-]+)[ \t]*\{([ \t]*(?:#.*)?)$/u.exec(
    text,
  );
  if (category) {
    const bracket = text.length - category[2]!.length - 1;
    token(0, category[1]!.length, "heading");
    token(bracket, bracket + 1, "bracket");
    token(bracket + 1, text.length, "comment");
    return { kind: "category-open", tokens };
  }
  if (/^\}[ \t]*(?:#.*)?$/.test(text)) {
    token(0, 1, "bracket");
    token(1, text.length, "comment");
    return { kind: "category-close", tokens };
  }
  const list = /^(?:([A-Z]):)?("[^"]+"|[\p{L}\p{N}_.-]+)[ \t]*<[ \t]*$/u.exec(
    text,
  );
  if (list) {
    const prefix = list[1] ? 2 : 0;
    token(0, prefix, "typeName");
    token(prefix, prefix + list[2]!.length, "propertyName");
    const bracket = text.lastIndexOf("<");
    token(bracket, bracket + 1, "bracket");
    return { kind: "list-open", tokens, valueType: list[1] || "S" };
  }
  const property =
    /^(?:([A-Z]):)?("[^"\r\n]*"|'[^'\r\n]*'|[^=:#;]+)([=:])/.exec(text);
  if (property) {
    const prefix = property[1] ? 2 : 0;
    token(0, prefix, "typeName");
    token(prefix, prefix + property[2]!.trimEnd().length, "propertyName");
    const operator = property[0].length - 1;
    token(operator, operator + 1, "operator");
    value(operator + 1, property[1]);
    return { kind: "property", tokens, valueType: property[1] };
  }
  return { kind: "unknown", tokens };
}

export function analyzeCfg(
  text: string,
  formatDocument: boolean,
): ConfigAnalysis {
  const issues: ConfigIssue[] = [];
  const edits: ConfigEdit[] = [];
  const categories: { from: number; line: number; column: number }[] = [];
  let list:
    { from: number; line: number; column: number; type: string } | undefined;
  let forge = false;
  let uncertain = false;
  let number = 0;
  // Preserve all line endings, blank lines, comments, values and list payloads.
  for (const match of text.matchAll(/[^\r\n]*(?:\r\n|\r|\n|$)/g)) {
    const line = match[0].replace(/[\r\n]+$/, "");
    if (!match[0]) continue;
    number++;
    const parsed = readCfgLine(line, list?.type ?? null);
    const indent = line.match(/^[ \t]*/)?.[0] || "";
    const location = {
      from: match.index + indent.length,
      line: number,
      column: indent.length + 1,
    };
    let depth = categories.length;
    if (parsed.kind === "category-close") {
      if (categories.length) categories.pop();
      else if (issues.length < 20)
        issues.push({
          ...location,
          to: location.from + 1,
          message: "CFG 分组缺少对应的左花括号",
          severity: "warning",
        });
      depth = categories.length;
    }
    if (parsed.kind === "category-open") categories.push(location);
    if (parsed.kind === "list-close") list = undefined;
    else if (parsed.kind === "list-open")
      list = { ...location, type: parsed.valueType! };
    if (parsed.kind === "property" || parsed.kind === "list-open")
      forge ||= /^[ \t]*[A-Z]:/.test(line);
    if (parsed.kind === "comment" && line.trim() === "# Configuration file")
      forge = true;
    // Unknown dialects, INI sections and wrappers remain editable, without
    // interpreting their braces as Forge grammar or reindenting their content.
    if (
      ["unknown", "section"].includes(parsed.kind) ||
      (parsed.kind === "directive" && !line.startsWith("~CONFIG_VERSION:"))
    )
      uncertain = true;
    if (
      formatDocument &&
      parsed.kind !== "blank" &&
      parsed.kind !== "list-value"
    ) {
      const insert = "    ".repeat(Math.min(depth, 64));
      if (insert !== indent)
        edits.push({
          from: match.index,
          to: match.index + indent.length,
          insert,
        });
    }
  }
  if (!forge || uncertain)
    return { issues: [], ...(formatDocument ? { edits: [] } : {}) };
  if (list)
    issues.push({
      from: list.from,
      to: list.from + 1,
      line: list.line,
      column: list.column,
      message: "CFG 列表未结束，缺少 >",
      severity: "warning",
    });
  for (const category of categories.slice(
    0,
    20 - Math.min(20, issues.length),
  )) {
    issues.push({
      ...category,
      to: category.from + 1,
      message: "CFG 分组未结束，缺少 }",
      severity: "warning",
    });
  }
  return { issues, ...(formatDocument && !issues.length ? { edits } : {}) };
}
