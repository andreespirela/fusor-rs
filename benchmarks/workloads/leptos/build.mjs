// Builds the three Leptos comparison applications and the native renderer from
// this isolated Cargo workspace. The Wasm pipeline matches fusor's production
// CLI: the release profile, the exact wasm-bindgen `--target web`, name-section
// removal unless FUSOR_KEEP_WASM_NAMES=1, and Binaryen only via FUSOR_WASM_OPT.
import { copyFile, mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { command, root } from "../../../scripts/build.mjs";

const workspace = fileURLToPath(new URL(".", import.meta.url));
const manifest = resolve(workspace, "Cargo.toml");
const output = resolve(root, "benchmarks/dist/leptos");
// Keep in sync with crates/fusor-cli/src/pipeline/wasm.rs.
const WASM_OPT_VERSION = "132";
const WASM_OPT_FLAGS = [
  "-O3",
  "--enable-bulk-memory",
  "--enable-reference-types",
  "--enable-multivalue",
  "--enable-sign-ext",
  "--enable-nontrapping-float-to-int",
];
const fixtures = [
  { name: "benchmark workload", package: "leptos-bench-workload", features: "hydrate", directory: "", html: "web/index.html" },
  { name: "hello", package: "leptos-bench-hello", directory: "hello", html: "web/hello.html" },
  { name: "todo", package: "leptos-bench-todo", directory: "todo", html: "web/todo.html" },
];

export async function lockedVersion(name) {
  const lock = await readFile(resolve(workspace, "Cargo.lock"), "utf8");
  const match = lock.match(
    new RegExp(`\\[\\[package\\]\\]\\nname = "${name}"\\nversion = "([^"]+)"`),
  );
  if (!match) throw Error(`Cargo.lock has no ${name} package`);
  return match[1];
}

// Mirrors fusor's tool cache (crates/fusor-cli/src/toolchain), so
// `cargo fusor install` provisions the same pinned CLI for both frameworks.
function cachedBindgen(version, host) {
  const cache = process.env.FUSOR_CACHE_DIR
    ? process.env.FUSOR_CACHE_DIR
    : process.platform === "win32"
      ? join(process.env.LOCALAPPDATA || "", "Fusor/Cache")
      : process.platform === "darwin"
        ? join(homedir(), "Library/Caches/fusor")
        : join(process.env.XDG_CACHE_HOME || join(homedir(), ".cache"), "fusor");
  const executable = process.platform === "win32" ? "wasm-bindgen.exe" : "wasm-bindgen";
  return join(cache, "wasm-bindgen", version, host, "bin", executable);
}

async function reports(binary, expected) {
  try {
    return (await command(binary, ["--version"], { capture: true })).trim() === expected;
  } catch {
    return false;
  }
}

async function wasmBindgen(version) {
  const expected = `wasm-bindgen ${version}`;
  const host = (await command("rustc", ["-vV"], { capture: true })).match(/^host: (.+)$/m)[1];
  const candidates = process.env.FUSOR_WASM_BINDGEN
    ? [process.env.FUSOR_WASM_BINDGEN]
    : [cachedBindgen(version, host), "wasm-bindgen"];
  for (const candidate of candidates)
    if (await reports(candidate, expected)) return candidate;
  throw Error(
    `${expected} is required by benchmarks/workloads/leptos/Cargo.lock. Run \`cargo fusor install -p fusor-playground\` or set FUSOR_WASM_BINDGEN.`,
  );
}

async function wasmOpt() {
  const binary = process.env.FUSOR_WASM_OPT;
  if (!binary) return null;
  const version = (await command(binary, ["--version"], { capture: true })).trim();
  const expected = `wasm-opt version ${WASM_OPT_VERSION}`;
  if (version !== expected && !version.startsWith(`${expected} `))
    throw Error(`FUSOR_WASM_OPT reports ${JSON.stringify(version)}; select Binaryen ${WASM_OPT_VERSION}`);
  return { binary, version, flags: WASM_OPT_FLAGS };
}

async function cargoArtifact(args, predicate, description) {
  const messages = (
    await command(
      "cargo",
      ["build", "--manifest-path", manifest, "--release", "--locked", "--message-format=json", ...args],
      { capture: true, maxBuffer: 64 * 1024 * 1024 },
    )
  )
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));
  const artifact = messages.find(
    (message) => message.reason === "compiler-artifact" && predicate(message),
  );
  if (!artifact) throw Error(`Cargo did not report ${description}`);
  return artifact;
}

export async function buildLeptos() {
  const keepNames = process.env.FUSOR_KEEP_WASM_NAMES === "1";
  const bindgenVersion = await lockedVersion("wasm-bindgen");
  const bindgen = await wasmBindgen(bindgenVersion);
  const optimizer = await wasmOpt();
  // Generated output only: rebuild it from scratch so no stale file is served.
  await rm(output, { recursive: true, force: true });
  for (const fixture of fixtures) {
    // One Cargo invocation per application, so Cargo cannot unify the
    // `hydrate` and `csr` features into a shared build.
    const artifact = await cargoArtifact(
      [
        "--target", "wasm32-unknown-unknown", "-p", fixture.package, "--lib",
        ...(fixture.features ? ["--features", fixture.features] : []),
      ],
      (message) => message.target.name === fixture.package.replaceAll("-", "_"),
      `the ${fixture.package} Wasm module`,
    );
    const wasm = artifact.filenames.find((file) => file.endsWith(".wasm"));
    if (!wasm) throw Error(`${fixture.package} produced no .wasm file`);
    const directory = resolve(output, fixture.directory);
    const pkg = resolve(directory, "pkg");
    await mkdir(pkg, { recursive: true });
    await command(bindgen, [
      wasm, "--target", "web", "--out-name", "app", "--out-dir", pkg,
      ...(keepNames ? [] : ["--remove-name-section"]),
    ]);
    if (optimizer) {
      const module = resolve(pkg, "app_bg.wasm");
      const optimized = resolve(pkg, "app_bg.optimized.wasm");
      await command(optimizer.binary, [
        module, ...optimizer.flags, ...(keepNames ? ["--debuginfo"] : []), "-o", optimized,
      ]);
      const bytes = await readFile(optimized);
      if (!bytes.subarray(0, 8).equals(Buffer.from("\0asm\x01\0\0\0", "binary")))
        throw Error("wasm-opt did not produce a WebAssembly module");
      await rename(optimized, module);
    }
    await copyFile(resolve(workspace, fixture.html), resolve(directory, "index.html"));
    console.log("Built Leptos", fixture.name);
  }
  // The benchmark page uses the same shared driver as the Vite applications.
  await copyFile(resolve(root, "benchmarks/workloads/common.js"), resolve(output, "common.js"));
  const native = await cargoArtifact(
    ["-p", "leptos-bench-workload", "--features", "ssr", "--bin", "ssr"],
    (message) => message.target.name === "ssr" && message.executable,
    "the Leptos native SSR executable",
  );
  console.log("Built Leptos native SSR renderer");
  return {
    nativeSsr: native.executable,
    toolchain: {
      leptos: await lockedVersion("leptos"),
      wasmBindgen: bindgenVersion,
      wasmOpt: optimizer ? { version: optimizer.version, flags: optimizer.flags } : null,
      wasmNameSection: keepNames ? "kept" : "removed",
      profile: "release: opt-level=3, lto=true, codegen-units=1, panic=abort",
    },
  };
}

// `just bench-build` calls buildLeptos(). Run directly, this rebuilds only the
// Leptos output and updates its entries in an existing build receipt.
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const path = resolve(root, "benchmarks/dist/build.json");
  const receipt = JSON.parse(await readFile(path, "utf8"));
  if (receipt.schema !== 2) throw Error("Run `just bench-build` first");
  const result = await buildLeptos();
  receipt.nativeSsr.leptos = result.nativeSsr;
  receipt.toolchain.leptos = result.toolchain;
  await writeFile(path, JSON.stringify(receipt, null, 2) + "\n");
}
