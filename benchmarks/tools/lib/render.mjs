import { mkdir, copyFile, writeFile, rm } from "node:fs/promises";
import { resolve, dirname } from "node:path";
import { root, results, index, recordPath, within } from "./storage.mjs";
import { load, published } from "./history.mjs";
const escape = (value) =>
  String(value).replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ],
  );
function page(title, body) {
  return `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${escape(title)} — fusor benchmarks</title><style>
body{margin:0;background:#f7f9fc;color:#182232;font:16px/1.6 system-ui,sans-serif}main{max-width:1100px;margin:48px auto;padding:0 24px}a{color:#075cbb}nav{display:flex;gap:24px;margin-bottom:36px}h1{font-size:36px;line-height:1.2}article,details{border:1px solid #dbe2eb;background:white;border-radius:10px;padding:20px;margin:16px 0}summary{cursor:pointer;font-weight:650;overflow-wrap:anywhere}pre{white-space:pre-wrap;overflow-wrap:anywhere;font:14px/1.65 ui-monospace,monospace}small{color:#526173}.table{overflow:auto}table{border-collapse:collapse;width:100%;font-size:14px}th,td{text-align:left;padding:10px;border-bottom:1px solid #dbe2eb}code{overflow-wrap:anywhere}ul{padding-left:24px}</style><main><nav><a href="/benchmarks/">Live results</a><a href="/benchmarks/history.html">Research history</a><a href="/docs/">Documentation</a></nav><h1>${escape(title)}</h1>${body}</main></html>`;
}
export function renderRecord(record) {
  const notes = record.notes;
  const measurements = record.evidence.filter((e) => e.kind === "measurement");
  return page(
    record.title,
    `<p><code>${escape(record.id)}</code> · ${escape(record.status)}</p><p><a href="/benchmarks/history/${record.id}/record.json" download>Download complete JSON record</a></p>
<h2>Measurements</h2><ul>${measurements.map((e) => `<li><a href="/benchmarks/history/${record.id}/${escape(e.path)}">${escape(e.name)}</a></li>`).join("")}</ul>
<h2>Research and decisions</h2>${notes.map((note, i) => `<details ${i === 0 ? "open" : ""}><summary>${escape(note.title)}</summary><small>${escape(note.kind || note.format)}${note.recordedAt ? " · " + escape(note.recordedAt) : ""}</small><pre>${escape(note.body)}</pre></details>`).join("")}`,
  );
}
export async function prepareSite(
  directory = results,
  output = resolve(root, "apps/benchmarks/public"),
) {
  const catalog = await index(directory),
    selected = await published(directory);
  await mkdir(output, { recursive: true });
  await writeFile(resolve(output, "results.json"), selected.bytes);
  await copyFile(
    resolve(root, "benchmarks/METHODOLOGY.md"),
    resolve(output, "methodology.md"),
  );
  await writeFile(
    resolve(output, "report.html"),
    renderRecord(selected.record),
  );
  // This directory contains only this generator's disposable output.
  const history = resolve(output, "history");
  await rm(history, { recursive: true, force: true });
  await mkdir(history, { recursive: true });
  await writeFile(
    resolve(output, "history.json"),
    JSON.stringify(catalog, null, 2) + "\n",
  );
  const cards = [];
  for (const entry of [...catalog.experiments].reverse()) {
    const record = await load(entry.id, directory);
    cards.push(
      `<article><h2><a href="/benchmarks/history/${entry.id}.html">${escape(record.title)}</a></h2><small>${escape(record.id)} · ${escape(record.status)}</small><p>${record.evidence.filter((e) => e.kind === "measurement").length} measurement reports · ${record.notes.length} notes</p></article>`,
    );
    await writeFile(
      resolve(history, `${record.id}.html`),
      renderRecord(record),
    );
    await mkdir(resolve(history, record.id), { recursive: true });
    await copyFile(
      recordPath(record.id, directory),
      resolve(history, record.id, "record.json"),
    );
    for (const evidence of record.evidence) {
      const target = within(resolve(history, record.id), evidence.path);
      await mkdir(dirname(target), { recursive: true });
      await copyFile(
        within(dirname(recordPath(record.id, directory)), evidence.path),
        target,
      );
    }
  }
  await writeFile(
    resolve(output, "history.html"),
    page(
      "Benchmark research history",
      `<p>Every recorded run, with its raw measurements and notes.</p>${cards.join("")}`,
    ),
  );
}
