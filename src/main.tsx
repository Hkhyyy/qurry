import React from "react";
import ReactDOM from "react-dom/client";
import "./lib/monaco";
import "./lib/completion";
import App from "./App";

async function boot() {
  // Dev only: opening in the browser with ?mock uses a fake backend (for checking the UI)
  if (import.meta.env.DEV && location.search.includes("mock")) await import("./lib/mock");
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}
boot();
