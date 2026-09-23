import { h } from "preact";
import { renderToString } from "preact-render-to-string";
import { App } from "./main.jsx";
export function render(n) {
  return renderToString(<App n={n} mode="rows" />);
}
