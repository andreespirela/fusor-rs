import { render as renderSvelte } from "svelte/server";
import App from "./App.svelte";
export function render(n) {
  return renderSvelte(App, { props: { n, mode: "rows" } }).body;
}
