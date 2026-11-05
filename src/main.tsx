import React from "react";
import ReactDOM from "react-dom/client";
import "./lib/monaco";
import "./lib/completion";
import { initShared } from "./lib/shared";
import { loadGridAlign } from "./lib/gridAlign";
import { loadCustomFonts } from "./lib/prefs";
import App from "./App";

async function boot() {
  // Dev only: opening in the browser with ?mock uses a fake backend (for checking the UI)
  if (import.meta.env.DEV && location.search.includes("mock")) await import("./lib/mock");
  // Not awaited: the app, and with it the in-app updater, must start even when the WebView refuses WebAssembly
  initShared().catch((e) =>
    console.error(`Qurry could not load its shared WebAssembly module (${e}). If the WebView refused to create a WebAssembly object, the system WebView is too old: update macOS (Safari) or webkit2gtk.`),
  );
  loadGridAlign();
  // Font files added in Settings: until registered, a font set to one renders with the default
  loadCustomFonts().catch((e) => console.warn(`The added font files could not be listed (${e})`));
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}
boot();
