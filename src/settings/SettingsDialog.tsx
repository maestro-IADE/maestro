import { useEffect, useRef, useState, type ReactNode } from "react";
import { isImeComposing } from "../ime";
import { useStore, type SettingsTarget } from "../store";
import { Bot, FolderGit2, Info, Palette, Settings2, X } from "../icons";
import { sectionOf, type SectionId } from "./agentState";
import { GeneralSection } from "./GeneralSection";
import { AgentsSection } from "./AgentsSection";
import { ProjectsSection } from "./ProjectsSection";
import { LookSection } from "./LookSection";
import { AboutSection } from "./AboutSection";

const SECTIONS: Record<SectionId, { label: string; icon: ReactNode; desc: string }> = {
  general: { label: "일반", icon: <Settings2 />, desc: "에이전트를 보는 방식과 처음 여는 화면, 사이드바, 자리 비움, 자동 저장을 정해요" },
  agents: { label: "에이전트", icon: <Bot />, desc: "쓸 에이전트를 켜고 끄고, 새 에이전트의 기본값과 설치·로그인을 관리해요" },
  projects: { label: "프로젝트", icon: <FolderGit2 />, desc: "프로젝트마다 병합 방식과 마에스트로를 관리해요" },
  look: { label: "모양", icon: <Palette />, desc: "테마와 화면 움직임의 느낌을 정해요" },
  about: { label: "정보", icon: <Info />, desc: "설치된 앱의 버전과 새 버전, 단축키를 봐요" },
};

/** how long a deep link's row stays outlined */
const FLASH_MS = 1500;

/**
 * Settings popup: five sections on the left (일반 / 에이전트 / 프로젝트 / 모양 / 정보), the chosen one on the right.
 * Opened from anywhere through the store (`openSettings`, the top bar, Ctrl+,). `target` is a deep link: its section,
 * and an agent's row (opened at 관리) or a project's card, scrolled to and outlined for 1.5s. A new link while it's
 * open goes there too. Esc closes it from anywhere in the window, except inside a setup terminal.
 */
export function SettingsDialog({ target, onClose }: { target: SettingsTarget; onClose: () => void }) {
  const [sec, setSec] = useState<SectionId>(sectionOf(target.section));
  const body = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  close.current = onClose;
  // the link already shown: picking another section and coming back doesn't flash it again
  const shown = useRef<SettingsTarget | null>(null);

  // re-check on open: a CLI may have been installed from a terminal meanwhile
  useEffect(() => void useStore.getState().loadCli().catch(() => {}), []);

  // another link while open: its section
  useEffect(() => setSec(sectionOf(target.section)), [target]);

  // a new section starts at the top
  useEffect(() => void body.current?.scrollTo({ top: 0 }), [sec]);

  // the link's row: once its section is on screen (an opened agent row has grown by then: two frames)
  useEffect(() => {
    if (shown.current === target || sec !== sectionOf(target.section) || (!target.agent && !target.root)) return;
    shown.current = target;
    let timer = 0;
    let f2 = 0;
    let el: HTMLElement | null = null;
    const f1 = requestAnimationFrame(() => {
      f2 = requestAnimationFrame(() => {
        const box = body.current;
        el = box?.querySelector<HTMLElement>("[data-target]") ?? null;
        if (!box || !el) return;
        // taller than the view (an agent row opened with its terminal): its top, not its middle
        el.scrollIntoView({ block: el.offsetHeight > box.clientHeight ? "start" : "center" });
        el.dataset.flash = "";
        timer = window.setTimeout(() => el && delete el.dataset.flash, FLASH_MS);
      });
    });
    return () => {
      cancelAnimationFrame(f1);
      cancelAnimationFrame(f2);
      clearTimeout(timer);
      if (el) delete el.dataset.flash;
    };
  }, [target, sec]);

  // Esc anywhere in the window (not only with the focus inside); a setup terminal keeps its Esc. A confirmation or
  // a menu over it takes Esc first (they listen while capturing and stop it)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented || isImeComposing(e)) return;
      if (e.target instanceof Element && e.target.closest(".xterm")) return;
      e.preventDefault();
      close.current();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const cur = SECTIONS[sec];
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal settings" role="dialog" aria-labelledby="settings-title">
        {/* a fixed top bar across the whole dialog: the title, and close on the right; the nav and the page scroll under it */}
        <header className="set-top">
          <h2 id="settings-title">설정</h2>
          <button className="icon set-close" title="닫기 (Esc)" aria-label="닫기" autoFocus onClick={onClose}><X /></button>
        </header>
        <nav className="set-nav">
          {(Object.keys(SECTIONS) as SectionId[]).map((id) => (
            <button key={id} data-on={id === sec || undefined} aria-current={id === sec || undefined} onClick={() => setSec(id)}>
              {SECTIONS[id].icon}
              {SECTIONS[id].label}
            </button>
          ))}
        </nav>

        <div className="set-body" ref={body}>
          <div className="set-content">
            <header className="set-head">
              <h3>{cur.label}</h3>
              <p>{cur.desc}</p>
            </header>
            {sec === "general" ? <GeneralSection />
              : sec === "agents" ? <AgentsSection target={target} />
              : sec === "projects" ? <ProjectsSection target={target} />
              : sec === "look" ? <LookSection />
              : <AboutSection />}
          </div>
        </div>
      </div>
    </div>
  );
}
