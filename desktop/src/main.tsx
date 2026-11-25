import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { About } from "./pages/About";
import "./index.css";
import "./lib/prefs"; // Reduce motion also applies in child windows

// Child windows load the same index.html with ?window=<label> (no router lib).
const win = new URLSearchParams(location.search).get("window");
const Root = win === "about" ? About : App;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
