import { useEffect, useRef, useState, type CSSProperties } from "react";
import { copyText } from "../agentActions";
import { Copy, CopyPlus, Ellipsis, Pencil, Plus, Trash2 } from "../icons";
import {
  BUILTIN_SEEDS, CUSTOM_PREFIX, DEFAULT_CONTRAST, decodePalette, derivePalette, encodePalette, freeName, isHex, type CustomTheme,
} from "../palette";
import { DEFAULT_THEME, previewTheme, THEMES, useSettings } from "../settings";
import { ask } from "../ui/Confirm";
import { openMenu, pointOf, type MenuItem } from "../ui/Menu";
import { Group } from "./parts";

// 설정 → 모양 → 테마 (docs/features/ui/themes-and-motion.md "테마 카드", "내 팔레트"): one card per theme, each a small
// window drawn in that theme's real tokens; the pointer resting on a card shows the theme on the whole app until it
// leaves the cards. After the built-ins come the user's palettes, then 새 팔레트, which opens the editor below.

/** A small window in a theme's colours: the sidebar with a selected row, a gridded canvas with a block, the primary. */
export function ThemeMini({ theme, big }: { theme: string | CustomTheme; big?: boolean }) {
  const p = typeof theme === "string" ? null : derivePalette(theme);
  return (
    <span
      className="theme-mini"
      data-big={big || undefined}
      data-theme={p ? (p.light ? "custom-light" : "custom") : (theme as string)}
      style={p ? (p.vars as unknown as CSSProperties) : undefined}
      aria-hidden
    >
      <span className="tm-side"><i /><i data-sel /><i /><i /></span>
      <span className="tm-main"><span className="tm-node"><i /><i /></span><span className="tm-btn" /></span>
    </span>
  );
}

type Draft = { id: string | null; name: string; base: string; accent: string; contrast: number };

const newId = () => Date.now().toString(36) + Math.floor(Math.random() * 1296).toString(36);

export function ThemePicker() {
  const theme = useSettings((s) => s.theme);
  const customs = useSettings((s) => s.customThemes) ?? [];
  const set = useSettings((s) => s.set);
  const [draft, setDraft] = useState<Draft | null>(null);

  // the pointer resting on a card shows that theme on the whole app; leaving the cards puts the saved one back
  const timer = useRef<number | undefined>(undefined);
  const hover = (t: string | CustomTheme | null) => {
    window.clearTimeout(timer.current);
    if (draft) return; // the editor shows its own palette while open
    timer.current = window.setTimeout(() => previewTheme(t), t ? 220 : 0);
  };
  useEffect(() => () => {
    window.clearTimeout(timer.current);
    previewTheme(null);
  }, []);
  // while the editor is open the whole app wears the palette being edited
  useEffect(() => {
    if (!draft) return;
    window.clearTimeout(timer.current);
    previewTheme({ ...draft, id: draft.id ?? "draft" });
  }, [draft]);

  const pick = (id: string) => {
    window.clearTimeout(timer.current);
    set({ theme: id });
    if (!draft) previewTheme(null); // an open editor keeps showing its palette until it closes
  };

  const close = () => {
    setDraft(null);
    previewTheme(null);
  };

  const startNew = (seed = BUILTIN_SEEDS[DEFAULT_THEME]) =>
    setDraft({ id: null, name: freeName(customs.map((c) => c.name)), ...seed });

  const edit = (c: CustomTheme) => setDraft({ ...c });

  const duplicate = (c: CustomTheme) =>
    setDraft({ id: null, name: freeName(customs.map((x) => x.name), `${c.name} 복사본`), base: c.base, accent: c.accent, contrast: c.contrast });

  const remove = async (c: CustomTheme) => {
    const ok = await ask({
      title: `팔레트 지우기 · ${c.name}`,
      body: "지우면 되돌릴 수 없어요. 이 팔레트를 쓰고 있었다면 기본 테마로 돌아가요.",
      confirm: "지우기",
      danger: true,
    });
    if (!ok) return;
    const s = useSettings.getState();
    s.set({
      customThemes: (s.customThemes ?? []).filter((x) => x.id !== c.id),
      ...(s.theme === CUSTOM_PREFIX + c.id ? { theme: DEFAULT_THEME } : {}),
    });
    if (draft?.id === c.id) close();
  };

  const menuItems = (c: CustomTheme): MenuItem[] => [
    { label: "편집", icon: <Pencil size={14} />, onSelect: () => edit(c) },
    { label: "복제", icon: <CopyPlus size={14} />, onSelect: () => duplicate(c) },
    { label: "팔레트 문자열 복사", icon: <Copy size={14} />, onSelect: () => copyText(encodePalette(c)) },
    "sep",
    { label: "지우기…", icon: <Trash2 size={14} />, danger: true, onSelect: () => void remove(c) },
  ];

  const save = (d: Draft) => {
    const s = useSettings.getState();
    const list = s.customThemes ?? [];
    const name = d.name.trim().slice(0, 40) || freeName(list.filter((c) => c.id !== d.id).map((c) => c.name));
    const value = { name, base: d.base, accent: d.accent, contrast: d.contrast };
    if (d.id) {
      s.set({ customThemes: list.map((c) => (c.id === d.id ? { ...c, ...value } : c)) });
    } else {
      const id = newId();
      s.set({ customThemes: [...list, { id, ...value }], theme: CUSTOM_PREFIX + id });
    }
    close();
  };

  return (
    <>
      <Group title="테마" plain>
        <div className="theme-pick" onPointerLeave={() => hover(null)}>
          {THEMES.map((t) => (
            <div key={t.id} className="theme-cell">
              <button className="theme-card" data-on={t.id === theme || undefined} onPointerEnter={() => hover(t.id)} onClick={() => pick(t.id)}>
                <ThemeMini theme={t.id} />
                <span className="theme-card-label"><span className="ellipsis">{t.label}</span></span>
              </button>
            </div>
          ))}
          {customs.map((c) => (
            <div key={c.id} className="theme-cell" onContextMenu={(e) => { e.preventDefault(); openMenu(pointOf(e), menuItems(c)); }}>
              <button
                className="theme-card"
                data-on={CUSTOM_PREFIX + c.id === theme || undefined}
                title={`${c.name} · 내 팔레트`}
                onPointerEnter={() => hover(c)}
                onClick={() => pick(CUSTOM_PREFIX + c.id)}
                onDoubleClick={() => edit(c)}
              >
                <ThemeMini theme={c} />
                <span className="theme-card-label"><span className="ellipsis">{c.name}</span></span>
              </button>
              <button
                className="icon theme-more"
                title="팔레트 메뉴"
                aria-label={`${c.name} 메뉴`}
                onClick={(e) => openMenu(e.currentTarget, menuItems(c), { owner: e.currentTarget })}
              >
                <Ellipsis size={14} />
              </button>
            </div>
          ))}
          <div className="theme-cell">
            <button className="theme-card theme-new" onPointerEnter={() => hover(null)} onClick={() => startNew()} disabled={draft !== null}>
              <Plus />
              <span>새 팔레트</span>
            </button>
          </div>
        </div>
      </Group>
      {draft && <PaletteEditor key={draft.id ?? "new"} draft={draft} onChange={setDraft} onSave={save} onCancel={close} />}
    </>
  );
}

/** A colour: the system picker and its hex, typed or pasted (with or without #). */
function ColorField({ label, value, onChange }: { label: string; value: string; onChange: (hex: string) => void }) {
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);
  return (
    <div className="theme-field">
      <span>{label}</span>
      <span className="theme-color">
        <input type="color" aria-label={label} value={value} onChange={(e) => onChange(e.target.value.toLowerCase())} />
        <input
          type="text"
          aria-label={`${label} 색 코드`}
          spellCheck={false}
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            const v = e.target.value.trim();
            const hex = (v.startsWith("#") ? v : `#${v}`).toLowerCase();
            if (isHex(hex)) onChange(hex);
          }}
          onBlur={() => setText(value)}
        />
      </span>
    </div>
  );
}

/** 내 팔레트 편집: three choices (바탕, 강조, 대비), a name, a built-in to start from, and import / export as one line. */
function PaletteEditor({ draft, onChange, onSave, onCancel }: {
  draft: Draft;
  onChange: (d: Draft) => void;
  onSave: (d: Draft) => void;
  onCancel: () => void;
}) {
  const [paste, setPaste] = useState("");
  const [note, setNote] = useState<{ text: string; err?: boolean } | null>(null);
  const up = (p: Partial<Draft>) => onChange({ ...draft, ...p });
  const light = derivePalette(draft).light;

  const load = () => {
    const p = decodePalette(paste);
    if (!p) return setNote({ text: "팔레트 문자열이 아니에요. maestro-palette: 로 시작하는 한 줄을 붙여 넣어 주세요.", err: true });
    up(p);
    setPaste("");
    setNote({ text: `불러왔어요 · ${p.name}` });
  };

  return (
    <Group title={draft.id ? "팔레트 편집" : "새 팔레트"}>
      <div className="theme-editor">
        <div className="theme-editor-preview">
          <ThemeMini theme={{ ...draft, id: draft.id ?? "draft" }} big />
          <p className="theme-editor-note">{light ? "라이트" : "다크"} · 바탕색의 밝기로 정해져요</p>
        </div>
        <div className="theme-editor-fields">
          <label className="theme-field">
            <span>이름</span>
            <input type="text" value={draft.name} maxLength={40} onChange={(e) => up({ name: e.target.value })} />
          </label>
          <label className="theme-field">
            <span>시작점</span>
            <select
              value=""
              onChange={(e) => {
                const seed = BUILTIN_SEEDS[e.target.value];
                if (seed) up(seed);
              }}
            >
              <option value="">테마에서 색 가져오기…</option>
              {THEMES.map((t) => <option key={t.id} value={t.id}>{t.label}</option>)}
            </select>
          </label>
          <ColorField label="바탕" value={draft.base} onChange={(base) => up({ base })} />
          <ColorField label="강조" value={draft.accent} onChange={(accent) => up({ accent })} />
          <div className="theme-field">
            <span>대비</span>
            <span className="set-range">
              <input
                type="range"
                aria-label="층 대비"
                min={0}
                max={100}
                step={5}
                value={draft.contrast}
                onChange={(e) => up({ contrast: Number(e.target.value) })}
                onDoubleClick={() => up({ contrast: DEFAULT_CONTRAST })}
              />
              <span className="muted small">{draft.contrast}</span>
            </span>
          </div>
        </div>
        <div className="theme-editor-foot">
          <input
            type="text"
            placeholder="팔레트 문자열 붙여 넣기"
            aria-label="팔레트 문자열"
            spellCheck={false}
            value={paste}
            onChange={(e) => setPaste(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && paste.trim() && load()}
          />
          <button disabled={!paste.trim()} onClick={load}>불러오기</button>
          <button
            title="이 팔레트를 한 줄로 복사해요. 다른 컴퓨터에서 붙여 넣으면 같은 팔레트가 돼요."
            onClick={() => {
              copyText(encodePalette(draft));
              setNote({ text: "팔레트 문자열을 복사했어요" });
            }}
          >
            <Copy size={14} />
            복사
          </button>
        </div>
        {note && <p className="theme-editor-note theme-editor-foot" data-err={note.err || undefined}>{note.text}</p>}
        <div className="theme-editor-foot">
          <span className="spacer" />
          <button onClick={onCancel}>취소</button>
          <button className="primary" onClick={() => onSave(draft)}>{draft.id ? "저장" : "만들기"}</button>
        </div>
      </div>
    </Group>
  );
}
