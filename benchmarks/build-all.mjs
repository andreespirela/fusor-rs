import { writeFile, mkdir, readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { command, buildPackage, root } from "../scripts/build.mjs";
import { buildLeptos } from "./workloads/leptos/build.mjs";
await command(process.execPath, ["benchmarks/build.mjs"]);
for (const name of [
  "fusor-bench-workload",
  "fusor-bench-hello",
  "fusor-bench-todo",
])
  await buildPackage(name);
const cargo = await command(
  "cargo",
  [
    "build",
    "-p",
    "fusor-bench-workload",
    "--bin",
    "ssr",
    "--release",
    "--locked",
    "--message-format=json",
  ],
  { capture: true },
);
const executable = cargo
  .split("\n")
  .filter(Boolean)
  .map((line) => JSON.parse(line))
  .find(
    (message) =>
      message.reason === "compiler-artifact" &&
      message.target.name === "ssr" &&
      message.executable,
  )?.executable;
if (!executable) throw Error("Cargo did not report the native SSR executable");
// Sequential: the Leptos workspace builds after fusor's, never concurrently.
const leptos = await buildLeptos();
const lock = await readFile(resolve(root, "Cargo.lock"), "utf8");
await mkdir(resolve(root, "benchmarks/dist"), { recursive: true });
await writeFile(
  resolve(root, "benchmarks/dist/build.json"),
  JSON.stringify(
    {
      schema: 2,
      nativeSsr: { fusor: executable, leptos: leptos.nativeSsr },
      toolchain: {
        fusor: {
          version: lock.match(
            /\[\[package\]\]\nname = "fusor-core"\nversion = "([^"]+)"/,
          )[1],
          wasmBindgen: lock.match(
            /\[\[package\]\]\nname = "wasm-bindgen"\nversion = "([^"]+)"/,
          )[1],
          // fusor's CLI verifies the Binaryen version and applies the same flags.
          wasmOpt: leptos.toolchain.wasmOpt,
          wasmNameSection: leptos.toolchain.wasmNameSection,
        },
        leptos: leptos.toolchain,
      },
    },
    null,
    2,
  ) + "\n",
);
console.log(
  "Built seven production workloads, SSR renderers, and fourteen bundle fixtures.",
);
