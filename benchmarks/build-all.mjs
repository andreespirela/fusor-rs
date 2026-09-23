import { writeFile, mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { command, buildPackage, root } from "../scripts/build.mjs";
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
await mkdir(resolve(root, "benchmarks/dist"), { recursive: true });
await writeFile(
  resolve(root, "benchmarks/dist/build.json"),
  JSON.stringify({ schema: 1, nativeSsr: executable }, null, 2) + "\n",
);
console.log(
  "Built six production workloads, SSR renderers, and twelve bundle fixtures.",
);
