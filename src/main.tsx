import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { ReactFlowProvider } from "@xyflow/react";
import App from "./App";
import { WorkspaceWindow } from "./Workspace";
import { loadAgents } from "./NewAgentDialog";
import { IS_WINDOW } from "./store";
// bundled, so the app looks the same offline: Geist for Latin, Geist Mono for figures and code, Pretendard for Korean
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "pretendard/dist/web/variable/pretendardvariable-dynamic-subset.css";
import "./styles.css";
import "./glow";

// the agent catalog first: names, logos and pickers read it synchronously
loadAgents()
  .catch((e) => console.error("agent catalog", e))
  .finally(() =>
    createRoot(document.getElementById("root")!).render(
      <StrictMode>
        {/* `?ws=1`: a pop-out window (from the backend's pool); otherwise the main window */}
        <ReactFlowProvider>{IS_WINDOW ? <WorkspaceWindow /> : <App />}</ReactFlowProvider>
      </StrictMode>,
    ),
  );
