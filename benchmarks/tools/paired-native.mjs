// Supplemental native controls; the renderer binary owns the timing boundary.
import { parseArgs } from "node:util";
import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { sha256, writeJson, exists } from "./lib/storage.mjs";
const { values: a } = parseArgs({
  options: Object.fromEntries(
    ["baseline", "candidate", "output", "pairs", "sizes"].map((k) => [
      k,
      { type: "string" },
    ]),
  ),
});
if (!a.baseline || !a.candidate || !a.output)
  throw Error(
    "Use --baseline BINARY --candidate BINARY --output FILE [--pairs 8] [--sizes 1000,10000]",
  );
const output = resolve(a.output),
  pairs = Number(a.pairs || 8),
  sizes = (a.sizes || "1000,10000").split(",").map(Number);
if (
  !Number.isSafeInteger(pairs) ||
  pairs < 1 ||
  sizes.some((n) => !Number.isSafeInteger(n) || n < 1)
)
  throw Error("Invalid pair count or sizes");
if (await exists(output)) throw Error("Output exists");
const binaries = Object.fromEntries(
  await Promise.all(
    ["baseline", "candidate"].map(async (name) => {
      const path = resolve(a[name]);
      return [name, { path, sha256: sha256(await readFile(path)) }];
    }),
  ),
);
const report = {
  schemaVersion: 1,
  kind: "paired-native",
  binaries,
  pairs: [],
  limitations:
    "Alternating process controls, not the full-protocol publication evaluator. Binary implementations determine their warmups and samples.",
};
try {
  for (const n of sizes)
    for (let pair = 0; pair < pairs; pair++) {
      const entry = {
        n,
        pair,
        order: pair % 2 ? ["candidate", "baseline"] : ["baseline", "candidate"],
      };
      for (const name of entry.order) {
        const result = JSON.parse(
          execFileSync(binaries[name].path, [String(n)], { maxBuffer: 50e6 }),
        );
        if (
          result.n !== n ||
          !result.samples?.length ||
          result.samples.some((x) => !Number.isFinite(x) || x < 0)
        )
          throw Error("Invalid native renderer output");
        entry[name] = result;
      }
      report.pairs.push(entry);
    }
  for (const name of Object.keys(binaries))
    if (sha256(await readFile(binaries[name].path)) !== binaries[name].sha256)
      throw Error("Binary changed during measurement");
} catch (error) {
  report.error = error.message;
  process.exitCode = 1;
}
await writeJson(output, report, { exclusive: true });
console.log(`Preserved ${report.pairs.length} pairs in ${output}`);
