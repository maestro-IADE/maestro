import { getVersion } from "@tauri-apps/api/app";
import { check } from "@tauri-apps/plugin-updater";
import { useEffect, useState } from "react";
import { IS_WINDOW } from "../store";
import { offerUpdate } from "../UpdateNotice";
import { SHORTCUTS, type ShortcutId } from "../shortcuts";
import { Group, Row } from "./parts";

// where a shortcut works, when that isn't simply "anywhere outside a terminal" (shortcuts.ts `scope`)
const WHERE: Partial<Record<ShortcutId, string>> = { rename: "사이드바 목록에서" };

/**
 * 정보: the installed version and a manual update check (a found update opens the top bar's popover, UpdateNotice.tsx),
 * and the app's shortcuts, read from the one table (shortcuts.ts).
 */
export function AboutSection() {
  const [version, setVersion] = useState("");
  const [state, setState] = useState<{ kind: "idle" | "checking" | "latest" } | { kind: "found"; version: string } | { kind: "failed"; message: string }>({ kind: "idle" });
  useEffect(() => void getVersion().then(setVersion).catch(() => {}), []);
  const run = () => {
    setState({ kind: "checking" });
    check()
      .then((u) => {
        if (!u) return setState({ kind: "latest" });
        setState({ kind: "found", version: u.version });
        offerUpdate(u);
      })
      .catch((e) => setState({ kind: "failed", message: String(e) }));
  };
  const line =
    state.kind === "checking" ? "확인하는 중…"
    : state.kind === "latest" ? "최신 버전이에요"
    : state.kind === "found" ? `새 버전 v${state.version}이 있어요 · ${IS_WINDOW ? "메인 창 " : ""}위쪽 막대의 업데이트 버튼에서 설치해요`
    : state.kind === "failed" ? `확인하지 못했어요: ${state.message}`
    : "켜진 지 10초 뒤와 그 뒤 6시간마다 저절로 확인해요";
  return (
    <>
      <Group title="버전">
        <Row label={`Maestro ${version ? `v${version}` : ""}`} desc={line}>
          <button className="primary" disabled={state.kind === "checking"} onClick={run}>업데이트 확인</button>
        </Row>
      </Group>
      <Group title="단축키">
        {SHORTCUTS.map((s) => (
          <Row key={s.id} label={s.label} desc={WHERE[s.id]}>
            <kbd className="keycap">{s.keys}</kbd>
          </Row>
        ))}
      </Group>
    </>
  );
}
