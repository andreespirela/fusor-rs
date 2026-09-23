import { createChart } from "../js/widgets.js";

export function onMount({ root, inputs, onCleanup }) {
  const chart = createChart(root, inputs.count.get(), () => {});
  onCleanup(() => chart.destroy());
  onCleanup(inputs.count.subscribe(value => chart.update(value)));
}
