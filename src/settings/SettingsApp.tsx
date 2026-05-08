import { useState } from "react";
import { GeneralPane } from "./panes/GeneralPane";
import { PermissionsPane } from "./panes/PermissionsPane";
import { AboutPane } from "./panes/AboutPane";

type Tab = "general" | "permissions" | "about";

export function SettingsApp() {
  const [tab, setTab] = useState<Tab>("general");

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="sidebar-title">Lord Varys</div>
        <button
          className={`tab ${tab === "general" ? "active" : ""}`}
          onClick={() => setTab("general")}
        >
          General
        </button>
        <button
          className={`tab ${tab === "permissions" ? "active" : ""}`}
          onClick={() => setTab("permissions")}
        >
          Permissions
        </button>
        <button
          className={`tab ${tab === "about" ? "active" : ""}`}
          onClick={() => setTab("about")}
        >
          About
        </button>
      </aside>
      <main className="pane">
        {tab === "general" && <GeneralPane />}
        {tab === "permissions" && <PermissionsPane />}
        {tab === "about" && <AboutPane />}
      </main>
    </div>
  );
}
