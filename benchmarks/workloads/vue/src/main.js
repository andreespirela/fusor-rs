import { createApp, createSSRApp, nextTick } from "vue";
import App from "./App.vue";
import { install } from "../../common.js";
let app, view;
const api = {
  hydrate(n) {
    app = createSSRApp(App, { n, mode: "rows" });
    view = app.mount("#app");
  },
  mount(n, mode = "rows") {
    app = createApp(App, { n, mode });
    view = app.mount("#app");
  },
  unmount() {
    app?.unmount();
    app = view = null;
  },
  flush: nextTick,
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
  api[name] = (...args) => view[name](...args);
install(api);
