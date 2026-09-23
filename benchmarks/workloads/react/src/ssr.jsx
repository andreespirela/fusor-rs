import React from "react";
import { renderToString } from "react-dom/server";
import { App } from "./main.jsx";
export function render(n) {
  return renderToString(<App n={n} mode="rows" />);
}
