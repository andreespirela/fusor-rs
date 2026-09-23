import assert from "node:assert/strict";
import { test } from "node:test";
import { rm, realpath, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { root, env, exec, startProcess, stopProcess, waitFor, temporaryDirectory, copyProject, independentManifest } from "./build.mjs";

for (const example of ["editor", "integrations"]) {
  test(`copied ${example} consumer resolves all workspace inheritance`, async () => {
    const directory = await temporaryDirectory(`fusor-${example}-manifest-`);
    try {
      await copyProject(join(root, "examples", example), directory);
      const manifest = join(directory, "Cargo.toml");
      await writeFile(manifest, await independentManifest(await readFile(manifest, "utf8")));
      const result = await exec("cargo", ["metadata", "--no-deps", "--format-version", "1", "--offline"],
        { cwd: directory, timeout: 30_000 });
      const metadata = JSON.parse(result.stdout);
      assert.equal(await realpath(metadata.workspace_root), await realpath(directory));
      assert.equal(metadata.packages.length, 1);
      const dependencies = metadata.packages[0].dependencies;
      assert(dependencies.some(dependency => dependency.name === "wasm-bindgen"));
      assert.equal(await realpath(dependencies.find(dependency => dependency.name === "fusor-build").path),
        await realpath(join(root, "crates/fusor-build")));
      assert(dependencies.find(dependency => dependency.name === "web-sys").features.length > 0);
    } finally { await rm(directory, { recursive: true, force: true }); }
  });
}

test("captured commands honor explicit cwd/environment and retain failure diagnostics", async () => {
  const directory = await temporaryDirectory("fusor-process-");
  try {
    const result = await exec(process.execPath, ["-e", "console.log(process.cwd()); console.log(process.env.FUSOR_WASM_BINDGEN)"],
      { cwd: directory, env: { ...env, FUSOR_WASM_BINDGEN: "explicit-test-binary" }, timeout: 10_000 });
    assert.equal(result.stdout, `${await realpath(directory)}\nexplicit-test-binary\n`);
    await assert.rejects(exec(process.execPath, ["-e", "console.error('authored diagnostic'); process.exit(7)"], { timeout: 10_000 }), error => {
      assert.equal(error.code, 7); assert.match(error.stderr, /authored diagnostic/); return true;
    });
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("readiness fails on process exit with its logs and shutdown is idempotent", async () => {
  const owned = startProcess(process.execPath, ["-e", "console.error('startup failed'); process.exit(3)"]);
  try {
    await owned.done;
    await assert.rejects(waitFor(() => false, "preview startup", { timeout: 10_000, process: owned }), /process exited[\s\S]*startup failed/);
  } finally { await stopProcess(owned); await stopProcess(owned); }
});

test("timed-out commands are reaped and missing programs fail promptly", async () => {
  await assert.rejects(exec(process.execPath, ["-e", "require('node:child_process').spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { stdio: 'inherit' }); setInterval(() => {}, 1000)"], { timeout: 100 }), /timed out/);
  await assert.rejects(exec("fusor-no-such-test-command", [], { timeout: 10_000 }), /ENOENT/);
});
