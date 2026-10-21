import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Chrome from "./Chrome";

const isChrome = new URLSearchParams(window.location.search).get("view") === "chrome";
document.documentElement.classList.toggle("desktop-chrome-mode", isChrome);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {isChrome ? <Chrome /> : <App />}
  </React.StrictMode>,
);
