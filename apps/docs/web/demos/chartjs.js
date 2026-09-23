// Ordinary JavaScript: no generated Rust wrapper for Chart.js.
export function onMount({ root, signal, inputs, onCleanup }) {
  // Async setup has its own rollback; the same cleanup also runs on unmount.
  const cleanups = [];
  let disposed = false;
  const own = (cleanup) => {
    if (disposed) {
      try { cleanup(); } catch (error) { console.error(error); }
    } else cleanups.push(cleanup);
  };
  function dispose() {
    if (disposed) return;
    disposed = true;
    for (const cleanup of cleanups.reverse()) {
      try { cleanup(); } catch (error) { console.error(error); }
    }
    cleanups.length = 0;
  }
  onCleanup(dispose);
  const report = (detail) => {
    if (!signal.aborted) root.dispatchEvent(new CustomEvent("chartstatus", { detail }));
  };
  // Only this route downloads the library. onMount itself stays synchronous.
  void import("chart.js/auto").then(({ default: Chart }) => {
    if (signal.aborted) return;
    const canvas = root.querySelector("canvas");
    const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
    const chart = new Chart(canvas, {
      type: "line",
      data: {
        labels: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
        datasets: [{
          label: "Visits",
          data: inputs.values.get(),
          borderColor: "#527dff",
          borderWidth: 3,
          pointRadius: 4,
          pointHoverRadius: 8,
          pointBackgroundColor: "#edf2ff",
          pointBorderWidth: 3,
          fill: true,
          tension: inputs.smooth.get() ? 0.38 : 0,
          backgroundColor(context) {
            const { chartArea, ctx } = context.chart;
            if (!chartArea) return "#527dff18";
            const gradient = ctx.createLinearGradient(0, chartArea.top, 0, chartArea.bottom);
            gradient.addColorStop(0, "#527dff45");
            gradient.addColorStop(1, "#527dff00");
            return gradient;
          },
        }],
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        animation: reducedMotion ? false : { duration: 420 },
        interaction: { mode: "index", intersect: false },
        plugins: {
          legend: { display: false },
          tooltip: { backgroundColor: "#172642", padding: 12, displayColors: false },
        },
        scales: {
          x: { grid: { display: false }, border: { display: false }, ticks: { color: "#8290a8", font: { size: 11 } } },
          y: { beginAtZero: true, grace: "20%", border: { display: false }, grid: { color: "#8f9eb220" }, ticks: { color: "#8290a8", maxTicksLimit: 5, font: { size: 10 }, padding: 10 } },
        },
        onClick(_event, points) {
          if (points.length && !signal.aborted) {
            root.dispatchEvent(new CustomEvent("chartselect", { detail: points[0].index }));
          }
        },
      },
    });
    own(() => chart.destroy());
    own(inputs.values.subscribe((values) => {
      chart.data.datasets[0].data = values;
      chart.update();
    }));
    own(inputs.smooth.subscribe((smooth) => {
      chart.data.datasets[0].tension = smooth ? 0.38 : 0;
      chart.update();
    }));
    own(inputs.selected.subscribe((index) => {
      chart.data.datasets[0].pointRadius = chart.data.labels.map((_, i) => i === index ? 8 : 4);
      chart.update();
    }));
    report("Chart.js ready · data lives in Rust");
  }).catch((error) => {
    dispose();
    report(`Chart unavailable: ${error.message}`);
    if (!signal.aborted) console.error(error);
  });
}
