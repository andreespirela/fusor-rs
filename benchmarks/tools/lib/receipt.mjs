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
  paths.push(resolve(root, cargoReceipt.nativeSsr));
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
