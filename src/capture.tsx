import React from "react";
import ReactDOM from "react-dom/client";
// Before the panel's own styles, which make the page transparent.
import "./styles/global.css";
import { Capture } from "./app/Capture";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Capture />
  </React.StrictMode>,
);
