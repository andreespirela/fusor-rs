import { h, render, hydrate } from "preact";
import { memo, flushSync } from "preact/compat";
import { useMemo, useState } from "preact/hooks";
import { install } from "../../common.js";
let control,
  setters = new Map();
const Row = memo(function Row({ id, mode, shared }) {
  const [value, set] = useState(id);
  setters.set(id, set);
  const computed = useMemo(() => value * 2, [value]);
  return (
    <li data-id={id}>
      <span className="value">
        {mode === "computed"
          ? computed
          : mode === "fanout"
            ? value + shared
            : value}
      </span>
      <button onClick={() => set((value) => value + 1)}>+</button>
    </li>
  );
});
export function App({ n, mode }) {
  const [rows, setRows] = useState(() =>
    Array.from({ length: n }, (_, id) => id),
  );
  const [shared, setShared] = useState(0);
  const [inputs, setInputs] = useState(() =>
    mode === "fanin" ? Array.from({ length: n }, (_, id) => id) : [],
  );
  const total = useMemo(() => inputs.reduce((a, b) => a + b, 0), [inputs]);
  control = { rows, setRows, setShared, setInputs };
  return (
    <section>
      <output id="total">{total}</output>
      <ul>
        {mode === "fanin"
          ? null
          : rows.map((id) => (
              <Row
                key={id}
                id={id}
                mode={mode}
                shared={mode === "fanout" ? shared : 0}
              />
            ))}
      </ul>
    </section>
  );
}
const api = {
  mount(n, mode = "rows") {
    flushSync(() =>
      render(<App n={n} mode={mode} />, document.getElementById("app")),
    );
  },
  hydrate(n) {
    flushSync(() =>
      hydrate(<App n={n} mode="rows" />, document.getElementById("app")),
    );
  },
  unmount() {
    render(null, document.getElementById("app"));
    control = null;
    setters.clear();
  },
  update(index, value) {
    flushSync(() => setters.get(control.rows[index])(value));
  },
  bulk(count) {
    flushSync(() => {
      for (let i = 0; i < count; i++)
        setters.get(control.rows[i])((value) => value + 1);
    });
  },
  insert(index, id) {
    flushSync(() => control.setRows((rows) => rows.toSpliced(index, 0, id)));
  },
  remove(index) {
    const id = control.rows[index];
    flushSync(() => control.setRows((rows) => rows.toSpliced(index, 1)));
    setters.delete(id);
  },
  swap(a, b) {
    flushSync(() =>
      control.setRows((rows) => {
        const next = rows.slice();
        [next[a], next[b]] = [next[b], next[a]];
        return next;
      }),
    );
  },
  fanout(value) {
    flushSync(() => control.setShared(value));
  },
  fanin() {
    flushSync(() =>
      control.setInputs((values) => values.map((value) => value + 1)),
    );
  },
  flush() {
    flushSync(() => {});
  },
};
install(api);
