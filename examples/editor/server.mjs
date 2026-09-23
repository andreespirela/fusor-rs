// Run after building the example. Local demo only; no database or authentication.
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { projectBackend } from "./backend.mjs";
const root = fileURLToPath(new URL("../../", import.meta.url));
const previewPort = process.env.EDITOR_PREVIEW_PORT || "4181";
const port = Number(process.env.PORT || 4180);
const executable = join(root, "target/debug", `fusor${process.platform === "win32" ? ".exe" : ""}`);
const preview = spawn(executable, ["preview", "examples/editor/dist", "--port", previewPort, "--locked", "--offline"], { cwd:root, stdio:["ignore","pipe","inherit"] });
const backend = projectBackend();
const server = createServer(async (req, res) => {
  if (await backend.handle(req, res)) return;
  try {
    const response = await fetch(new URL(req.url, `http://127.0.0.1:${previewPort}`), { headers:{ accept:req.headers.accept || "*/*" } });
    res.writeHead(response.status, Object.fromEntries(response.headers)); res.end(Buffer.from(await response.arrayBuffer()));
  } catch { res.writeHead(502); res.end("Preview unavailable"); }
});
let started = false;
preview.stdout.on("data", data => {
  if (!started && data.toString().includes("Ctrl+C to stop.")) {
    started = true; server.listen(port, "127.0.0.1", () => console.log(`Editor: http://127.0.0.1:${port}/editor/project`));
  }
});
function close() { backend.close(); server.closeAllConnections(); server.close(); preview.kill(); }
preview.on("error", error => { console.error(error.message); close(); process.exitCode = 1; });
preview.on("exit", code => { close(); if (code) process.exitCode = code; });
process.on("SIGINT", close); process.on("SIGTERM", close);
