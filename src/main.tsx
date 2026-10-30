import React from "react";
import ReactDOM from "react-dom/client";
import "./lib/monaco";
import "./lib/completion";
import { initShared } from "./lib/shared";
import App from "./App";

async function boot() {
  // Dev only: opening in the browser with ?mock uses a fake backend (for checking the UI)
  if (import.meta.env.DEV && location.search.includes("mock")) await import("./lib/mock");
  try {
    await initShared();
  } catch (e) {
    // Without it the app cannot work: say so in the window instead of leaving it blank
    document.getElementById("root")!.textContent = `Qurry could not start its WebAssembly module (${e}). Reinstall Qurry; if you built it yourself, run \`pnpm wasm\`.`;
    throw e;
  }
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}
boot();
