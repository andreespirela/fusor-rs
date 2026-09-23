import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, rm, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve, dirname } from "node:path";
import {
  create,
  select,
  add,
  attach,
  note,
  publish,
  published,
  load,
  verify,
} from "../lib/history.mjs";
import {
  recordPath,
  within,
  validateReport,
} from "../lib/storage.mjs";
import { compare } from "../lib/compare.mjs";
import { summarize } from "../lib/summary.mjs";
import protocol from "../../schemas/full-protocol.json" with { type: "json" };
import { renderRecord, prepareSite } from "../lib/render.mjs";
const original = JSON.parse((await published()).bytes);
async function fixture(t) {
  const directory = await mkdtemp(resolve(tmpdir(), "rf-benchmark-tools-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  return resolve(directory, "results");
}
async function measurement(directory, name = "input.json") {
  const report = structuredClone(original);
  report.generatedAt = new Date().toISOString();
  const path = resolve(directory, "..", name);
  await writeFile(path, JSON.stringify(report));
  return path;
}
test("record, preselect, publish and render preserve exact raw measurement bytes", async (t) => {
  const directory = await fixture(t);
  await create("example", "Example investigation", directory);
  await note(
    "example",
    {
      kind: "hypothesis",
      title: "Question",
      format: "text",
      body: "A <script> must stay text.",
    },
    directory,
  );
  await select("example", "full-b", "Select before timing", directory);
  const file = await measurement(directory);
  await add("example", "full-b", file, "full", directory);
  await publish("example", directory);
  await publish("example", directory);
  assert.deepEqual((await published(directory)).bytes, await readFile(file));
  const record = await load("example", directory);
  assert.match(renderRecord(record), /&lt;script&gt;/);
  assert.doesNotMatch(renderRecord(record), /<script>/);
  assert.equal((await verify(directory)).checkedFiles, 1);
  await assert.rejects(
    note(
      "example",
      { kind: "observation", title: "late", format: "text", body: "late" },
      directory,
    ),
    /immutable/,
  );
  const site = resolve(directory, "..", "site");
  await prepareSite(directory, site);
  assert.deepEqual(
    await readFile(resolve(site, "results.json")),
    await readFile(file),
  );
  assert.match(
    await readFile(resolve(site, "history.html"), "utf8"),
    /Example investigation/,
  );
});
test("publication rejects screens, altered statistics, duplicate rows and late selection", async (t) => {
  const directory = await fixture(t);
  await create("screen", "Screen", directory);
  await select("screen", "sample", "Before measurement", directory);
  const file = await measurement(directory);
  await add("screen", "sample", file, "screen", directory);
  await assert.rejects(publish("screen", directory), /full report/);
  await assert.rejects(
    select("screen", "another", "Too late", directory),
    /immutable/,
  );
  const bad = structuredClone(original);
  bad.results[0].median += 1;
  assert.throws(() => validateReport(bad), /statistics/);
  const duplicate = structuredClone(original);
  duplicate.results.push(duplicate.results[0]);
  assert.throws(() => validateReport(duplicate), /Duplicate/);
  const partial = structuredClone(original);
  partial.memory = [];
  assert.throws(
    () => validateReport(partial, { full: true }),
    /full six-framework/,
  );
  await create("late", "Late", directory);
  await select(
    "late",
    "full",
    "Before import but after measurement",
    directory,
  );
  const historical = resolve(directory, "..", "old.json");
  await writeFile(historical, JSON.stringify(original));
  await assert.rejects(
    add("late", "full", historical, "full", directory),
    /after publication preselection/,
  );
});
test("failed diagnostic data is retained but cannot be published; checksums detect corruption", async (t) => {
  const directory = await fixture(t);
  await create("failed", "Failed experiment", directory);
  const path = resolve(directory, "..", "failure.json");
  await writeFile(
    path,
    JSON.stringify({ errors: ["failure"], samples: [1, 2] }),
  );
  await attach("failed", "failure", path, "failure", directory);
  await assert.rejects(publish("failed", directory), /preselected/);
  assert.equal((await verify(directory)).checkedFiles, 1);
  const record = await load("failed", directory);
  await writeFile(
    within(dirname(recordPath("failed", directory)), record.evidence[0].path),
    "{}",
  );
  await assert.rejects(verify(directory), /checksum mismatch/);
});
test("unsafe paths and duplicate experiment IDs are rejected", async (t) => {
  const directory = await fixture(t);
  for (const name of ["../escape", "/tmp/escape", "a/b", ""])
    await assert.rejects(create(name, "Invalid", directory), /ID/);
  assert.throws(() => within(directory, "../escape"), /escapes/);
  await create("one", "One", directory);
  await assert.rejects(create("one", "Again", directory), /exists/);
});
test("comparisons flag incompatible environments and avoid division by zero", () => {
  const candidate = structuredClone(original);
  candidate.environment.browser = "different";
  // Give one row a zero median in both reports, which a real run may not contain.
  const baseline = structuredClone(original);
  for (const report of [baseline, candidate]) {
    const row = report.results.find((row) => !row.id.startsWith("ssr-"));
    row.samples = row.samples.map(() => 0);
    Object.assign(row, { median: 0, min: 0, max: 0, p95: 0 });
  }
  const result = compare(baseline, candidate);
  assert(result.environmentDifferences.includes("browser"));
  assert(
    result.rows.some(
      (r) =>
        r.baseline === 0 &&
        r.changePercent === null &&
        r.belowBrowserClockFloor,
    ),
  );
  const changed = structuredClone(original);
  changed.results[0].unit = "bytes";
  assert.throws(() => compare(original, changed), /Unit mismatch/);
});
test("unregistered scripts cannot enter the committed evidence tree", async (t) => {
  const directory = await fixture(t);
  await create("one", "One", directory);
  await writeFile(resolve(directory, "ad-hoc.mjs"), "// no per-run scripts");
  await assert.rejects(verify(directory), /Unregistered file/);
});
test("summaries list every protocol metric and fixture for every framework", () => {
  const text = summarize(original);
  for (const framework of protocol.frameworks) assert.match(text, new RegExp(`\\| ${framework} `));
  for (const metric of protocol.metrics) assert.match(text, new RegExp(`^\\| ${metric.id} \\|`, "m"));
  for (const fixture of protocol.fixtures) assert.match(text, new RegExp(`^\\| ${fixture} \\|`, "m"));
});
