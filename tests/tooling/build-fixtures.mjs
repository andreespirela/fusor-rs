import { buildPackage, command } from "../../scripts/build.mjs";
const groups = {
  async: ["fusor-async-components"],
  children: ["fusor-children"],
  "benchmark-runtime": ["fusor-bench-workload"],
  coherent: ["fusor-coherent"],
  islands: ["catalog-site"],
};
for (const group of process.argv.slice(2)) {
  // The landing page, docs and benchmarks are tested as the assembled site.
  if (group === "site") {
    await command("cargo", ["run", "-p", "fusor-cli", "--bin", "fusor", "--locked", "--", "build", "--site", "--locked"]);
    continue;
  }
  if (!groups[group]) throw Error(`Unknown fixture ${group}`);
  for (const name of groups[group]) await buildPackage(name);
}
