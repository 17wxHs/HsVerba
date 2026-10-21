// HsVerba — Author: Hs
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Mini from "./Mini";
import Snip from "./Snip";
import Sub from "./Sub";
import "./styles.css";

const params = new URLSearchParams(window.location.search);
const isMini = params.has("mini");
const isSnip = params.has("snip");
const isSub = params.has("sub");
if (isMini) {
  document.documentElement.dataset.window = "mini";
}
if (isSnip) {
  document.documentElement.dataset.window = "snip";
}
if (isSub) {
  document.documentElement.dataset.window = "sub";
}

let view: React.ReactElement;
if (isMini) {
  view = <Mini />;
} else if (isSnip) {
  view = <Snip />;
} else if (isSub) {
  view = <Sub />;
} else {
  view = (
    <React.StrictMode>
      <App />
    </React.StrictMode>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(view);
