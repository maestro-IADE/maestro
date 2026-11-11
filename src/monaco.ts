import * as monaco from "monaco-editor";
import { loader } from "@monaco-editor/react";
import editorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import jsonWorker from "monaco-editor/esm/vs/language/json/json.worker?worker";
import cssWorker from "monaco-editor/esm/vs/language/css/css.worker?worker";
import htmlWorker from "monaco-editor/esm/vs/language/html/html.worker?worker";
import tsWorker from "monaco-editor/esm/vs/language/typescript/ts.worker?worker";

// Monaco from the bundle (the loader's default is a CDN), with its language workers built by Vite. Imported by the
// lazy views that use it (CodeView, MergeDiffDialog), so it loads only when one opens.
self.MonacoEnvironment = {
  getWorker(_, label) {
    if (label === "json") return new jsonWorker();
    if (label === "css" || label === "scss" || label === "less") return new cssWorker();
    if (label === "html" || label === "handlebars" || label === "razor") return new htmlWorker();
    if (label === "typescript" || label === "javascript") return new tsWorker();
    return new editorWorker();
  },
};
loader.config({ monaco });
// a lone file has no tsconfig or node_modules: type errors would be mostly noise, keep syntax errors only
for (const d of [monaco.typescript.typescriptDefaults, monaco.typescript.javascriptDefaults]) d.setDiagnosticsOptions({ noSemanticValidation: true });
// transparent so the panel's own (theme) background shows through
for (const [name, base] of [["maestro-dark", "vs-dark"], ["maestro-light", "vs"]] as const)
  monaco.editor.defineTheme(name, { base, inherit: true, rules: [], colors: { "editor.background": "#00000000", "editorGutter.background": "#00000000" } });

export { monaco };

export const OPTIONS: monaco.editor.IStandaloneEditorConstructionOptions = {
  fontFamily: "Cascadia Mono, Consolas, monospace",
  fontSize: 13,
  scrollBeyondLastLine: false,
  automaticLayout: true,
  renderWhitespace: "selection",
  minimap: { enabled: false }, // reviewing an agent's edits, not navigating a codebase
};

/** Monaco's language for a file name; the diff editor needs it spelled out (it falls back to plain text). */
export const languageOf = (rel: string) => {
  const name = rel.split("/").pop()!.toLowerCase();
  return monaco.languages.getLanguages().find((l) => l.filenames?.some((f) => f.toLowerCase() === name) || l.extensions?.some((x) => name.endsWith(x.toLowerCase())))?.id;
};
