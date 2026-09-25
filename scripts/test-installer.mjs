import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import { createServer } from "node:http";
import { join, resolve } from "node:path";
import { command, root } from "./build.mjs";

const [archiveDirectory, suppliedVersion, installDirectory] = process.argv.slice(2);
if (!archiveDirectory || !suppliedVersion || !installDirectory) {
  throw new Error("Usage: node scripts/test-installer.mjs <archive-directory> <version> <install-directory>");
}
const version = suppliedVersion.replace(/^v/, "");
const directory = resolve(archiveDirectory, `v${version}`);
const artifacts = new Map(await Promise.all((await readdir(directory)).map(async name =>
  [`/v${version}/${name}`, await readFile(join(directory, name))])));

// Python's http.server can stall in getfqdn() before listening on macOS
// runners (actions/runner-images#14409). Bind the numeric loopback address and
// wait for the listening event; no DNS lookup, fixed port or startup sleep.
const server = createServer((request, response) => {
  const artifact = artifacts.get(request.url);
  console.log(`${request.method} ${request.url} ${artifact ? 200 : 404}`);
  response.writeHead(artifact ? 200 : 404, {
    "Content-Type": "application/octet-stream",
  });
  response.end(artifact ?? "Not found");
});

try {
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const downloadBase = `http://127.0.0.1:${server.address().port}`;
  console.log(`Installer test server listening at ${downloadBase}`);
  const environment = {
    ...process.env,
    FUSOR_DOWNLOAD_BASE: downloadBase,
    FUSOR_INSTALL: resolve(installDirectory),
    FUSOR_VERSION: `v${version}`,
  };
  const windows = process.platform === "win32";
  await command(windows ? "pwsh" : "sh", windows
    ? ["-NoLogo", "-NoProfile", "-File", join(root, "install.ps1")]
    : [join(root, "install.sh"), `v${version}`],
  { env: environment, timeout: 120_000 });

  const binary = join(environment.FUSOR_INSTALL, "bin", `fusor${windows ? ".exe" : ""}`);
  const output = await command(binary, ["--version"], { timeout: 10_000 });
  assert.equal(output.trim(), `fusor ${version}`, "fusor installed the expected version");
} finally {
  await new Promise(resolve => {
    server.close(resolve);
    server.closeAllConnections();
  });
}
