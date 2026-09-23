import { readFile, writeFile, mkdir, readdir } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import {
  results,
  id,
  index,
  recordPath,
  within,
  sha256,
  readJson,
  writeJson,
  validateRecord,
  validateReport,
  exists,
} from "./storage.mjs";
export async function load(name, directory = results) {
  const record = validateRecord(await readJson(recordPath(name, directory)));
  if (record.id !== name) throw Error("Record ID does not match its directory");
  return record;
}
export async function create(name, title, directory = results) {
  id(name);
  const catalog = await index(directory);
  if (
    catalog.experiments.some((e) => e.id === name) ||
    (await exists(recordPath(name, directory)))
  )
    throw Error(`Experiment exists: ${name}`);
  const record = {
    schemaVersion: 1,
    id: name,
    title,
    createdAt: new Date().toISOString(),
    status: "open",
    selection: null,
    notes: [],
    events: [],
    evidence: [],
  };
  validateRecord(record);
  await writeJson(recordPath(name, directory), record, { exclusive: true });
  catalog.experiments.push({
    id: name,
    title,
    path: `history/${name}/record.json`,
  });
  await writeJson(resolve(directory, "index.json"), catalog);
  return record;
}
export async function select(name, reportName, reason, directory = results) {
  const record = await load(name, directory);
  id(reportName);
  if (
    record.status !== "open" ||
    record.selection ||
    record.evidence.some((e) => e.name === reportName)
  )
    throw Error(
      "Select a publication candidate before recording that report; selection is immutable",
    );
  if (!reason?.trim()) throw Error("A selection reason is required");
  record.selection = {
    name: reportName,
    reason,
    selectedAt: new Date().toISOString(),
  };
  await writeJson(recordPath(name, directory), record);
}
export async function note(name, note, directory = results) {
  const record = await load(name, directory);
  if (record.status !== "open")
    throw Error(
      "Archived/completed records are immutable; create a follow-up experiment",
    );
  if (
    !["hypothesis", "decision", "observation", "research"].includes(note.kind)
  )
    throw Error(
      "Note kind must be hypothesis, decision, observation or research",
    );
  record.notes.push({
    kind: note.kind,
    title: note.title,
    body: note.body,
    format: note.format,
    recordedAt: new Date().toISOString(),
  });
  validateRecord(record);
  await writeJson(recordPath(name, directory), record);
}
export async function add(
  name,
  reportName,
  input,
  kind = "screen",
  directory = results,
) {
  id(reportName);
  if (!["screen", "full"].includes(kind))
    throw Error("Report kind must be screen or full");
  const record = await load(name, directory);
  if (record.status !== "open")
    throw Error("Only open experiments accept measurements");
  if (record.evidence.some((e) => e.name === reportName))
    throw Error("Report name already exists");
  const bytes = await readFile(input),
    report = validateReport(JSON.parse(bytes), { full: kind === "full" });
  if (
    record.selection?.name === reportName &&
    !(Date.parse(report.generatedAt) >= Date.parse(record.selection.selectedAt))
  )
    throw Error(
      "Selected report must be measured after publication preselection",
    );
  const path = `data/${reportName}.json`,
    target = within(dirname(recordPath(name, directory)), path);
  await mkdir(dirname(target), { recursive: true });
  await writeFile(target, bytes, { flag: "wx" });
  record.evidence.push({
    name: reportName,
    kind: "measurement",
    protocol: kind,
    path,
    bytes: bytes.length,
    sha256: sha256(bytes),
    sourceSha256: report.sourceSha256,
    recordedAt: new Date().toISOString(),
  });
  await writeJson(recordPath(name, directory), record);
}
export async function attach(
  name,
  evidenceName,
  input,
  kind = "diagnostic",
  directory = results,
) {
  id(evidenceName);
  if (!["diagnostic", "receipt", "validation", "failure"].includes(kind))
    throw Error("Invalid evidence kind");
  const record = await load(name, directory);
  if (record.status !== "open")
    throw Error("Only open experiments accept evidence");
  const path = `data/${evidenceName}.json`;
  if (record.evidence.some((e) => e.path === path))
    throw Error("Evidence name already exists");
  const bytes = await readFile(input);
  JSON.parse(bytes);
  const target = within(dirname(recordPath(name, directory)), path);
  await mkdir(dirname(target), { recursive: true });
  await writeFile(target, bytes, { flag: "wx" });
  record.evidence.push({
    name: evidenceName,
    kind,
    path,
    bytes: bytes.length,
    sha256: sha256(bytes),
  });
  await writeJson(recordPath(name, directory), record);
}
export async function publish(name, directory = results) {
  const record = await load(name, directory);
  if (!["open", "complete"].includes(record.status) || !record.selection)
    throw Error(
      "Publication needs an open experiment with a preselected report",
    );
  const entry = record.evidence.find((e) => e.name === record.selection.name);
  if (!entry || entry.protocol !== "full")
    throw Error("The preselected full report has not been recorded");
  const bytes = await readFile(
    within(dirname(recordPath(name, directory)), entry.path),
  );
  if (sha256(bytes) !== entry.sha256) throw Error("Report integrity failure");
  validateReport(JSON.parse(bytes), { full: true });
  if (record.status === "open") {
    record.status = "complete";
    record.completedAt = new Date().toISOString();
    await writeJson(recordPath(name, directory), record);
  }
  const catalog = await index(directory);
  catalog.published = {
    experiment: name,
    evidence: entry.path,
    sha256: entry.sha256,
  };
  await writeJson(resolve(directory, "index.json"), catalog);
}
export async function published(directory = results) {
  const catalog = await index(directory);
  if (!catalog.published) throw Error("No published measurement");
  const record = await load(catalog.published.experiment, directory);
  const evidence = record.evidence.find(
    (e) => e.path === catalog.published.evidence,
  );
  if (!evidence) throw Error("Publication points at missing evidence");
  const path = within(dirname(recordPath(record.id, directory)), evidence.path),
    bytes = await readFile(path);
  if (
    sha256(bytes) !== catalog.published.sha256 ||
    sha256(bytes) !== evidence.sha256
  )
    throw Error("Published bytes do not match their recorded hash");
  validateReport(JSON.parse(bytes), { full: true });
  return { record, evidence, bytes, path };
}
export async function verify(directory = results) {
  const catalog = await index(directory),
    seen = new Set();
  let count = 0;
  const allowed = new Set(["README.md", "index.json"]);
  if (catalog.schemaVersion !== 1 || !Array.isArray(catalog.experiments))
    throw Error("Invalid history index");
  for (const item of catalog.experiments) {
    if (seen.has(item.id) || item.path !== `history/${id(item.id)}/record.json`)
      throw Error("Invalid/duplicate history index entry");
    seen.add(item.id);
    const record = await load(item.id, directory);
    allowed.add(item.path);
    for (const entry of record.evidence) {
      allowed.add(`history/${item.id}/${entry.path}`);
      const bytes = await readFile(
        within(dirname(recordPath(record.id, directory)), entry.path),
      );
      if (bytes.length !== entry.bytes || sha256(bytes) !== entry.sha256)
        throw Error(`Evidence checksum mismatch: ${item.id}/${entry.path}`);
      count++;
    }
  }
  async function walk(path, prefix = "") {
    for (const entry of await readdir(path, { withFileTypes: true })) {
      const name = prefix + entry.name;
      if (entry.isSymbolicLink())
        throw Error(`Symlink is not evidence: ${name}`);
      if (entry.isDirectory())
        await walk(resolve(path, entry.name), name + "/");
      else if (!allowed.has(name))
        throw Error(`Unregistered file in results: ${name}`);
    }
  }
  await walk(directory);
  if (catalog.published) await published(directory);
  return { experiments: seen.size, checkedFiles: count };
}
