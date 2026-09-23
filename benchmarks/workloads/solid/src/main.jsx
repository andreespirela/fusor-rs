import { createSignal, createMemo, For, batch } from "solid-js";
import { render, hydrate } from "solid-js/web";
import { install } from "../../common.js";
let dispose, rows, setRows, shared, setShared, inputs, mode;
function item(id) {
  const [value, set] = createSignal(id);
  return { id, value, set };
}
function Row(props) {
  const computed = createMemo(() => props.item.value() * 2);
  return (
    <li data-id={props.item.id}>
      <span class="value">
        {mode === "computed"
          ? computed()
          : mode === "fanout"
            ? props.item.value() + shared()
            : props.item.value()}
      </span>
      <button onClick={() => props.item.set((value) => value + 1)}>+</button>
    </li>
  );
}
export function App() {
  const total = createMemo(() =>
    mode === "fanin" ? inputs.reduce((sum, item) => sum + item.value(), 0) : 0,
  );
  return (
    <section>
      <output id="total">{total()}</output>
      <ul>
        <For each={mode === "fanin" ? [] : rows()}>
          {(item) => <Row item={item} />}
        </For>
      </ul>
    </section>
  );
}
export function configure(n, kind) {
  mode = kind;
  [rows, setRows] = createSignal(
    Array.from({ length: n }, (_, id) => item(id)),
  );
  inputs = mode === "fanin" ? rows() : [];
  [shared, setShared] = createSignal(0);
}
install({
  hydrate(n) {
    configure(n, "rows");
    dispose = hydrate(App, document.getElementById("app"));
  },
  mount(n, kind = "rows") {
    configure(n, kind);
    dispose = render(App, document.getElementById("app"));
  },
  unmount() {
    dispose?.();
    dispose = null;
    rows = setRows = shared = setShared = null;
    inputs = [];
  },
  update(index, value) {
    rows()[index].set(value);
  },
  bulk(count) {
    batch(() => {
      for (let i = 0; i < count; i++) rows()[i].set((value) => value + 1);
    });
  },
  insert(index, id) {
    setRows((rows) => rows.toSpliced(index, 0, item(id)));
  },
  remove(index) {
    setRows((rows) => rows.toSpliced(index, 1));
  },
  swap(a, b) {
    setRows((rows) => {
      const next = rows.slice();
      [next[a], next[b]] = [next[b], next[a]];
      return next;
    });
  },
  fanout(value) {
    setShared(value);
  },
  fanin() {
    batch(() => inputs.forEach((item) => item.set((value) => value + 1)));
  },
  flush() {},
});
