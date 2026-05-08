import { useState } from "react";
import { GeneralPane } from "./panes/GeneralPane";
import { ModelsPane } from "./panes/ModelsPane";
import { PermissionsPane } from "./panes/PermissionsPane";
import { AboutPane } from "./panes/AboutPane";
import { VarysLogo } from "../components/Logo";

type Section = "general" | "models" | "permissions" | "about";

const NAV: { id: Section; label: string; icon: string }[] = [
  { id: "general", label: "General", icon: "◐" },
  { id: "models", label: "Models", icon: "▣" },
  { id: "permissions", label: "Permissions", icon: "✦" },
  { id: "about", label: "About", icon: "★" },
];

export function SettingsApp() {
  const [section, setSection] = useState<Section>("general");

  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="sidebar-brand">
          <VarysLogo className="sidebar-logo" />
          <div>
            <div className="sidebar-title">Lord Varys</div>
            <div className="sidebar-tagline">Ambient memory</div>
          </div>
        </div>
        <nav>
          {NAV.map((n) => (
            <button
              key={n.id}
              type="button"
              className={`nav-btn ${section === n.id ? "active" : ""}`}
              onClick={() => setSection(n.id)}
            >
              <span className="nav-icon">{n.icon}</span>
              <span>{n.label}</span>
            </button>
          ))}
        </nav>
        <div className="sidebar-foot subtle">
          Click the tray icon to record.
        </div>
      </aside>
      <main className="pane">
        {section === "general" && <GeneralPane />}
        {section === "models" && <ModelsPane />}
        {section === "permissions" && <PermissionsPane />}
        {section === "about" && <AboutPane />}
      </main>
    </div>
  );
}
