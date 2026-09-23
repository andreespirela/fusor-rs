import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { resolve, extname, sep } from "node:path";
import { fileURLToPath } from "node:url";
export const root = resolve(fileURLToPath(new URL("../..", import.meta.url)));
export function asset(pathname) {
  const match = pathname.match(/^\/workloads\/([^/]+)\/(.*)$/);
  if (match && match[1] === "fusor" && /^(hello|todo)\//.test(match[2])) {
    const [kind, ...rest] = match[2].split("/");
    return safe(
      resolve(root, `benchmarks/workloads/fusor-${kind}/dist`),
      rest.join("/") || "index.html",
    );
  }
  if (match) {
    const base =
      match[1] === "fusor"
        ? resolve(root, "benchmarks/workloads/fusor/dist")
        : resolve(root, "benchmarks/dist", match[1]);
    return safe(
      base,
      match[2].endsWith("/")
        ? match[2] + "index.html"
        : match[2] || "index.html",
    );
  }
  if (pathname.startsWith("/docs/")) {
    const relative = pathname.slice(6);
    return safe(
      resolve(root, "dist/docs"),
      extname(relative) ? relative : "index.html",
    );
  }
  if (pathname.startsWith("/benchmarks/")) {
    const relative = pathname.slice(12);
    return safe(
      resolve(root, "dist/benchmarks"),
      extname(relative) ? relative : "index.html",
    );
  }
  return null;
}
function safe(base, relative) {
  const path = resolve(base, relative);
  return path.startsWith(base + sep) ? path : null;
}
export function createSiteServer() {
  return createServer(async (req, res) => {
    const url = new URL(req.url, "http://localhost");
    if (url.pathname === "/favicon.ico") {
      res.writeHead(204);
      res.end();
      return;
    }
    if (url.pathname === "/") {
      res.writeHead(302, { location: "/docs/" });
      res.end();
      return;
    }
    const path = asset(url.pathname);
    if (!path) {
      res.writeHead(404);
      res.end();
      return;
    }
    try {
      const data = await readFile(path);
      res.setHeader(
        "content-type",
        {
          ".html": "text/html",
          ".js": "text/javascript",
          ".css": "text/css",
          ".wasm": "application/wasm",
          ".json": "application/json",
          ".svg": "image/svg+xml",
          ".csv": "text/csv",
          ".txt": "text/plain; charset=utf-8",
        }[extname(path)] || "application/octet-stream",
      );
      res.setHeader(
        "cache-control",
        /\/__fusor\/g-[^/]+\//.test(url.pathname) ||
          /\/assets\/[^/]+-[\w-]+\.(js|css)$/.test(url.pathname)
          ? "public,max-age=31536000,immutable"
          : "no-store",
      );
      res.end(data);
    } catch {
      res.writeHead(404);
      res.end();
    }
  });
}
