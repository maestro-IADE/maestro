import { useEffect, useState, type KeyboardEvent } from "react";
import { agentLabel, permissionOf } from "../agents";
import { samePath } from "../attention";
import { ipc } from "../ipc";
import { MergeModeRow } from "../MergeModeRow";
import { removeFromList } from "../Sidebar";
import { useStore, type SettingsTarget } from "../store";
import type { ProjectPrep, Repo } from "../types";
import { Group, Row } from "./parts";
import { copyFilesChanged, copyFilesText, parseCopyFiles } from "./prepText";

/**
 * 프로젝트: one card per project in the list, in the sidebar's order: its merge mode (R6), 워크트리 준비, its Maestro (a
 * summary and Lead 설정…, or Lead 만들기) and 목록에서 제거…. `target.root` marks that project's card for the deep link.
 */
export function ProjectsSection({ target }: { target: SettingsTarget }) {
  const repos = useStore((s) => s.repos);
  if (!repos.length) return <p className="muted small">프로젝트가 없어요. 사이드바의 프로젝트 추가로 폴더를 더해요</p>;
  return (
    <>
      {repos.map((r) => <ProjectCard key={r.root} repo={r} target={!!target.root && samePath(target.root, r.root)} />)}
      <p className="set-note">
        PR 모드는 GitHub 원격과 로그인된 gh(2.40 이상)가 있어야 써요. 앱은 로그인 정보를 보관하지 않고, 강제 푸시나 원격 브랜치 삭제는 하지 않아요. squash는 PR 모드에서만 골라요
      </p>
    </>
  );
}

/**
 * One project's card. The dialogs it opens (agent settings, the new agent dialog) are drawn under the settings
 * dialog (Workspace.tsx `WindowLayers`), so settings closes first and the dialog takes over.
 */
function ProjectCard({ repo, target }: { repo: Repo; target: boolean }) {
  const maestro = useStore((s) => s.worktrees.find((w) => w.isMaestro && w.root === repo.root));
  const s = useStore.getState;
  const summary = maestro
    ? [
        agentLabel(maestro.adapterId),
        maestro.permissionMode ? permissionOf(maestro.permissionMode).label : "",
        maestro.model ?? "",
        `기준 ${maestro.baseBranch ?? repo.defaultBranch}`,
      ].filter(Boolean).join(" · ")
    : "프로젝트 폴더에서 일하며 워커에게 일을 나눠 맡기는 에이전트예요";
  return (
    <section className="set-group set-project" data-target={target || undefined}>
      <div className="set-card">
        <div className="set-project-head">
          <b>{repo.name}</b>
          <code title={repo.root}>{repo.root}</code>
        </div>
        <MergeModeRow repo={repo} label="병합 방식" />
        <PrepRows root={repo.root} />
        <Row label="Lead" desc={summary}>
          {maestro ? (
            <button
              onClick={() => {
                s().closeSettings();
                s().openAgentSettings({ path: maestro.path });
              }}
            >
              Lead 설정…
            </button>
          ) : (
            <button
              onClick={() => {
                s().closeSettings();
                s().openNewAgent({ root: repo.root, mode: "maestro" });
              }}
            >
              Lead 만들기
            </button>
          )}
        </Row>
        <Row label="목록에서 제거" desc="디스크의 파일은 그대로예요">
          <button className="set-danger" onClick={() => void removeFromList(repo)}>목록에서 제거…</button>
        </Row>
      </div>
    </section>
  );
}

/**
 * 워크트리 준비 (prep.rs): what each new worktree of the project gets, the local files copied in ("복사할 파일",
 * comma separated) and the command run there ("준비 명령"). Each field saves on blur or Enter; the backend answers
 * the cleaned value, or why it refused (shown under the rows, the field keeps what was typed).
 */
function PrepRows({ root }: { root: string }) {
  const [saved, setSaved] = useState<ProjectPrep | null>(null);
  const [files, setFiles] = useState("");
  const [cmd, setCmd] = useState("");
  const [err, setErr] = useState("");

  const show = (p: ProjectPrep) => {
    setSaved(p);
    setFiles(copyFilesText(p.copyFiles));
    setCmd(p.setupCommand);
  };

  useEffect(() => {
    let live = true;
    ipc.projectPrep(root).then((p) => live && show(p)).catch((e) => live && setErr(String(e)));
    return () => {
      live = false;
    };
  }, [root]);

  const save = (change: { copyFiles?: string[]; setupCommand?: string }) =>
    ipc.setProjectPrep(root, change).then((p) => { show(p); setErr(""); }, (e) => setErr(String(e)));
  const enter = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" && !e.nativeEvent.isComposing) e.currentTarget.blur();
  };

  return (
    <div className="set-prep">
      <div className="set-prep-title">워크트리 준비</div>
      <Row label="복사할 파일">
        <input
          type="text"
          aria-label="복사할 파일"
          spellCheck={false}
          placeholder=".env*"
          disabled={!saved}
          value={files}
          onChange={(e) => setFiles(e.target.value)}
          onKeyDown={enter}
          onBlur={() => saved && copyFilesChanged(files, saved.copyFiles) && void save({ copyFiles: parseCopyFiles(files) })}
        />
      </Row>
      <Row label="준비 명령">
        <input
          type="text"
          aria-label="준비 명령"
          spellCheck={false}
          placeholder="npm install"
          disabled={!saved}
          value={cmd}
          onChange={(e) => setCmd(e.target.value)}
          onKeyDown={enter}
          onBlur={() => saved && cmd.trim() !== saved.setupCommand && void save({ setupCommand: cmd })}
        />
      </Row>
      {err && <div className="err small wrap">{err}</div>}
    </div>
  );
}
