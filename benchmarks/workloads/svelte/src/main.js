import { mount, unmount, hydrate, flushSync } from "svelte";
import App from "./App.svelte";
import { install } from "../../common.js";
let app;
const api = {
  hydrate(n) {
    flushSync(
      () =>
        (app = hydrate(App, {
          target: document.getElementById("app"),
          props: { n, mode: "rows" },
          recover: false,
        })),
    );
  },
  mount(n, mode = "rows") {
    flushSync(
      () =>
        (app = mount(App, {
          target: document.getElementById("app"),
          props: { n, mode },
        })),
    );
  },
  unmount() {
    const old = app;
    app = null;
    return unmount(old);
  },
  flush() {
    flushSync();
  },
};
for (const name of [
  "update",
  "bulk",
  "insert",
  "remove",
  "swap",
  "fanout",
  "fanin",
])
  api[name] = (...args) => flushSync(() => app[name](...args));
install(api);
