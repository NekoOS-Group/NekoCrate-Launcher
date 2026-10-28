import { format as formatJson, printParseErrorCode, visit } from "jsonc-parser";
import { parseTOML, ParseError, type AST } from "toml-eslint-parser";
import { analyzeCfg } from "./cfgSyntax.ts";

export type ConfigFormat = "json" | "toml" | "cfg";
export interface ConfigIssue {
  from: number;
  to: number;
  line: number;
  column: number;
  message: string;
  severity?: "error" | "warning";
}
export interface ConfigEdit {
  from: number;
  to: number;
  insert: string;
}
export interface ConfigAnalysis {
  issues: ConfigIssue[];
  edits?: ConfigEdit[];
}
export interface ConfigAnalysisRequest {
  id: number;
  revision: number;
  text: string;
  format: ConfigFormat;
  formatDocument: boolean;
}
export interface ConfigAnalysisReply extends ConfigAnalysis {
  id: number;
  revision: number;
}

const diagnosticMessages: Record<string, string> = {
  InvalidSymbol: "无效符号",
  InvalidNumberFormat: "数字格式无效",
  PropertyNameExpected: "属性名需要双引号",
  ValueExpected: "缺少值",
  ColonExpected: "缺少冒号",
  CommaExpected: "缺少逗号",
  CloseBraceExpected: "缺少右花括号",
  CloseBracketExpected: "缺少右方括号",
  EndOfFileExpected: "此处应结束文档",
  InvalidCommentToken: "JSON 不支持注释",
  UnexpectedEndOfComment: "注释未结束",
  UnexpectedEndOfString: "字符串未结束",
  UnexpectedEndOfNumber: "数字未结束",
  InvalidUnicode: "Unicode 转义无效",
  InvalidEscapeCharacter: "转义字符无效",
  InvalidCharacter: "无效字符",
  "Unterminated string constant": "字符串未结束",
  "Unterminated table-key": "表名未结束",
  "Unterminated array": "数组未结束",
  "Unterminated inline table": "内联表未结束",
  "Empty bare keys are not allowed": "键名不能为空",
  "Must be a newline": "此处需要换行",
  "Expected equal (=) token": "缺少等号",
  "Unspecified values are invalid": "缺少值",
  "Expected comma (,) token": "缺少逗号",
  "Defining a key multiple times is invalid": "键名重复定义",
  "Unexpected character": "无效字符",
  "Unexpected token": "此处不应出现该内容",
  "Control characters (codes < 0x1f and 0x7f) are not allowed":
    "不允许控制字符",
  "The key, equals sign, and value must be on the same line":
    "键名、等号和值必须在同一行",
  "No newlines are allowed between the curly braces unless they are valid within a value":
    "内联表中此处不允许换行",
  "Underscores are allowed between digits": "下划线只能出现在数字之间",
  "Unexpected spaces": "此处不允许空格",
  "Three or more quotes are not permitted": "此处不允许连续多个引号",
  "Unexpected invalid date": "日期无效",
  "Unexpected invalid time": "时间无效",
  "Leading zeros are not allowed": "不允许多余的前导零",
  "Trailing comma is not permitted in an inline table": "内联表不允许尾随逗号",
  "Invalid character in escape sequence": "转义字符无效",
  "Consecutive dots are not permitted in keys": "键名不能包含连续的点",
  "Keys cannot end with a dot": "键名不能以点结尾",
  "Keys cannot start with a dot": "键名不能以点开头",
};

function issue(
  text: string,
  offset: number,
  length: number,
  message: string,
): ConfigIssue {
  const from = Math.max(0, Math.min(text.length, offset));
  const before = text.slice(0, from);
  return {
    from,
    to: Math.min(text.length, from + Math.max(1, length)),
    line: before.split("\n").length,
    column: from - before.lastIndexOf("\n"),
    message: diagnosticMessages[message] || message,
  };
}

// Only edit whitespace BETWEEN parser tokens. Comments, multiline strings, key
// ordering, dates and numeric lexemes (including 64-bit integers) stay untouched.
function formatToml(text: string, ast: AST.TOMLProgram): ConfigEdit[] {
  const tokens: (AST.Token | AST.Comment)[] = [
    ...ast.tokens,
    ...ast.comments,
  ].sort((a, b) => a.range[0] - b.range[0]);
  const edits: ConfigEdit[] = [];
  let previous: AST.Token | AST.Comment | undefined;
  let depth = 0;
  const eol = text.includes("\r\n") ? "\r\n" : "\n";
  const punctuation = (token?: AST.Token | AST.Comment) =>
    token?.type === "Punctuator" ? token.value : "";
  for (const token of tokens) {
    const from = previous?.range[1] || 0,
      to = token.range[0];
    const gap = text.slice(from, to);
    const left = punctuation(previous),
      right = punctuation(token);
    if (left === "[" || left === "{") depth++;
    if (left === "]" || left === "}") depth--;
    let insert = gap;
    if (!previous || /\n/.test(gap)) {
      const indent = Math.max(
        0,
        depth - (right === "]" || right === "}" ? 1 : 0),
      );
      insert = gap
        .replace(/[ \t]+(?=\r?\n)/g, "")
        .replace(/[ \t]*$/, "  ".repeat(indent));
    } else if (token.type === "Block") insert = "  ";
    else if (
      left === "." ||
      right === "." ||
      right === "," ||
      left === "[" ||
      right === "]"
    )
      insert = "";
    else if (left === "=" || right === "=" || left === ",") insert = " ";
    else if (left === "{" || right === "}")
      insert = left === "{" && right === "}" ? "" : " ";
    if (insert !== gap) edits.push({ from, to, insert });
    previous = token;
  }
  const from = previous?.range[1] || 0;
  const tail = text.slice(from);
  let insert = tail.replace(/[ \t]+(?=\r?\n|$)/g, "");
  if (tokens.length && !insert.endsWith("\n")) insert += eol;
  if (tail !== insert) edits.push({ from, to: text.length, insert });
  return edits;
}

export function analyzeConfig(
  text: string,
  format: ConfigFormat,
  formatDocument = false,
): ConfigAnalysis {
  if (
    text.length > 512 * 1024 ||
    new TextEncoder().encode(text).length > 512 * 1024
  ) {
    return { issues: [issue(text, 0, 1, "配置内容无效或超过 512 KiB")] };
  }
  const bom = text.startsWith("\ufeff") ? 1 : 0;
  const body = text.slice(bom);
  const issues: ConfigIssue[] = [];
  let edits: ConfigEdit[] | undefined;
  try {
    if (format === "json") {
      visit(
        body,
        {
          onError: (code, offset, length) => {
            if (issues.length < 20)
              issues.push(
                issue(text, offset + bom, length, printParseErrorCode(code)),
              );
          },
        },
        {
          disallowComments: true,
          allowTrailingComma: false,
          allowEmptyContent: false,
        },
      );
      if (!issues.length && formatDocument) {
        edits = formatJson(body, undefined, {
          tabSize: 2,
          insertSpaces: true,
          eol: body.includes("\r\n") ? "\r\n" : "\n",
          insertFinalNewline: true,
        }).map((edit) => ({
          from: edit.offset,
          to: edit.offset + edit.length,
          insert: edit.content,
        }));
      }
    } else if (format === "cfg") {
      const result = analyzeCfg(body, formatDocument);
      issues.push(
        ...result.issues.map((entry) => ({
          ...entry,
          from: entry.from + bom,
          to: entry.to + bom,
          column: entry.column + (entry.line === 1 ? bom : 0),
        })),
      );
      edits = result.edits;
    } else {
      const ast = parseTOML(body, { tomlVersion: "1.1" });
      if (formatDocument) edits = formatToml(body, ast);
    }
  } catch (error) {
    issues.push(
      issue(
        text,
        error instanceof ParseError ? error.index + bom : bom,
        1,
        error instanceof Error ? error.message : String(error),
      ),
    );
  }
  return {
    issues,
    edits: edits?.map((edit) => ({
      ...edit,
      from: edit.from + bom,
      to: edit.to + bom,
    })),
  };
}
