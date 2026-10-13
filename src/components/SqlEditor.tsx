import { autocompletion, closeBrackets, completionKeymap } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import {
  MySQL,
  PostgreSQL,
  SQLite,
  StandardSQL,
  sql,
  type SQLNamespace,
} from "@codemirror/lang-sql";
import { HighlightStyle, bracketMatching, syntaxHighlighting } from "@codemirror/language";
import { Compartment, EditorState, Prec } from "@codemirror/state";
import {
  EditorView,
  drawSelection,
  highlightActiveLine,
  keymap,
  lineNumbers,
  placeholder,
} from "@codemirror/view";
import { tags as t } from "@lezer/highlight";
import { useEffect, useRef } from "react";
import type { DatabaseSchema, Engine } from "@/types/schema";

const theme = EditorView.theme(
  {
    "&": {
      height: "100%",
      backgroundColor: "var(--color-ink-0)",
      color: "var(--color-fg)",
      fontSize: "13px",
    },
    ".cm-content": {
      fontFamily: "var(--font-mono)",
      caretColor: "var(--color-cyan)",
      padding: "8px 0",
    },
    ".cm-gutters": {
      backgroundColor: "var(--color-ink-1)",
      color: "var(--color-fg-dim)",
      border: "none",
      borderRight: "1px solid var(--color-line)",
    },
    ".cm-activeLine": { backgroundColor: "rgb(45 212 230 / 0.04)" },
    ".cm-activeLineGutter": {
      backgroundColor: "rgb(45 212 230 / 0.06)",
      color: "var(--color-fg-muted)",
    },
    ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": {
      backgroundColor: "rgb(45 212 230 / 0.25) !important",
    },
    "&.cm-focused": { outline: "none" },
    ".cm-cursor": { borderLeftColor: "var(--color-cyan)" },
    ".cm-tooltip": {
      backgroundColor: "var(--color-ink-2)",
      border: "1px solid var(--color-line-strong)",
      color: "var(--color-fg)",
    },
    ".cm-tooltip-autocomplete ul li[aria-selected]": {
      backgroundColor: "var(--color-cyan-wash)",
      color: "var(--color-fg)",
    },
    ".cm-placeholder": { color: "var(--color-fg-dim)" },
  },
  { dark: true },
);

const highlight = HighlightStyle.define([
  { tag: [t.keyword, t.operatorKeyword], color: "#2dd4e6" },
  { tag: [t.string, t.special(t.string)], color: "#3ddc97" },
  { tag: [t.number, t.bool, t.null], color: "#f2b13c" },
  { tag: [t.comment, t.lineComment, t.blockComment], color: "#5f6b78", fontStyle: "italic" },
  { tag: [t.typeName, t.standard(t.name)], color: "#94a1af" },
  { tag: t.punctuation, color: "#94a1af" },
]);

/** Schema-aware completion: schemas, tables and columns come from the loaded metadata. */
export function completionSchema(schema: DatabaseSchema | undefined): SQLNamespace {
  const out: Record<string, Record<string, string[]>> = {};
  if (!schema) return out;
  for (const t of schema.tables) (out[t.namespace] ??= {})[t.name] = t.columns.map((c) => c.name);
  for (const v of schema.views) (out[v.namespace] ??= {})[v.name] = v.columns.map((c) => c.name);
  return out;
}

function dialectFor(engine: Engine | undefined) {
  switch (engine) {
    case "postgres":
      return PostgreSQL;
    case "mysql":
      return MySQL;
    case "sqlite":
      return SQLite;
    default:
      return StandardSQL;
  }
}

export function SqlEditor({
  value,
  onChange,
  onRun,
  schema,
  engine,
  readOnly = false,
}: {
  value: string;
  onChange: (v: string) => void;
  onRun?: () => void;
  schema?: DatabaseSchema;
  engine?: Engine;
  readOnly?: boolean;
}) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const lang = useRef(new Compartment());
  const onChangeRef = useRef(onChange);
  const onRunRef = useRef(onRun);
  useEffect(() => {
    onChangeRef.current = onChange;
    onRunRef.current = onRun;
  });

  useEffect(() => {
    if (!host.current) return;
    const v = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: value,
        extensions: [
          lineNumbers(),
          history(),
          drawSelection(),
          highlightActiveLine(),
          bracketMatching(),
          closeBrackets(),
          autocompletion(),
          placeholder("SELECT … FROM …   (Ctrl+Enter to run)"),
          lang.current.of(
            sql({
              dialect: dialectFor(engine),
              schema: completionSchema(schema),
              upperCaseKeywords: true,
            }),
          ),
          syntaxHighlighting(highlight),
          theme,
          EditorState.readOnly.of(readOnly),
          Prec.highest(
            keymap.of([
              {
                key: "Mod-Enter",
                run: () => {
                  onRunRef.current?.();
                  return true;
                },
              },
            ]),
          ),
          keymap.of([...completionKeymap, ...historyKeymap, ...defaultKeymap]),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) onChangeRef.current(u.state.doc.toString());
          }),
        ],
      }),
    });
    view.current = v;
    return () => {
      v.destroy();
      view.current = null;
    };
    // The editor is created once; props below are synced by the effects that follow.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // external value changes (Format, history) without clobbering the cursor on every keystroke
  useEffect(() => {
    const v = view.current;
    if (v && v.state.doc.toString() !== value)
      v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: value } });
  }, [value]);

  useEffect(() => {
    view.current?.dispatch({
      effects: lang.current.reconfigure(
        sql({
          dialect: dialectFor(engine),
          schema: completionSchema(schema),
          upperCaseKeywords: true,
        }),
      ),
    });
  }, [schema, engine]);

  return <div ref={host} className="h-full min-h-0 overflow-hidden" aria-label="SQL editor" />;
}
