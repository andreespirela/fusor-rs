import protocol from "../../schemas/full-protocol.json" with { type: "json" };
import { validateReport } from "./storage.mjs";

// Matches the results site: the browser clock cannot resolve shorter operations.
const time = (ms) => (ms < 0.1 ? "<0.10" : ms.toFixed(2));
const kib = (bytes) => (bytes / 1024).toFixed(1);

export function summarize(report) {
  validateReport(report);
  const frameworks = protocol.frameworks.filter((name) =>
    report.results.some((row) => row.framework === name),
  );
  const header = (first) =>
    `| ${first} | ${frameworks.join(" | ")} |\n| --- |${" ---: |".repeat(frameworks.length)}`;
  const env = report.environment;
  const lines = [
    `Recorded ${report.generatedAt} on ${env.cpu}, ${env.os}, browser ${env.browser}, ${env.samples} samples after ${env.warmups} warmups.`,
    "",
    "Median milliseconds:",
    "",
    header("Metric"),
  ];
  for (const metric of protocol.metrics) {
    const cells = frameworks.map((framework) => {
      const row = report.results.find(
        (row) => row.framework === framework && row.id === metric.id,
      );
      return row ? time(row.median) : "—";
    });
    if (cells.some((cell) => cell !== "—"))
      lines.push(`| ${metric.id} | ${cells.join(" | ")} |`);
  }
  if (report.bundles?.length) {
    lines.push("", "Gzip KiB per fixture:", "", header("Fixture"));
    for (const fixture of protocol.fixtures) {
      const cells = frameworks.map((framework) => {
        const bundle = report.bundles.find(
          (bundle) => bundle.framework === framework && bundle.fixture === fixture,
        );
        return bundle
          ? kib(bundle.files.reduce((sum, file) => sum + file.gzip, 0))
          : "—";
      });
      lines.push(`| ${fixture} | ${cells.join(" | ")} |`);
    }
  }
  return lines.join("\n") + "\n";
}
