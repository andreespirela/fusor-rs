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
import { current, protocols } from "../lib/protocol.mjs";
import { renderRecord, prepareSite } from "../lib/render.mjs";
// The immutable six-framework baseline, measured under six-framework-v1.
const original = JSON.parse(
  await readFile(
    new URL("../../results/history/20260922-baseline/data/full.json", import.meta.url),
    "utf8",
  ),
);
const legacy = protocols.find(({ id }) => id === "six-framework-v1");
// A synthetic report shaped like a new seven-framework run. The added framework's
// rows copy another framework's samples: this is structure, not a measurement.
function sevenFramework() {
  const report = structuredClone(original);
  report.schema = 2;
  report.protocol = current.id;
  report.generatedAt = new Date().toISOString();
  const added = current.frameworks.filter(
    (framework) => !legacy.frameworks.includes(framework),
  );
  for (const framework of added) {
    const copy = (rows) =>
      rows
        .filter((row) => row.framework === "solid")
        .map((row) => ({ ...structuredClone(row), framework }));
    report.results.push(...copy(report.results));
    report.memory.push(...copy(report.memory));
    report.bundles.push(...copy(report.bundles));
    report.environment.frameworkOrder.push(framework);
  }
  return report;
}
async function fixture(t) {
  const directory = await mkdtemp(resolve(tmpdir(), "rf-benchmark-tools-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  return resolve(directory, "results");
}
async function measurement(directory, name = "input.json") {
  const report = sevenFramework();
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
    /complete six-framework-v1 protocol/,
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
  for (const [report, protocol] of [
    [original, legacy],
    [sevenFramework(), current],
  ]) {
    const text = summarize(report);
    for (const framework of protocol.frameworks) assert.match(text, new RegExp(`\\| ${framework} `));
    for (const metric of protocol.metrics) assert.match(text, new RegExp(`^\\| ${metric.id} \\|`, "m"));
    for (const fixture of protocol.fixtures) assert.match(text, new RegExp(`^\\| ${fixture} \\|`, "m"));
    const header = text.split("\n").find((line) => line.startsWith("| Metric |"));
    assert.equal(header.split("|").length - 3, protocol.frameworks.length);
  }
});

test("the recorded six-framework baseline stays valid under its versioned protocol", async () => {
  assert.equal(original.schema, 1);
  assert.equal(original.protocol, undefined);
  assert.deepEqual(
    [...new Set(original.results.map((row) => row.framework))].sort(),
    [...legacy.frameworks].sort(),
  );
  assert.doesNotThrow(() => validateReport(original, { full: true }));
  // The committed history, its checksums and its publication still verify.
  assert((await verify()).checkedFiles >= 1);
  assert(current.frameworks.includes("leptos"));
  assert(legacy.frameworks.every((name) => current.frameworks.includes(name)));
  assert.equal(current.frameworks.length, legacy.frameworks.length + 1);
});

test("new full reports must be complete under the seven-framework protocol", () => {
  const complete = sevenFramework();
  assert.doesNotThrow(() => validateReport(complete, { full: true }));

  // A new run in the legacy format cannot pass as historical.
  const unversioned = structuredClone(original);
  unversioned.generatedAt = new Date().toISOString();
  assert.throws(() => validateReport(unversioned, { full: true }), /closed/);
  const claimed = structuredClone(unversioned);
  claimed.protocol = legacy.id;
  assert.throws(() => validateReport(claimed, { full: true }), /closed/);
  const relabelled = structuredClone(complete);
  relabelled.protocol = legacy.id;
  assert.throws(() => validateReport(relabelled), /requires report schema 1/);

  // Omitting the new framework (rows, memory and bundles) is incomplete.
  const withoutLeptos = structuredClone(complete);
  for (const key of ["results", "memory", "bundles"])
    withoutLeptos[key] = withoutLeptos[key].filter((row) => row.framework !== "leptos");
  withoutLeptos.environment.frameworkOrder = withoutLeptos.environment.frameworkOrder.filter(
    (name) => name !== "leptos",
  );
  assert.throws(
    () => validateReport(withoutLeptos, { full: true }),
    /complete seven-framework-v2 protocol/,
  );
  // It remains a valid smoke/screen report.
  assert.doesNotThrow(() => validateReport(withoutLeptos));

  const noMemory = structuredClone(complete);
  noMemory.memory = noMemory.memory.filter((row) => row.framework !== "leptos");
  noMemory.memory.push(structuredClone(noMemory.memory[0]));
  assert.throws(() => validateReport(noMemory, { full: true }), /Incomplete framework: (fusor|leptos)/);

  const noBundle = structuredClone(complete);
  const bundle = noBundle.bundles.find((row) => row.framework === "leptos" && row.fixture === "todo");
  bundle.fixture = "hello";
  assert.throws(() => validateReport(noBundle, { full: true }), /Missing\/duplicate bundle: leptos/);

  const missingMetric = structuredClone(complete);
  missingMetric.results.find((row) => row.framework === "leptos" && row.id === "hydration-10000").id = "other";
  assert.throws(() => validateReport(missingMetric, { full: true }), /leptos\/hydration-10000/);

  const smoke = structuredClone(complete);
  smoke.profile = "smoke";
  assert.throws(() => validateReport(smoke, { full: true }), /smoke/);

  const order = structuredClone(complete);
  order.environment.frameworkOrder.pop();
  assert.throws(() => validateReport(order, { full: true }), /complete seven-framework-v2/);

  const unknown = structuredClone(complete);
  unknown.results.find((row) => row.framework === "leptos").framework = "unknown";
  assert.throws(() => validateReport(unknown), /Framework outside/);

  const undeclared = structuredClone(complete);
  delete undeclared.protocol;
  assert.throws(() => validateReport(undeclared), /declare its protocol/);
  const future = structuredClone(complete);
  future.protocol = "eight-framework-v3";
  assert.throws(() => validateReport(future), /Unknown benchmark protocol/);
});

test("an incomplete seven-framework run cannot be recorded as the full publication", async (t) => {
  const directory = await fixture(t);
  await create("partial", "Partial", directory);
  await select("partial", "full", "Before measurement", directory);
  const report = sevenFramework();
  for (const key of ["results", "memory", "bundles"])
    report[key] = report[key].filter((row) => row.framework !== "leptos");
  const file = resolve(directory, "..", "partial.json");
  await writeFile(file, JSON.stringify(report));
  await assert.rejects(add("partial", "full", file, "full", directory), /seven-framework-v2/);
  await add("partial", "screen", file, "screen", directory);
  await assert.rejects(publish("partial", directory), /full report/);
});
