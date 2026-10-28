import { StreamLanguage, type StreamParser } from "@codemirror/language";
import { readCfgLine, type CfgToken, type CfgKind } from "./cfgSyntax.ts";

interface CfgState {
  depth: number;
  listType: string | null;
  tokens: CfgToken[];
  tokenIndex: number;
  lineKind: CfgKind;
  valueType: string;
}

export const cfgParser: StreamParser<CfgState> = {
  name: "cfg",
  startState: () => ({
    depth: 0,
    listType: null,
    tokens: [],
    tokenIndex: 0,
    lineKind: "blank",
    valueType: "S",
  }),
  copyState: (state) => ({ ...state }),
  token(stream, state) {
    if (stream.sol()) {
      const parsed = readCfgLine(stream.string, state.listType);
      state.tokens = parsed.tokens;
      state.tokenIndex = 0;
      state.lineKind = parsed.kind;
      state.valueType = parsed.valueType || "S";
    }
    const token = state.tokens[state.tokenIndex];
    if (!token) {
      stream.skipToEnd();
      return null;
    }
    if (stream.pos < token.from) {
      stream.pos = token.from;
      return null;
    }
    stream.pos = token.to;
    state.tokenIndex++;
    // Apply transitions when their token is consumed, not at line start.
    // CodeMirror also tokenizes partial lines to calculate Enter indentation.
    if (token.style === "bracket") {
      if (state.lineKind.startsWith("category-")) {
        state.depth = Math.max(
          0,
          state.depth + (stream.string[token.from] === "{" ? 1 : -1),
        );
      } else if (state.lineKind === "list-open")
        state.listType = state.valueType;
      else if (state.lineKind === "list-close") state.listType = null;
    }
    return token.style;
  },
  indent(state, after, context) {
    const close =
      state.listType !== null ? /^\s*>/.test(after) : /^\s*}/.test(after);
    return (
      Math.max(
        0,
        state.depth + (state.listType === null ? 0 : 1) - (close ? 1 : 0),
      ) * context.unit
    );
  },
  languageData: {
    commentTokens: { line: "#" },
    // Deliberately exclude < and >: they may be comparisons or literal values.
    closeBrackets: { brackets: ["[", "{", '"', "'"] },
    indentOnInput: /^\s*[}>]$/,
  },
};

export const cfgLanguage = StreamLanguage.define(cfgParser);
