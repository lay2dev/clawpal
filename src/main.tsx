import React from "react";
import { createRoot } from "react-dom/client";
import "./i18n";
import { App } from "./App";

import "./styles.css";

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
