import assert from "node:assert/strict";
import test from "node:test";
import { parseTOML } from "toml-eslint-parser";
import { analyzeConfig } from "../src/configSyntax.ts";
import { readCfgLine } from "../src/cfgSyntax.ts";
import { cfgLanguage } from "../src/cfgLanguage.ts";
import { EditorState } from "@codemirror/state";
import { insertNewlineAndIndent } from "@codemirror/commands";
import {
  ensureSyntaxTree,
  getIndentation,
  indentUnit,
} from "@codemirror/language";

function formatted(text, format) {
  const result = analyzeConfig(text, format, true);
  assert.deepEqual(result.issues, []);
  let output = text;
  for (const edit of [...result.edits].reverse()) {
    assert.match(
      text.slice(edit.from, edit.to),
      /^\s*$/u,
      "Only whitespace may be replaced",
    );
    assert.match(edit.insert, /^\s*$/u);
    output = output.slice(0, edit.from) + edit.insert + output.slice(edit.to);
  }
  assert.deepEqual(
    analyzeConfig(output, format, true),
    { issues: [], edits: [] },
    "Formatting is idempotent",
  );
  return output;
}

test("JSON formatting preserves large integers, numeric lexemes, escapes, key order and duplicate keys", () => {
  const input =
    '{"z":9007199254740993,"precise":1.2300e+19,"s":"\\u0061", "z":2,"nested":[true,false,null]}';
  const result = formatted(input, "json");
  for (const literal of ["9007199254740993", "1.2300e+19", '"\\u0061"'])
    assert.ok(result.includes(literal));
  assert.equal(result.match(/"z"/g).length, 2);
  assert.ok(result.indexOf('"z"') < result.indexOf('"precise"'));
  assert.ok(result.includes('\n  "z": 9007199254740993,'));
});

test("TOML formatting changes whitespace while preserving comments, quoted keys, dates and multiline strings", () => {
  const input = `# 保留说明\n[ "猫猫" . render ]\nvalue=9007199254740993 # 保留注释\nwhen=1979-05-27T07:32:00-08:00\nprecise=1.2000e+3\ntext="""\n    # 这是字符串，不能调整其缩进 = [ ]\n  猫猫\n"""\nliteral='''\n    content\n'''\nitems=[\n1,{a=2,b=[3,4]}, # another comment\n5,\n]\n[[ cats ]]\nname='Neko'\n`;
  const output = formatted(input, "toml");
  const lexemes = (text) => {
    const ast = parseTOML(text);
    return [...ast.tokens, ...ast.comments]
      .sort((a, b) => a.range[0] - b.range[0])
      .map((token) => [token.type, token.value]);
  };
  assert.deepEqual(lexemes(output), lexemes(input));
  assert.ok(output.includes("value = 9007199254740993  # 保留注释"));
  assert.ok(output.includes("items = [\n  1, { a = 2, b = [3, 4] },"));
  assert.ok(output.includes("    # 这是字符串，不能调整其缩进 = [ ]"));
});

test("formatting respects UTF-8 BOM and CRLF without converting file content", () => {
  for (const [format, body] of [
    ["json", '{\r\n"a":1,"b":2\r\n}'],
    ["toml", "x=1\r\ny=[\r\n2,3\r\n]\r\n"],
  ]) {
    const result = formatted("\ufeff" + body, format);
    assert.ok(result.startsWith("\ufeff"));
    assert.equal(result.replaceAll("\r\n", "").includes("\n"), false);
  }
});

test("syntax errors include accurate positions and cannot produce destructive format edits", () => {
  for (const [format, text] of [
    ["json", '{\n  "猫": 1\n  "b": 2\n}'],
    ["json", '{"a":1,}'],
    ["json", "// comment\n{}"],
    ["json", ""],
    ["toml", "cat = 1\ncat = 2"],
    ["toml", "cat = [1, 2"],
    ["toml", '"猫" = "unfinished'],
  ]) {
    const result = analyzeConfig(text, format, true);
    assert.ok(result.issues.length);
    assert.equal(result.edits, undefined);
    for (const issue of result.issues) {
      const lines = text.slice(0, issue.from).split("\n");
      assert.equal(issue.line, lines.length);
      assert.equal(issue.column, lines.at(-1).length + 1);
      assert.ok(
        issue.from >= 0 && issue.to >= issue.from && issue.to <= text.length,
      );
    }
  }
  assert.match(analyzeConfig("x=1\nx=2", "toml").issues[0].message, /重复/);
});

test("valid TOML 1.1 and JSON scalar documents are accepted; oversized text is bounded", () => {
  for (const text of ["", "# comments only", "x={\na=1,\n}", '"猫"="喜欢猫咪"'])
    assert.deepEqual(analyzeConfig(text, "toml").issues, []);
  for (const text of ["null", "false", "1", '"cat"'])
    assert.deepEqual(analyzeConfig(text, "json").issues, []);
  const tooLarge = analyzeConfig(
    'x="' + "猫".repeat(200000) + '"',
    "toml",
    true,
  );
  assert.equal(tooLarge.issues.length, 1);
  assert.match(tooLarge.issues[0].message, /512 KiB/);
  assert.equal(tooLarge.edits, undefined);
});

test("Forge CFG formatting only changes structural indentation, preserving values and list contents", () => {
  const input =
    '# Configuration file\r\n~CONFIG_VERSION: 1\r\n\r\ngeneral {\r\n# [default: true]\r\nB:enabled=true\r\n  "猫咪设置" {\r\nS:"Title = # {}"=  # title { } < > ;  \r\nI:large=9007199254740993\r\nD:precise=1.2300e+19\r\nS:items <\r\n    # literal, not a comment\r\n      {"cat":"minecraft:cat", "query":"a > b"}\r\n    value=with spaces  \r\n\r\n >\r\n}\r\n}\r\n';
  const output = formatted("\ufeff" + input, "cfg");
  assert.ok(
    output.startsWith("\ufeff# Configuration file\r\n~CONFIG_VERSION: 1"),
  );
  assert.ok(output.includes('    B:enabled=true\r\n    "猫咪设置" {'));
  assert.ok(
    output.includes('        S:"Title = # {}"=  # title { } < > ;  \r\n'),
  );
  assert.ok(
    output.includes(
      '    # literal, not a comment\r\n      {"cat":"minecraft:cat", "query":"a > b"}\r\n    value=with spaces  \r\n\r\n        >',
    ),
  );
  assert.deepEqual(
    output
      .replace(/^\ufeff/, "")
      .split("\r\n")
      .map((x) => x.trimStart()),
    input.split("\r\n").map((x) => x.trimStart()),
  );
  assert.equal(output.replaceAll("\r\n", "").includes("\n"), false);
});

test("CFG highlighting respects typed values, quoted names, INI sections and literal lists", () => {
  for (const [line, kind, lastStyle] of [
    ['B:"Enabled cat"=false', "property", "bool"],
    ["I:limit=42", "property", "number"],
    ["S:title=true # raw text {", "property", "string"],
    ['S:"a=b"=true', "property", "string"],
    ["D:ratio=-1.2e+3", "property", "number"],
    ["[render]", "section", "heading"],
    ["; comment", "comment", "comment"],
    ["# [default: true]", "comment", "comment"],
    ['"nested category" {', "category-open", "bracket"],
  ]) {
    const parsed = readCfgLine(line);
    assert.equal(parsed.kind, kind);
    assert.equal(parsed.tokens.at(-1).style, lastStyle);
  }
  for (const text of [
    "# not a comment",
    "}",
    "B:enabled=true",
    'minecraft:cat > {name:"Neko"}',
  ]) {
    const line = readCfgLine(text, "S");
    assert.equal(line.kind, "list-value");
    assert.equal(line.tokens[0].style, "string");
  }
  assert.equal(readCfgLine("   >", "S").kind, "list-close");
});

test("CFG structural diagnostics are advisory and formatting does not touch incomplete files", () => {
  for (const [input, expected] of [
    ["# Configuration file\ngeneral {\n B:on=true\n", "分组未结束"],
    ["# Configuration file\ngeneral {\n S:items <\n# literal", "列表未结束"],
    ["# Configuration file\n}", "左花括号"],
  ]) {
    const analysis = analyzeConfig(input, "cfg", true);
    assert.ok(analysis.issues.some((x) => x.message.includes(expected)));
    assert.equal(analysis.edits, undefined);
    for (const issue of analysis.issues) {
      assert.equal(issue.severity, "warning");
      const lines = input.slice(0, issue.from).split("\n");
      assert.equal(issue.line, lines.length);
      assert.equal(issue.column, lines.at(-1).length + 1);
    }
  }
});

test("unknown CFG/INI dialects are never rejected or rewritten as Forge", () => {
  for (const input of [
    "; renderer\n[render]\n  enabled = true\npath = C:\\猫猫\\minecraft\nvalue = {raw} # text",
    '{"general":{"a":1}}',
    "# custom cfg\nsetting = {\n   raw text",
    "# Configuration file\ngeneral {\nB:on=true\ncustom directive here\n}",
  ])
    assert.deepEqual(analyzeConfig(input, "cfg", true), {
      issues: [],
      edits: [],
    });
});

test("CFG language indentation tracks nested categories and literal lists without treating values as braces", () => {
  for (const [text, after, expected] of [
    ["general {\n", "", 4],
    ["general {\n    nested {\n", "", 8],
    ["general {\n    S:items <\n        {literal}\n", "", 8],
    ["general {\n    S:items <\n", "    >", 4],
    ["general {\n    S:value={not a group\n", "}", 0],
  ]) {
    const state = EditorState.create({
      doc: text + after,
      extensions: [cfgLanguage, indentUnit.of("    ")],
    });
    ensureSyntaxTree(state, state.doc.length, 1000);
    assert.equal(getIndentation(state, text.length), expected);
  }
});

test("Enter inside a newly paired CFG group indents the body and leaves its closing brace outside", () => {
  let state = EditorState.create({
    doc: "general {}",
    selection: { anchor: 9 },
    extensions: [cfgLanguage, indentUnit.of("    ")],
  });
  insertNewlineAndIndent({
    state,
    dispatch: (tr) => {
      state = tr.state;
    },
  });
  assert.equal(state.doc.toString(), "general {\n    \n}");
  assert.equal(state.selection.main.head, 14);
});
