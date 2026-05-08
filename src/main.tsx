import React from "react";
import ReactDOM from "react-dom/client";
import { SettingsApp } from "./settings/SettingsApp";
import { OnboardingApp } from "./onboarding/OnboardingApp";
import "./styles.css";
import "./onboarding/onboarding.css";

const params = new URLSearchParams(window.location.search);
const view = params.get("view");

function Root() {
  if (view === "onboarding") {
    return <OnboardingApp />;
  }
  if (view === "settings") {
    return <SettingsApp />;
  }
  return (
    <div className="placeholder">
      <h1>Lord Varys</h1>
      <p>Use the tray icon to open Settings or quit.</p>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>
);
