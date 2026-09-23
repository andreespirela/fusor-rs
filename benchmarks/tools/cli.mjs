#!/usr/bin/env node
import { parseArgs } from "node:util";
import { mkdir, rm, readFile, writeFile } from "node:fs/promises";
import { resolve, dirname, relative } from "node:path";
import { createWriteStream } from "node:fs";
import { once } from "node:events";
import { receipt } from "./lib/receipt.mjs";
import { spawn } from "node:child_process";
import {
  root,
  results,
  id,
  readJson,
  writeJson,
  recordPath,
  exists,
} from "./lib/storage.mjs";
import {
  create,
  select,
  note,
  add,
  attach,
  publish,
  load,
  verify,
} from "./lib/history.mjs";
import { compare } from "./lib/compare.mjs";
import { summarize } from "./lib/summary.mjs";
import { prepareSite, renderRecord } from "./lib/render.mjs";
const {
  positionals: [command],
  values: args,
} = parseArgs({
  allowPositionals: true,
  options: Object.fromEntries(
    [
      "id",
      "title",
      "name",
      "reason",
      "file",
      "report",
      "kind",
      "baseline",
      "candidate",
      "output",
    ]
      .map((k) => [k, { type: "string" }])
      .concat([["help", { type: "boolean" }]]),
  ),
});
const help = `Benchmark tooling (run from any working directory):
  init --id ID --title TITLE
  note --id ID --file NOTE.json
  select --id ID --name REPORT --reason REASON   # before that report is measured
  run --id ID [--title TITLE]                   # records a full run; never auto-publishes
  add --id ID --name REPORT --report FILE [--kind full|screen]
  attach --id ID --name NAME --file FILE.json [--kind diagnostic|receipt|validation|failure]
  publish --id ID                              # selected, validated full report only
  verify                                      # recorded evidence, checksums and publication
  compare --baseline FILE --candidate FILE [--output FILE]
  summary --report FILE                       # Markdown medians and gzip sizes for a note
  receipt --output FILE                       # source and production artifact hashes
  render --id ID --output FILE.html
  site                                        # generate site data/history
  New files are never silently overwritten. Completed records are immutable.
`;
const need = (key) => {
  if (!args[key]) throw Error(`Missing --${key}`);
  return args[key];
};
async function emit(value) {
  if (args.output)
    await writeJson(resolve(args.output), value, { exclusive: true });
  else console.log(JSON.stringify(value, null, 2));
}
async function child(script, env, logPath) {
  const log = createWriteStream(logPath, { flags: "wx" });
  await once(log, "open");
  try {
    await new Promise((accept, reject) => {
      const process = spawn(
        globalThis.process.execPath,
        [resolve(root, script)],
        { cwd: root, env, stdio: ["ignore", "pipe", "pipe"] },
      );
      process.stdout.on("data", (bytes) => {
        log.write(bytes);
        globalThis.process.stdout.write(bytes);
      });
      process.stderr.on("data", (bytes) => {
        log.write(bytes);
        globalThis.process.stderr.write(bytes);
      });
      process.on("error", reject);
      process.on("close", (code) =>
        code === 0 ? accept() : reject(Error(`${script} exited ${code}`)),
      );
    });
  } finally {
    log.end();
    await once(log, "finish");
  }
}
let locked = false;
const lock = resolve(root, ".cache/benchmarks/cli.lock");
try {
  if (!command || args.help) {
    console.log(help);
  } else {
    if (
      [
        "init",
        "note",
        "select",
        "add",
        "attach",
        "publish",
        "run",
        "site",
      ].includes(command)
    ) {
      await mkdir(dirname(lock), { recursive: true });
      try {
        await mkdir(lock);
        locked = true;
        await writeFile(
          resolve(lock, "owner.json"),
          JSON.stringify({ pid: process.pid, command }),
        );
      } catch (error) {
        if (error.code === "EEXIST")
          throw Error(
            `Another benchmark writer holds ${lock}. If interrupted, verify its owner process has stopped before removing this lock.`,
          );
        throw error;
      }
    }
    switch (command) {
      case "init":
        await create(need("id"), need("title"));
        break;
      case "note":
        await note(need("id"), await readJson(resolve(need("file"))));
        break;
      case "select":
        await select(need("id"), need("name"), need("reason"));
        break;
      case "add":
        await add(
          need("id"),
          need("name"),
          resolve(need("report")),
          args.kind || "screen",
        );
        break;
      case "attach":
        await attach(
          need("id"),
          need("name"),
          resolve(need("file")),
          args.kind || "diagnostic",
        );
        break;
      case "publish":
        await publish(need("id"));
        break;
      case "verify":
        await emit(await verify(results));
        break;
      case "compare":
        await emit(
          compare(
            await readJson(resolve(need("baseline"))),
            await readJson(resolve(need("candidate"))),
          ),
        );
        break;
      case "summary":
        process.stdout.write(summarize(await readJson(resolve(need("report")))));
        break;
      case "render":
        await writeFile(
          resolve(need("output")),
          renderRecord(await load(need("id"))),
          { flag: "wx" },
        );
        break;
      case "site":
        await prepareSite();
        break;
      case "receipt":
        need("output");
        await emit(await receipt());
        break;
      case "run": {
        const name = id(need("id")),
          output = resolve(root, "target/benchmarks/runs", name);
        // Refuse both record and scratch collisions before running any measurement.
        await mkdir(dirname(output), { recursive: true });
        await mkdir(output);
        await create(name, args.title || name);
        await select(
          name,
          "full",
          "Single full run selected before measurement; publication is a separate explicit command.",
        );
        try {
          const env = { ...process.env, BENCH_OUTPUT_DIR: output };
          const before = await receipt();
          await writeJson(resolve(output, "receipt.json"), before, {
            exclusive: true,
          });
          await child(
            "benchmarks/harness/ssr.mjs",
            env,
            resolve(output, "ssr.log"),
          );
          await child(
            "benchmarks/harness/run.mjs",
            env,
            resolve(output, "browser.log"),
          );
          const after = await receipt();
          if (JSON.stringify(before) !== JSON.stringify(after))
            throw Error(
              "Sources or production artifacts changed during measurement",
            );
          await add(name, "full", resolve(output, "latest.json"), "full");
          await attach(
            name,
            "receipt",
            resolve(output, "receipt.json"),
            "receipt",
          );
          console.log(
            `Recorded ${name}. Review with render/compare; publish explicitly with --id ${name}.`,
          );
        } catch (error) {
          for (const file of ["latest.json", "ssr.json", "receipt.json"]) {
            const path = resolve(output, file);
            if (await exists(path))
              await attach(
                name,
                `failed-${file.slice(0, -5)}`,
                path,
                "failure",
              );
          }
          const record = await load(name);
          record.status = "failed";
          record.events.push({
            event: "failure",
            at: new Date().toISOString(),
            message: error.message,
            localOutput: relative(root, output),
          });
          await writeJson(recordPath(name), record);
          throw error;
        }
        break;
      }
      default:
        throw Error(`Unknown command: ${command}\n${help}`);
    }
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
} finally {
  if (locked) await rm(lock, { recursive: true });
}
