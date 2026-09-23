import { protocolOf, acceptsFull } from "./protocol.mjs";
import { createHash } from "node:crypto";
import { readFile, writeFile, mkdir, rename, access } from "node:fs/promises";
import { resolve, dirname, sep } from "node:path";
import { fileURLToPath } from "node:url";
export const root = fileURLToPath(new URL("../../../", import.meta.url));
export const results = resolve(root, "benchmarks/results");
export const sha256 = (bytes) =>
  createHash("sha256").update(bytes).digest("hex");
export const readJson = async (path) =>
  JSON.parse(await readFile(path, "utf8"));
export const exists = async (path) =>
  access(path).then(
    () => true,
    () => false,
  );
export function id(value) {
  if (!/^[a-z0-9][a-z0-9-]{0,95}$/.test(value || ""))
    throw Error(
      "ID must contain lowercase letters, digits or hyphens (1–96 characters)",
    );
  return value;
}
export function within(base, path) {
  const target = resolve(base, path);
  if (target === resolve(base) || !target.startsWith(resolve(base) + sep))
    throw Error(`Path escapes storage root: ${path}`);
  return target;
}
export const recordPath = (name, directory = results) =>
  resolve(directory, "history", id(name), "record.json");
export async function writeJson(path, value, { exclusive = false } = {}) {
  await mkdir(dirname(path), { recursive: true });
  const text = JSON.stringify(value, null, 2) + "\n";
  if (exclusive) return writeFile(path, text, { flag: "wx" });
  const temporary = `${path}.${process.pid}.tmp`;
  await writeFile(temporary, text, { flag: "wx" });
  await rename(temporary, path);
}
export async function index(directory = results) {
  const path = resolve(directory, "index.json");
  return (await exists(path))
    ? readJson(path)
    : { schemaVersion: 1, published: null, experiments: [] };
}
export function validateRecord(record) {
  if (record.schemaVersion !== 1) throw Error("Unsupported experiment schema");
  id(record.id);
  if (
    !record.title ||
    !["open", "complete", "failed"].includes(record.status)
  )
    throw Error("Invalid experiment metadata");
  for (const key of ["notes", "events", "evidence"])
    if (!Array.isArray(record[key])) throw Error(`Missing ${key}`);
  const paths = new Set();
  for (const entry of record.evidence) {
    if (!entry.path || paths.has(entry.path))
      throw Error(`Duplicate/missing evidence path: ${entry.path}`);
    paths.add(entry.path);
    if (
      !/^[a-f0-9]{64}$/.test(entry.sha256) ||
      !Number.isSafeInteger(entry.bytes) ||
      entry.bytes < 0
    )
      throw Error("Invalid evidence digest/length");
    within("/record", entry.path);
  }
  for (const note of record.notes)
    if (
      typeof note.body !== "string" ||
      !note.title ||
      !["text", "markdown"].includes(note.format)
    )
      throw Error("Invalid note");
  if (
    record.selection &&
    (!record.selection.name ||
      !record.selection.reason ||
      !record.selection.selectedAt)
  )
    throw Error("Incomplete publication selection");
  return record;
}
export function validateReport(report, { full = false } = {}) {
  if (
    ![1, 2].includes(report.schema) ||
    !Array.isArray(report.results) ||
    !report.results.length ||
    !Array.isArray(report.errors)
  )
    throw Error("Invalid measurement report");
  const protocol = protocolOf(report);
  if (!/^[a-f0-9]{64}$/.test(report.sourceSha256 || ""))
    throw Error("Report has no source fingerprint");
  if (report.errors.length)
    throw Error("Measurement report contains evaluator errors");
  const seen = new Set();
  for (const row of report.results) {
    const key = `${row.framework}/${row.id}`;
    if (seen.has(key)) throw Error(`Duplicate measurement: ${key}`);
    seen.add(key);
    if (!protocol.frameworks.includes(row.framework))
      throw Error(`Framework outside ${protocol.id}: ${key}`);
    if (row.status !== "measured") throw Error(`Unmeasured result: ${key}`);
    if (
      !row.samples?.length ||
      row.samples.some((n) => !Number.isFinite(n) || n < 0)
    )
      throw Error(`Invalid samples: ${key}`);
    const ordered = [...row.samples].sort((a, b) => a - b),
      mid = Math.floor(ordered.length / 2);
    // Match the evaluator's upper-middle order statistic for even-sized screens.
    const median = ordered[mid];
    if (
      row.median !== median ||
      row.min !== ordered[0] ||
      row.max !== ordered.at(-1) ||
      row.p95 !== ordered[Math.ceil(ordered.length * 0.95) - 1]
    )
      throw Error(`Incorrect statistics: ${key}`);
  }
  if (full) {
    acceptsFull(protocol, report);
    if (report.profile !== "baseline")
      throw Error("A smoke or subset report cannot be a full report");
    const frameworks = protocol.frameworks;
    const order = report.environment?.frameworkOrder;
    if (
      report.results.length !== frameworks.length * protocol.metrics.length ||
      report.memory?.length !== frameworks.length ||
      report.bundles?.length !== frameworks.length * protocol.fixtures.length ||
      report.environment.samples !== protocol.samples ||
      report.environment.warmups !== protocol.warmups ||
      !Array.isArray(order) ||
      order.length !== frameworks.length ||
      frameworks.some((framework) => !order.includes(framework))
    )
      throw Error(
        `Publication requires the complete ${protocol.id} protocol (${frameworks.join(", ")})`,
      );
    for (const framework of frameworks) {
      if (
        report.results.filter((r) => r.framework === framework).length !==
          protocol.metrics.length ||
        report.memory.filter((r) => r.framework === framework).length !== 1 ||
        report.bundles.filter((r) => r.framework === framework).length !==
          protocol.fixtures.length
      )
        throw Error(`Incomplete framework: ${framework}`);
    }
    for (const framework of frameworks) {
      for (const metric of protocol.metrics) {
        const row = report.results.find(
          (row) => row.framework === framework && row.id === metric.id,
        );
        if (!row || row.unit !== metric.unit)
          throw Error(
            `Missing/invalid protocol metric: ${framework}/${metric.id}`,
          );
      }
      for (const fixture of protocol.fixtures) {
        const bundles = report.bundles.filter(
          (bundle) =>
            bundle.framework === framework && bundle.fixture === fixture,
        );
        if (bundles.length !== 1 || !bundles[0].files?.length)
          throw Error(`Missing/duplicate bundle: ${framework}/${fixture}`);
      }
    }
    if (report.results.some((r) => r.samples.length !== protocol.samples))
      throw Error("Publication requires 15 samples per row");
  }
  return report;
}
