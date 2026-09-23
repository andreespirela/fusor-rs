import { validateReport } from "./storage.mjs";
export function compare(baseline, candidate) {
  validateReport(baseline);
  validateReport(candidate);
  const old = new Map(
    baseline.results.map((row) => [`${row.framework}/${row.id}`, row]),
  );
  const rows = candidate.results.map((row) => {
    const framework = row.framework;
    const key = `${framework}/${row.id}`;
    const before = old.get(key);
    if (!before) return { framework, id: row.id, status: "added" };
    old.delete(key);
    if (row.unit !== before.unit) throw Error(`Unit mismatch: ${row.id}`);
    return {
      framework,
      id: row.id,
      status: "compared",
      unit: row.unit,
      baseline: before.median,
      candidate: row.median,
      changePercent:
        before.median === 0 ? null : (row.median / before.median - 1) * 100,
      belowBrowserClockFloor:
        !row.id.startsWith("ssr-") && Math.min(row.median, before.median) < 0.1,
    };
  });
  for (const row of old.values())
    rows.push({
      framework: row.framework,
      id: row.id,
      status: "removed",
    });
  const environmentDifferences = Object.keys({
    ...baseline.environment,
    ...candidate.environment,
  }).filter(
    (key) =>
      JSON.stringify(baseline.environment[key]) !==
      JSON.stringify(candidate.environment[key]),
  );
  return {
    schemaVersion: 1,
    baselineSource: baseline.sourceSha256,
    candidateSource: candidate.sourceSha256,
    environmentDifferences,
    limitations:
      "Descriptive comparison, not a causal estimate or significance test. Different environments, sample counts and clock-floor results require review.",
    rows,
  };
}
