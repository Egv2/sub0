import React from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/inter";
import App from "./App";
import { applyTheme, readTheme } from "./theme";
import "./styles/global.css";
import "./styles/titlebar.css";
import { IS_MAC } from "./components/TitleBar";

if (IS_MAC) document.documentElement.dataset.platform = "mac";
applyTheme(readTheme());

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
