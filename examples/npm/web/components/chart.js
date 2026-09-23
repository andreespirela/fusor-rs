import Chart from 'chart.js/auto';

// Fusor owns the canvas element; Chart.js owns its drawing and listeners.
/** @param {import('../../.fusor/types/web-components-chart-html-ChartPanel').MountContext} context */
export function onMount({ root, inputs, onCleanup }) {
  const chart = new Chart(root.querySelector('canvas'), {
    type: 'bar',
    data: { labels: ['Count', 'Reference'], datasets: [{ label: 'Rust values', data: [inputs.count.get(), 2] }] },
    options: { animation: false, responsive: false },
  });
  onCleanup(() => chart.destroy());
  inputs.count.subscribe(value => {
    chart.data.datasets[0].data[0] = value;
    chart.update('none');
  });
}
