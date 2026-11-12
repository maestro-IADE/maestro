import { useEffect, useRef, useState } from "react";
import { DiffEditor, Editor } from "@monaco-editor/react";
import { languageOf, monaco, OPTIONS } from "./monaco";
import { ipc } from "./ipc";
import { isLightTheme, useSettings } from "./settings";
import { fileKey, useStore } from "./store";
import { RichText } from "./nodes/AgentNode";
import { X } from "./icons";
import { askDiscardEdits } from "./ui/Confirm";

type Doc ={ text: string; mtime: number; head: string; missing: boolean };

/**
 * A worktree file in Monaco over the stage: edit and save, or compare with HEAD. Follows the agent's edits on disk.
 * One per file tab; the ones not on screen stay mounted (hidden) so switching tabs keeps unsaved edits.
 */
export default function CodeView({ groupId, base, rel, diff, active }: { groupId: string; base: string; rel: string; diff: boolean; active: boolean }) {
  const openEditor = useStore((s) => s.openEditor);
  const closeTab = useStore((s) => s.closeTab);
  const setFileDirty = useStore((s) => s.setFileDirty);
  const theme = useSettings((s) => (isLightTheme(s.theme) ? "maestro-light" : "maestro-dark"));
  const autoSave = useSettings((s) => s.autoSave);
  const key = fileKey(base, rel);
  const loaded = useRef(false);
  const autoTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const draftTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const pendingDraft = useRef<string | null | undefined>(undefined); // typed but not yet sent to the backend
  const autoSaveRef = useRef(autoSave);
  autoSaveRef.current = autoSave;
  const [doc, setDoc] = useState<Doc | null>(null);
  const [err, setErr] = useState("");
  const [dirty, setDirty] = useState(false);
  const [stale, setStale] = useState(false); // changed on disk while there are unsaved edits
  const isMd = /\.(md|markdown|mdx)$/i.test(rel);
  const [preview, setPreview] = useState(isMd && !diff); // Markdown opens rendered; 편집 switches to the source
  const ed = useRef<monaco.editor.ICodeEditor | null>(null); // the editable one (the right side of a diff)
  const saved = useRef(""); // text on disk as last read or written
  const draft = useRef<string | null>(null); // carries unsaved edits across an edit ↔ diff switch
  const mtime = useRef(0);
  const dirtyRef = useRef(false);
  dirtyRef.current = dirty;

  const load = async () => {
    try {
      const head = await ipc.fileAtHead(base, rel);
      const f = await ipc.readTextFile(base, rel).catch((e) => {
        // deleted in the worktree: still worth comparing with HEAD
        if (head) return { text: "", mtime: 0, missing: true };
        throw e;
      });
      saved.current = f.text;
      mtime.current = f.mtime;
      if (!loaded.current) {
        // first open: this tab may have come from another group with unsaved edits
        loaded.current = true;
        const d = await ipc.wsDraftGet(key).catch(() => null);
        draft.current = d ?? null;
        setDirty(d != null && d !== f.text);
      } else {
        // re-read (the agent changed it, or "디스크 내용 불러오기"): nothing unsaved any more
        draft.current = null;
        void ipc.wsDraftSet(key, null);
        setDirty(false);
      }
      setDoc({ text: f.text, mtime: f.mtime, head, missing: "missing" in f });
      ed.current?.getModel()?.setValue(draft.current ?? f.text);
      setStale(false);
      setErr("");
    } catch (e) {
      setErr(String(e));
    }
  };

  const save = async (force = false) => {
    // in the Markdown preview the editor is unmounted (its model disposed): the draft holds the text
    const text = ed.current?.getModel() ? ed.current.getValue() : draft.current;
    if (text == null || doc?.missing) return;
    try {
      mtime.current = await ipc.writeTextFile(base, rel, text, mtime.current, force);
      saved.current = text;
      clearTimeout(draftTimer.current);
      void ipc.wsDraftSet(key, null);
      setDirty(ed.current?.getModel() ? ed.current.getValue() !== text : false); // typed on while it saved
      setStale(false);
    } catch (e) {
      if (String(e) === "CHANGED") setStale(true);
      else setErr(String(e));
    }
  };
  const saveRef = useRef(save);
  saveRef.current = save;

  useEffect(() => {
    load();
    // the agent may be editing this very file
    const t = setInterval(async () => {
      const m = await ipc.fileMtime(base, rel).catch(() => mtime.current);
      if (m === mtime.current) return;
      if (dirtyRef.current) setStale(true);
      else load();
    }, 2000);
    return () => clearInterval(t);
  }, [base, rel]);

  const mount = (e: monaco.editor.IStandaloneCodeEditor) => {
    ed.current = e;
    if (draft.current != null) e.setValue(draft.current);
    e.onDidChangeModelContent(() => {
      const v = e.getValue();
      draft.current = v;
      // kept by the backend (debounced), so the tab keeps its edits in whatever window or group it moves to
      clearTimeout(draftTimer.current);
      pendingDraft.current = v !== saved.current ? v : null;
      draftTimer.current = setTimeout(() => {
        pendingDraft.current = undefined;
        void ipc.wsDraftSet(key, v !== saved.current ? v : null);
      }, 300);
      setDirty(v !== saved.current);
      // auto-save (settings → 파일): a second after typing stops
      clearTimeout(autoTimer.current);
      if (autoSaveRef.current && v !== saved.current) autoTimer.current = setTimeout(() => saveRef.current(), 1000);
    });
    e.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => saveRef.current());
  };

  // the tab shows ● and asks before closing; an edit also keeps the tab (no longer a preview)
  useEffect(() => {
    setFileDirty(key, dirty);
    if (dirty) useStore.getState().pinTab({ kind: "file", base, rel, diff }, groupId);
  }, [dirty]);
  useEffect(
    () => () => {
      clearTimeout(autoTimer.current);
      clearTimeout(draftTimer.current);
      // the tab moved to another group or window (still open somewhere): its last keystrokes go with it
      const stillOpen = Object.values(useStore.getState().groups).some((g) => g.tabs.some((t) => t.kind === "file" && t.base === base && t.rel === rel));
      if (stillOpen && pendingDraft.current !== undefined) void ipc.wsDraftSet(key, pendingDraft.current);
      setFileDirty(key, false);
    },
    [],
  );
  // auto-save turned on with edits waiting: save them now rather than on the next keystroke
  useEffect(() => void (autoSave && dirtyRef.current && saveRef.current()), [autoSave]);

  const close = async () => {
    if (dirty && !(await askDiscardEdits(rel))) return;
    closeTab({ kind: "file", base, rel, diff }, groupId);
  };
  // per group: the same file open in two split groups gets two models (a shared one would be disposed by
  // whichever editor unmounts first)
  const uri = `file:///${groupId}/${base.replace(/\\/g, "/")}/${rel}`;
  const mode = diff ? "diff" : preview ? "preview" : "edit";
  const setMode = (m: typeof mode) => {
    setPreview(m === "preview");
    openEditor({ base, rel, diff: m === "diff" });
  };

  return (
    // nokey: React Flow (still mounted underneath) skips keys from here. Monaco types through EditContext, not a
    // textarea, so React Flow would otherwise take Space for panning and Backspace for deleting canvas blocks.
    <div className="code-view nokey" hidden={!active}>
      <div className="cv-head">
        <span className="cv-path ellipsis" title={rel}>
          {rel}
          {dirty && <span className="cv-dirty" title="저장하지 않음 (Ctrl+S로 저장)"> ●</span>}
        </span>
        <span className="spacer" />
        <span className="seg">
          <button data-on={mode === "edit" || undefined} onClick={() => setMode("edit")}>편집</button>
          {isMd && <button data-on={mode === "preview" || undefined} onClick={() => setMode("preview")}>미리보기</button>}
          <button data-on={mode === "diff" || undefined} onClick={() => setMode("diff")}>변경 비교</button>
        </span>
        <button className="icon" title="닫기" aria-label="닫기" onClick={close}><X /></button>
      </div>
      {stale && (
        <div className="cv-banner">
          <span>에이전트가 이 파일을 바꿨어요</span>
          <button onClick={load}>디스크 내용 불러오기</button>
          <button onClick={() => save(true)}>내 편집으로 덮어쓰기</button>
        </div>
      )}
      {err && <div className="err small wrap cv-err">{err}</div>}
      <div className="cv-body">
        {!doc ? null : diff ? (
          <DiffEditor
            original={doc.head}
            modified={draft.current ?? doc.text}
            language={languageOf(rel)}
            originalModelPath={`inmemory://head/${groupId}/${rel}`}
            modifiedModelPath={uri}
            theme={theme}
            options={{ ...OPTIONS, readOnly: doc.missing, originalEditable: false }}
            onMount={(d) => mount(d.getModifiedEditor())}
          />
        ) : preview ? (
          // Monaco has no Markdown viewer: rendered like agent replies, from the unsaved text if there is one
          <RichText className="cv-md" text={draft.current ?? doc.text} />
        ) : (
          <Editor path={uri} defaultValue={draft.current ?? doc.text} theme={theme} options={OPTIONS} onMount={mount} />
        )}
      </div>
    </div>
  );
}
