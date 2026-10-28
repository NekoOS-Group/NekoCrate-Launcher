<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { AlertCircle } from "lucide-vue-next";
import { Compartment, EditorState, Prec } from "@codemirror/state";
import {
  EditorView,
  drawSelection,
  highlightActiveLine,
  keymap,
  lineNumbers,
  tooltips,
} from "@codemirror/view";
import {
  defaultKeymap,
  history,
  historyKeymap,
  indentWithTab,
  isolateHistory,
} from "@codemirror/commands";
import {
  bracketMatching,
  HighlightStyle,
  indentOnInput,
  indentUnit,
  StreamLanguage,
  syntaxHighlighting,
  type StreamParser,
} from "@codemirror/language";
import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
import { json } from "@codemirror/lang-json";
import { toml } from "@codemirror/legacy-modes/mode/toml";
import { cfgLanguage } from "../cfgLanguage";
import {
  nextDiagnostic,
  previousDiagnostic,
  setDiagnostics,
} from "@codemirror/lint";
import { tags } from "@lezer/highlight";
import AnalysisWorker from "../configEditor.worker?worker&inline";
import type {
  ConfigAnalysisReply,
  ConfigFormat,
  ConfigIssue,
} from "../configSyntax";
import { locale, t } from "../i18n";

const props = defineProps<{
  modelValue: string;
  format: ConfigFormat;
  label: string;
  readonly?: boolean;
}>();
const emit = defineEmits<{
  "update:modelValue": [text: string];
  issues: [count: number];
  save: [];
}>();
const host = ref<HTMLElement>();
const issues = ref<ConfigIssue[]>([]);
const position = ref({ line: 1, column: 1 });
const formatting = ref(false);
const unavailable = ref(false);
const readOnly = new Compartment();
let view: EditorView | undefined;
let worker: Worker | undefined;
let timer: ReturnType<typeof setTimeout> | undefined;
let revision = 0;
let request = 0;
let destroyed = false;
const eol = props.modelValue.match(/\r\n|\r|\n/)?.[0] || "\n";
const normalized = (text: string) => text.replace(/\r\n?|\n/g, "\n");
const location = computed(() =>
  t("第 {line} 行，第 {column} 列", position.value),
);
const issueText = (entry: ConfigIssue) =>
  `${t("第 {line} 行，第 {column} 列", { line: entry.line, column: entry.column })} · ${t(entry.message)}`;
const diagnosticText = computed(() => issues.value.map(issueText).join("\n"));

function showDiagnostics() {
  if (!view) return;
  view.dispatch(
    setDiagnostics(
      view.state,
      issues.value.map((entry) => ({
        from: entry.from,
        to: entry.to,
        severity: entry.severity || "error",
        message: t(entry.message),
      })),
    ),
  );
}
function analyze(formatDocument = false) {
  clearTimeout(timer);
  if (!view || !worker || unavailable.value || view.composing) return;
  formatting.value = formatDocument;
  worker.postMessage({
    id: ++request,
    revision,
    text: view.state.doc.toString(),
    format: props.format,
    formatDocument,
  });
}
function scheduleAnalysis() {
  clearTimeout(timer);
  timer = setTimeout(() => analyze(), 300);
}
function format() {
  if (!props.readonly && !formatting.value) analyze(true);
}
function goToIssue() {
  const entry = issues.value[0];
  if (!view || !entry) return;
  view.dispatch({
    selection: { anchor: entry.from, head: entry.to },
    scrollIntoView: true,
  });
  view.focus();
}
function workerFailed() {
  unavailable.value = true;
  formatting.value = false;
  issues.value = [];
  emit("issues", 0);
  showDiagnostics();
}

onMounted(() => {
  if (!host.value) return;
  type TomlState = { inString: boolean; inArray: number };
  const tomlLanguage = StreamLanguage.define({
    ...(toml as StreamParser<TomlState>),
    indent: (state: TomlState, after: string, context) =>
      state.inString
        ? null
        : Math.max(0, state.inArray - (/^\s*\]/.test(after) ? 1 : 0)) *
          context.unit,
    languageData: {
      commentTokens: { line: "#" },
      closeBrackets: { brackets: ["[", "{", "'", '"', "'''", '"""'] },
    },
  });
  view = new EditorView({
    parent: host.value,
    state: EditorState.create({
      doc: props.modelValue,
      extensions: [
        props.format === "json"
          ? json()
          : props.format === "cfg"
            ? cfgLanguage
            : tomlLanguage,
        lineNumbers(),
        history(),
        drawSelection(),
        highlightActiveLine(),
        indentUnit.of(props.format === "cfg" ? "    " : "  "),
        indentOnInput(),
        bracketMatching(),
        closeBrackets(),
        readOnly.of([
          EditorState.readOnly.of(!!props.readonly),
          EditorView.editable.of(!props.readonly),
        ]),
        EditorView.contentAttributes.of({
          "aria-label": props.label,
          spellcheck: "false",
          autocapitalize: "off",
          autocomplete: "off",
        }),
        tooltips({ parent: host.value.closest("dialog") || document.body }),
        Prec.highest(
          keymap.of([
            {
              key: "Mod-Enter",
              run: () => {
                if (!props.readonly) emit("save");
                return true;
              },
            },
            {
              key: "Alt-Shift-f",
              run: () => {
                format();
                return true;
              },
            },
            { key: "F8", run: nextDiagnostic, shift: previousDiagnostic },
            {
              key: "Escape",
              run: (editor) => {
                editor.setTabFocusMode(2000);
                return true;
              },
            },
          ]),
        ),
        keymap.of([
          ...closeBracketsKeymap,
          ...defaultKeymap,
          ...historyKeymap,
          indentWithTab,
        ]),
        syntaxHighlighting(
          HighlightStyle.define([
            {
              tag: [tags.propertyName, tags.definition(tags.propertyName)],
              color: "var(--tab-color)",
            },
            { tag: tags.string, color: "#b4d6a2" },
            { tag: [tags.heading, tags.typeName], color: "var(--tab-color)" },
            { tag: tags.meta, color: "var(--muted)" },
            {
              tag: [tags.number, tags.bool, tags.null, tags.atom],
              color: "#e7b98a",
            },
            { tag: tags.comment, color: "var(--muted)", fontStyle: "italic" },
            { tag: [tags.bracket, tags.punctuation], color: "var(--ink)" },
          ]),
        ),
        EditorView.theme({
          "&": {
            height: "100%",
            fontSize: "12px",
            color: "var(--ink)",
            backgroundColor: "transparent",
          },
          "&.cm-focused": { outline: "none" },
          ".cm-scroller": {
            fontFamily: "ui-monospace, SFMono-Regular, Consolas, monospace",
            lineHeight: "1.65",
            overflow: "auto",
            overscrollBehavior: "contain",
          },
          ".cm-content": {
            padding: "8px 0 12px",
            caretColor: "var(--tab-color)",
          },
          ".cm-line": { padding: "0 12px 0 4px" },
          ".cm-gutters": {
            backgroundColor: "transparent",
            color: "var(--muted)",
            border: "none",
            fontSize: "10px",
          },
          ".cm-lineNumbers .cm-gutterElement": {
            minWidth: "34px",
            padding: "0 10px 0 6px",
          },
          ".cm-activeLine": {
            backgroundColor:
              "color-mix(in srgb, var(--tab-color) 5%, transparent)",
          },
          ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--tab-color)" },
          "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": {
            backgroundColor:
              "color-mix(in srgb, var(--tab-color) 22%, transparent)",
          },
          ".cm-matchingBracket": {
            color: "var(--tab-color)",
            backgroundColor:
              "color-mix(in srgb, var(--tab-color) 16%, transparent)",
            outline:
              "1px solid color-mix(in srgb, var(--tab-color) 35%, transparent)",
          },
          ".cm-tooltip": {
            color: "var(--ink)",
            backgroundColor: "var(--night)",
            border: "1px solid var(--glass-stroke)",
            fontSize: "11px",
          },
        }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            revision++;
            formatting.value = false;
            issues.value = [];
            emit("issues", 0);
            emit(
              "update:modelValue",
              update.state.doc.toString().replace(/\n/g, eol),
            );
            scheduleAnalysis();
          }
          if (update.docChanged || update.selectionSet) {
            const head = update.state.selection.main.head;
            const line = update.state.doc.lineAt(head);
            position.value = {
              line: line.number,
              column: head - line.from + 1,
            };
          }
        }),
        EditorView.domEventHandlers({
          compositionend: () => {
            scheduleAnalysis();
          },
        }),
      ],
    }),
  });
  try {
    worker = new AnalysisWorker();
    worker.onerror = workerFailed;
    worker.onmessage = (event: MessageEvent<ConfigAnalysisReply>) => {
      const result = event.data;
      if (
        destroyed ||
        !view ||
        result.id !== request ||
        result.revision !== revision
      )
        return;
      formatting.value = false;
      issues.value = result.issues;
      emit(
        "issues",
        result.issues.filter((entry) => entry.severity !== "warning").length,
      );
      showDiagnostics();
      if (result.edits?.length && !props.readonly && !view.composing) {
        view.dispatch({
          changes: result.edits,
          annotations: isolateHistory.of("full"),
          userEvent: "input.format",
        });
      }
    };
    analyze();
  } catch {
    workerFailed();
  }
});

watch(
  () => props.modelValue,
  (text) => {
    if (!view || normalized(text) === view.state.doc.toString()) return;
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: text },
      annotations: isolateHistory.of("full"),
    });
  },
);
watch(
  () => props.readonly,
  (locked) => {
    view?.dispatch({
      effects: readOnly.reconfigure([
        EditorState.readOnly.of(!!locked),
        EditorView.editable.of(!locked),
      ]),
    });
  },
);
watch(locale, showDiagnostics);
onBeforeUnmount(() => {
  destroyed = true;
  clearTimeout(timer);
  worker?.terminate();
  view?.destroy();
});
defineExpose({ format, formatting });
</script>

<template>
  <div class="config-code">
    <div ref="host" class="config-code-host" />
    <div class="config-code-status">
      <button
        v-if="issues.length"
        type="button"
        class="config-code-issue"
        :class="{ 'config-code-warning': issues[0]?.severity === 'warning' }"
        :title="diagnosticText"
        @click="goToIssue"
      >
        <AlertCircle :size="12" /><span>{{ issueText(issues[0]!) }}</span
        ><small v-if="issues.length > 1">+{{ issues.length - 1 }}</small>
      </button>
      <span v-else-if="unavailable">{{ t("编辑辅助暂不可用") }}</span>
      <span
        v-else
        :title="t('Tab 缩进；Esc 后按 Tab 移出编辑器；Ctrl/⌘ + Enter 保存')"
        >{{ location }}</span
      >
      <span>{{ props.format.toUpperCase() }}</span>
    </div>
  </div>
</template>

<style scoped>
.config-code {
  min-width: 0;
}
.config-code-host {
  height: clamp(180px, 30vh, 280px);
  min-height: 180px;
  max-height: 420px;
  resize: vertical;
  overflow: hidden;
}
.config-code-status {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-height: 26px;
  padding: 0 12px 4px;
  font: 10px/16px var(--mono);
  color: var(--muted);
}
.config-code-status > span:last-child {
  flex-shrink: 0;
}
.config-code-issue {
  display: flex;
  align-items: center;
  min-width: 0;
  gap: 6px;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--danger);
  text-align: left;
  font: inherit;
  cursor: pointer;
}
.config-code-issue > span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.config-code-warning {
  color: #e7b98a;
}
.config-code-issue > svg,
.config-code-issue > small {
  flex-shrink: 0;
}
</style>
