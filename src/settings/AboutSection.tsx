import { getVersion } from "@tauri-apps/api/app";
import { check } from "@tauri-apps/plugin-updater";
import { useEffect, useState } from "react";
import { IS_WINDOW } from "../store";
import { offerUpdate } from "../UpdateNotice";
import { Group, Row } from "./parts";

/** 정보: the installed version and a manual update check. A found update opens the top bar's popover (UpdateNotice.tsx). */
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
    <Group title="버전">
      <Row label={`Maestro ${version ? `v${version}` : ""}`} desc={line}>
        <button className="primary" disabled={state.kind === "checking"} onClick={run}>업데이트 확인</button>
      </Row>
    </Group>
  );
}
