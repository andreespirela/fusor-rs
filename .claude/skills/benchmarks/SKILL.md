---
name: benchmarks
description: Run the six-framework benchmark suite, record the run, write its note, publish it and rebuild the results site. Use when asked to run, re-run, refresh or publish benchmarks, or to measure a performance change against the published result.
---

# Run and publish the benchmarks

The suite compares fusor with React, Svelte, Solid, Vue and Preact on the same
workloads. `benchmarks/README.md` and `benchmarks/METHODOLOGY.md` are the source
of truth; read the methodology before interpreting numbers. Every command runs
from the repository root.

A run produces one record, `benchmarks/results/history/<id>/record.json`, with
the raw report and a source receipt under `data/`. Publishing points
`benchmarks/results/index.json` at it. `just site` rebuilds the results site at
`/benchmarks/` from that, through the benchmarks app's `assets-build` step.

## 1. Prerequisites

Check each, and stop and tell the user if one is missing:

- Node 22.16 or newer (`node --version`).
- Installed Google Chrome. The published run uses the `chrome` channel; do not
  set `PLAYWRIGHT_CHANNEL` unless the user asks for Playwright's Chromium.
- wasm-bindgen 0.2.117: `cargo fusor install -p fusor-playground` provides it.
- Binaryen **132** `wasm-opt`, passed as `FUSOR_WASM_OPT`. Published results use
  it. If it is not installed, download the release for this host from
  `https://github.com/WebAssembly/binaryen/releases/tag/version_132` into
  `target/tools/`, and check the archive against its `.sha256` file before
  using it. Confirm `wasm-opt --version` prints `wasm-opt version 132`.

Nothing else should be building or running a browser while the suite runs.

## 2. Choose an ID

Use `YYYYMMDD-<slug>`, for example `20260922-baseline` or
`20260930-keyed-swap`. IDs are permanent and cannot be reused. Ask the user for
the purpose if it is not clear; it becomes the title.

## 3. Build and run

```sh
npm ci
npm ci --prefix benchmarks
cargo fusor install -p fusor-playground
FUSOR_WASM_OPT=/path/to/wasm-opt just bench-build
BENCH_SEED=20261004 just bench-run <id> "<title>"
```

`bench-run` takes about five minutes on a recent laptop; a cold `bench-build`
adds several more. Run it in the background and wait for it to finish. Do not
edit anything under `crates/`, `benchmarks/workloads/`, `benchmarks/harness/` or
the lockfiles while it runs: the run refuses to record
if its source fingerprint changes. Editing docs or `benchmarks/tools/` is fine.

Keep `BENCH_SEED=20261004` unless the user wants a different framework order;
the seed is recorded either way. Logs land in `target/benchmarks/runs/<id>/`.

If the run fails, the record is marked `failed` and keeps whatever it measured.
Report the failure and its log to the user. Do not delete the record or retry
under the same ID; a retry needs a new ID.

## 4. Summarize

```sh
just bench summary --report benchmarks/results/history/<id>/data/full.json
```

This prints the median of every metric and the gzip size of every fixture as
Markdown tables, formatted the way the results site shows them. Use it verbatim
in the note rather than retyping numbers.

When a previous run is published, also compare against it:

```sh
just bench compare \
  --baseline benchmarks/results/history/<previous>/data/full.json \
  --candidate benchmarks/results/history/<id>/data/full.json
```

Read `environmentDifferences` first. A different browser, CPU or sample count
means the change is not attributable to code.

## 5. Write the note

Write the note to a scratch file such as `target/benchmarks/<id>-note.json`,
never inside `benchmarks/results/`:

```json
{
  "kind": "decision",
  "title": "<what this run establishes>",
  "format": "markdown",
  "body": "..."
}
```

The body has, in order:

1. One or two sentences on why the run exists, and the commit it measured
   (`git rev-parse --short HEAD`, noting uncommitted changes if any).
2. The build settings: Binaryen 132 or the default pipeline, and the browser.
3. The `summary` tables.
4. What changed against the previous published run, from `compare`: only rows
   whose change is well outside the noise. Treat anything under 0.10 ms as
   unmeasurable, and name environment differences.
5. Limitations: one local machine, 15 samples, no throttling, Chromium-only
   timings.

State measurements, not verdicts. Do not rank frameworks overall or claim
statistical significance; the methodology rules both out.

Then record it:

```sh
just bench note --id <id> --file target/benchmarks/<id>-note.json
```

Add every note before publishing. A published record is immutable.

## 6. Publish and check

Publish only when the user wants the run to replace the published result:

```sh
just bench publish --id <id>
just bench verify
just test-tools
just site
just test benchmarks
```

`test-tools` uses the published report as its fixture, and `test benchmarks`
checks the results site in `dist/benchmarks/` against it. Both must pass.

## 7. Report back

Tell the user the ID, whether it was published, the build settings, the three
or four largest changes against the previous run (or that this is the first
run), and anything that failed. Point them at `just preview` and
`http://127.0.0.1:8080/benchmarks/` to see it.
