import { readFile, readdir } from "node:fs/promises";
import { resolve, relative } from "node:path";
import { root, readJson, sha256 } from "./storage.mjs";
import { sourceHash } from "../../harness/provenance.mjs";
export async function receipt() {
  const paths = [];
  async function walk(path) {
    for (const entry of await readdir(path, { withFileTypes: true })) {
      const child = resolve(path, entry.name);
      if (entry.isDirectory()) await walk(child);
      else paths.push(child);
    }
  }
  for (const directory of [
    "benchmarks/dist",
    ...["fusor", "fusor-hello", "fusor-todo"].map(
      (name) => `benchmarks/workloads/${name}/dist`,
    ),
  ])
    await walk(resolve(root, directory));
  const cargoReceipt = await readJson(
    resolve(root, "benchmarks/dist/build.json"),
  );
  if (cargoReceipt.schema !== 2)
    throw Error("Rebuild with `just bench-build` to record every native renderer");
  // Every native renderer Cargo reported (fusor and Leptos).
  for (const executable of Object.values(cargoReceipt.nativeSsr))
    paths.push(resolve(root, executable));
  const artifacts = [];
  for (const path of [...new Set(paths)].sort()) {
    const bytes = await readFile(path);
    artifacts.push({
      path: relative(root, path),
      bytes: bytes.length,
      sha256: sha256(bytes),
    });
  }
  return {
    schemaVersion: 1,
    sourceSha256: await sourceHash(),
    cargoReceipt,
    artifacts,
  };
}
