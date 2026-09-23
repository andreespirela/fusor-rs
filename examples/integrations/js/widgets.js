import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import Chart from "chart.js/auto";

// Counters make the example’s lifecycle observable in browser tests.
export const counts = { created: 0, destroyed: 0, observers: 0, callbacks: 0 };
export let lateCallback = () => {};
function lifetime(host, onChange) {
  let active = true;
  const observer = new ResizeObserver(() => { if (active) host.dataset.observed = "true"; });
  observer.observe(host); counts.observers++; counts.created++;
  const notify = value => { if (active) { counts.callbacks++; onChange(value); } };
  lateCallback = () => notify("late callback");
  return { notify, dispose() {
    // Exercise callback suppression during cleanup as well as afterwards.
    onChange("during cleanup");
    active = false; observer.disconnect(); counts.observers--; counts.destroyed++;
    host.replaceChildren();
  } };
}
export function createEditor(host, value, onChange) {
  const life = lifetime(host, onChange);
  let view;
  try {
    view = new EditorView({ parent: host, state: EditorState.create({ doc: value, extensions: [
      EditorView.updateListener.of(update => { if (update.docChanged) life.notify(update.state.doc.toString()); })
    ] }) });
  } catch (error) { life.dispose(); throw error; }
  return { update(value) {
    if (value !== view.state.doc.toString()) view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value } });
  }, destroy() { view.destroy(); life.dispose(); } };
}
export function createChart(host, value, onChange) {
  const life = lifetime(host, onChange);
  const canvas = document.createElement("canvas"); canvas.width = 320; canvas.height = 150; host.append(canvas);
  let chart;
  try {
    chart = new Chart(canvas, { type: "bar", data: { labels: ["Count"], datasets: [{ label: "Shared Rust state", data: [Number(value)] }] }, options: { responsive: false, animation: false } });
  } catch (error) { Chart.getChart(canvas)?.destroy(); life.dispose(); throw error; }
  return { update(value) { chart.data.datasets[0].data[0] = Number(value); chart.update("none"); }, destroy() { chart.destroy(); life.dispose(); } };
}

// Test instrumentation only; library use itself is ordinary module imports.
globalThis.FusorWidgets = { counts, lateCallback: () => lateCallback() };
