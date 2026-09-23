import { renderToString, generateHydrationScript } from "solid-js/web";
import { App, configure } from "./main.jsx";
export function render(n) {
  configure(n, "rows");
  return generateHydrationScript() + renderToString(App);
}
