import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles/globals.css";

// Disable the browser context menu outside inputs/terminal (native-app feel).
window.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement;
  if (!t.closest("input, textarea, .selectable, .xterm")) e.preventDefault();
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
