import { createHash } from "node:crypto";
import { readdir, readFile } from "node:fs/promises";
import { resolve, relative } from "node:path";
import { root } from "./server.mjs";
// Fingerprint source and lockfiles, not timestamps, generated outputs, or results.
// benchmarks/workloads includes the isolated Leptos workspace: its manifests,
// Cargo.lock, build script, pages and sources.
export async function sourceHash() {
  const paths = [];
  async function visit(path) {
    for (const entry of await readdir(path, { withFileTypes: true })) {
      if (["target", "dist", "node_modules"].includes(entry.name)) continue;
      const child = resolve(path, entry.name);
      if (entry.isDirectory()) await visit(child);
      else paths.push(child);
    }
  }
  for (const name of ["crates", "benchmarks/workloads", "benchmarks/harness"])
    await visit(resolve(root, name));
  for (const name of [
    "Cargo.toml",
    "Cargo.lock",
    "benchmarks/package.json",
    "benchmarks/package-lock.json",
    "benchmarks/build.mjs",
    "benchmarks/build-all.mjs",
    // The harness measures the frameworks this registry defines.
    "benchmarks/schemas/full-protocol.json",
    "benchmarks/tools/lib/protocol.mjs",
  ])
    paths.push(resolve(root, name));
  const hash = createHash("sha256");
  for (const path of paths.sort()) {
    hash.update(relative(root, path));
    hash.update("\0");
    hash.update(await readFile(path));
    hash.update("\0");
  }
  return hash.digest("hex");
}
