// Transactional build, unit isolation, and delivery-cost acceptance fixture.
// Source edits are restricted to these fixture files and always restored.
import assert from "node:assert/strict";
import { readFile, writeFile, readdir, stat, mkdir } from "node:fs/promises";
import { resolve, join } from "node:path";
import { spawn, execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { gzipSync } from "node:zlib";
const root = process.cwd(),
  site = resolve("examples/islands/site"),
  output = join(site, "dist");
const paths = {
  manifest: join(site, "Cargo.toml"),
  source: join(site, "src/main.rs"),
  html: join(site, "web/index.html"),
  cart: resolve("examples/islands/cart-web/src/lib.rs"),
};
const original = Object.fromEntries(
  await Promise.all(
    Object.entries(paths).map(async ([key, path]) => [
      key,
      await readFile(path, "utf8"),
    ]),
  ),
);
const env = {
  ...process.env,
};
async function build(features, fail = false) {
  const args = [
    "run",
    "-p",
    "fusor-cli",
    "--bin",
    "fusor",
    "--locked",
    "--offline",
    "--",
    "build",
    "-p",
    "catalog-site",
    "--offline",
    "--locked",
  ];
  if (features) args.push("--features", features);
  const child = spawn("cargo", args, {
    cwd: root,
    env,
    stdio: ["ignore", "pipe", "pipe"],
  });
  let log = "";
  for (const stream of [child.stdout, child.stderr])
    stream.on("data", (chunk) => (log += chunk));
  const code = await new Promise((resolve, reject) => {
    child.on("error", reject);
    child.on("exit", resolve);
  });
  if (fail) {
    assert.notEqual(code, 0, "expected fixture build to fail");
    return log;
  }
  if (code !== 0) throw Error(log);
  return log;
}
const target = JSON.parse(
  execFileSync(
    "cargo",
    ["metadata", "--no-deps", "--format-version", "1", "--offline"],
    { encoding: "utf8" },
  ),
).target_directory;
const hash = (data) => createHash("sha256").update(data).digest("hex");
async function files(dir, prefix = "") {
  let out = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const relative = join(prefix, entry.name);
    out.push(
      ...(entry.isDirectory()
        ? await files(join(dir, entry.name), relative)
        : [relative]),
    );
  }
  return out;
}
async function snapshot() {
  const html = await readFile(join(output, "index.html"));
  const match = html
    .toString()
    .match(/__fusor\/(g-[^/]+)\/composition.js/);
  assert(match);
  const generation = match[1];
  const manifest = JSON.parse(
    await readFile(join(output, "__fusor", generation, "manifest.json")),
  );
  const units = {};
  for (const [name, unit] of Object.entries(manifest.units)) {
    const assets = [];
    for (const url of [unit.javascript, unit.wasm, ...unit.dependencies]) {
      const bytes = await readFile(join(output, url.slice(1)));
      assets.push({
        path: url.slice(url.indexOf("/" + name + "/") + 1),
        bytes: bytes.length,
        gzip: gzipSync(bytes, { level: 9 }).length,
        sha256: hash(bytes),
      });
    }
    const input = await readFile(
      join(
        target,
        "wasm32-unknown-unknown/release",
        `catalog_${name}_web.wasm`,
      ),
    );
    units[name] = {
      compilerWasmSha256: hash(input),
      assets,
      bytes: assets.reduce((n, a) => n + a.bytes, 0),
      gzip: assets.reduce((n, a) => n + a.gzip, 0),
    };
  }
  const initial = [
    {
      path: "index.html",
      bytes: html.length,
      gzip: gzipSync(html, { level: 9 }).length,
    },
  ];
  for (const name of [
    "boot.js",
    "registry.js",
    "composition.js",
    "manifest.json",
  ]) {
    const bytes = await readFile(
      join(output, "__fusor", generation, name),
    );
    initial.push({
      path: name,
      bytes: bytes.length,
      gzip: gzipSync(bytes, { level: 9 }).length,
    });
  }
  return { generation, htmlHash: hash(html), units, initial };
}
const report = { schema: 1, generatedAt: new Date().toISOString(), checks: [] };
try {
  await build();
  let baseline = (report.separate = await snapshot());
  console.log("PASS baseline: two independently compiled production units");
  const before = new Map(
    await Promise.all(
      (await files(output)).map(async (name) => [
        name,
        hash(await readFile(join(output, name))),
      ]),
    ),
  );
  await writeFile(
    paths.cart,
    original.cart +
      '\ncompile_error!("deliberate delivery acceptance failure");\n',
  );
  await build(null, true);
  await writeFile(paths.cart, original.cart);
  assert.equal((await snapshot()).htmlHash, baseline.htmlHash);
  const after = new Map(
    await Promise.all(
      (await files(output)).map(async (name) => [
        name,
        hash(await readFile(join(output, name))),
      ]),
    ),
  );
  assert.deepEqual(after, before);
  report.checks.push("failed unit preserves every previously published file");
  console.log(
    "PASS failed compilation preserves the entire last good generation",
  );
  await writeFile(
    paths.cart,
    original.cart +
      "\n#[wasm_bindgen::prelude::wasm_bindgen(start)] pub fn application_start() {}\n",
  );
  const startLog = await build(null, true);
  assert.match(startLog, /start/i);
  await writeFile(paths.cart, original.cart);
  assert.equal((await snapshot()).htmlHash, baseline.htmlHash);
  report.checks.push(
    "application start rejected at the native wasm-bindgen boundary",
  );
  console.log("PASS eager application start is rejected");
  await writeFile(
    paths.cart,
    original.cart +
      '\n#[cfg(target_arch = "wasm32")] pub fn wrong_props() { let _ = fusor_islands::browser::Unit::new().entry::<catalog_types::Cart, catalog_views::CartView>(|_, _: catalog_types::DesignerProps| unreachable!()); }\n',
  );
  const propsLog = await build(null, true);
  assert.match(propsLog, /type mismatch|closure arguments/);
  await writeFile(paths.cart, original.cart);
  report.checks.push("mismatched props rejected by rustc");
  await writeFile(
    paths.manifest,
    original.manifest.replace(
      /\[package.metadata.fusor.delivery.units.designer\][\s\S]*$/,
      "",
    ),
  );
  const missingLog = await build(null, true);
  assert.match(missingLog, /registration|unit|descriptor/);
  await writeFile(paths.manifest, original.manifest);
  assert.equal((await snapshot()).htmlHash, baseline.htmlHash);
  report.checks.push("missing delivery unit fails before publication");
  console.log("PASS real rustc props mismatch and missing-unit validation");
  // Establish the comparison after rebuilding the deliberately edited unit.
  await build();
  baseline = report.separate = await snapshot();
  const source = original.source
    .replace(
      "fn registry()",
      "struct ServerOnly { title: String }\nfn registry()",
    )
    .replace(
      "        fs::write(&args[3],",
      `        for number in 0..20 {\n            let page = ServerOnly { title: format!("Server-only article {number}") }.render(&mut Context::new())?;\n            let directory = std::path::Path::new(&args[3]).parent().unwrap();\n            fs::write(directory.join(format!("article-{number}.html")), page.into_string())?;\n        }\n        fs::write(&args[3],`,
    );
  await writeFile(paths.source, source);
  await writeFile(
    paths.html,
    original.html +
      '\n<template rust:component="ServerOnly" rust:render="server"><article><h1>{{ state.title }}</h1><p>Content rendered only by native Rust.</p></article></template>\n',
  );
  await build();
  const expanded = await snapshot();
  for (const name of ["cart", "designer"]) {
    assert.equal(
      expanded.units[name].compilerWasmSha256,
      baseline.units[name].compilerWasmSha256,
      "server-only code cannot enter the compiled browser unit",
    );
    assert.deepEqual(
      expanded.units[name].assets.map(({ path, bytes }) => ({ path, bytes })),
      baseline.units[name].assets.map(({ path, bytes }) => ({ path, bytes })),
    );
  }
  report.afterServerPages = expanded;

  let contentBytes = 0;
  for (let i = 0; i < 20; i++) {
    const page = await readFile(join(output, `article-${i}.html`));
    assert(!page.includes("<script"));
    contentBytes += page.length;
  }
  report.serverOnly = {
    pages: 20,
    htmlBytes: contentBytes,
    compilerWasmUnchanged: true,
    generatedArtifactLengthsUnchanged: true,
  };
  report.checks.push(
    "twenty server-only pages leave compiler Wasm byte-identical and final asset lengths unchanged",
  );
  console.log(
    "PASS 20 server-only pages leave compiler Wasm byte-identical and generated asset lengths unchanged",
  );
  // The immediately preceding page generation remains available to open tabs.
  await stat(
    join(output, "__fusor", baseline.generation, "manifest.json"),
  );
  report.checks.push("previous immutable generation retained");
  await writeFile(paths.source, original.source);
  await writeFile(paths.html, original.html);
  const grouped =
    original.manifest.slice(
      0,
      original.manifest.indexOf(
        "[package.metadata.fusor.delivery.units.cart]",
      ),
    ) +
    '[package.metadata.fusor.delivery.units.grouped]\npackage = "catalog-grouped-web"\n';
  await writeFile(paths.manifest, grouped);
  await build("grouped");
  report.grouped = await snapshot();
  assert.equal(Object.keys(report.grouped.units).length, 1);
  report.checks.push("same view implementations built as one grouped unit");
  console.log(
    "PASS grouped-unit comparison built from the same component libraries",
  );
  // Only one preceding generation is retained; an expired URL is absent, never
  // silently redirected to a newer generation's module.
  await assert.rejects(
    stat(join(output, "__fusor", baseline.generation, "manifest.json")),
  );
  report.checks.push("older generation expires without replacement");
  await mkdir(resolve("test-results"), { recursive: true });
  await writeFile(
    resolve("test-results/selective-delivery-costs.json"),
    JSON.stringify(report, null, 2) + "\n",
  );
} finally {
  for (const [key, path] of Object.entries(paths))
    await writeFile(path, original[key]);
  await build();
}
console.log(
  "Delivery acceptance complete; fixture sources and separate-unit output restored.",
);
