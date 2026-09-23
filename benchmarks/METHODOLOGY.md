# fusor benchmark methodology

This suite measures six independently built production applications performing
equivalent application work. The [history index](results/index.json) identifies
the published result and the record for every run. Notes live inside each
record's `notes` array; executable tooling is shared under `tools/`.
The documentation and results websites are fusor applications; the five
comparison frameworks are used only in their own isolated benchmark applications.

## Reproduce

Requirements: the repository's Rust toolchain and Wasm target, pinned wasm-bindgen
CLI (`cargo fusor install -p fusor-playground`), Node 22.16 or newer, and an
installed Chrome. The published run additionally enables Binaryen 132 via
`FUSOR_WASM_OPT`; install that tool separately. Omitting the variable measures
the default build pipeline.

```sh
npm ci
npm ci --prefix benchmarks
cargo fusor install -p fusor-playground
npx playwright install chromium
FUSOR_WASM_OPT=/path/to/binaryen-132/wasm-opt just bench-build
BENCH_SEED=20261004 just bench-run my-comparison
just bench publish --id my-comparison
just site
just preview
```

The runner defaults to the installed `chrome` channel, which the published run
uses. Set `PLAYWRIGHT_CHANNEL=chromium` to select Playwright's Chromium instead.
During a run, the runner serves each production comparison application at
`/workloads/<framework>/`. `just preview` serves the published results at
`/benchmarks/`, beside the docs.

`just bench-build` builds all six browser workloads, all server renderers, and twelve
independent Hello World/Todo fixtures. Cargo's JSON artifact receipt locates the
native renderer; the runner does not guess a target directory. `just bench-run` runs
server measurements first, then the browser measurements. The shared runner preserves raw samples under `results/history/<id>/data/`.
Temporary JSON, CSV and logs go to `target/benchmarks/runs/<id>/`; failed checks
remain visible and exit nonzero. Publication is explicit and updates the history
index only after validating the preselected full report.
`just site` copies the published record into the results application.

For smoke testing only, `BENCH_FRAMEWORKS=solid BENCH_SAMPLES=1 BENCH_WARMUPS=0
BENCH_SKIP_MEMORY=1 node benchmarks/harness/run.mjs` selects a subset. Do not publish
that output as the full baseline. Direct harness invocations write to `target/benchmarks/current/` by default;
set `BENCH_OUTPUT_DIR` to an isolated directory when needed. They do not publish.
Server measurements always use 15 samples and
3 warmups. Run from the repository root. The normal browser run also uses 15
samples and 3 warmups. `BENCH_SEED` controls the recorded, shuffled framework order.

## Versions and production settings

Exact versions and lockfiles are checked in. The report records framework, browser,
Node, Rust, OS, CPU, sample count, run order, and a source SHA-256. Each transferred
bundle file has its own SHA-256.

JavaScript applications use Vite's production compiler/minifier and official
framework compilation paths. fusor uses the shared workspace release profile
(`opt-level=3`, LTO, one codegen unit, aborting panics), pinned wasm-bindgen, and the
framework's production CLI. The published run explicitly enables Binaryen
132 `-O3` after wasm-bindgen (bulk-memory, reference-types, multivalue, sign-ext
and nontrapping-float-to-int enabled). This is an optional production CLI step,
applied to all three fusor fixtures. No custom compression or native-server
bytecode step is applied.

The production CLI removes the nonsemantic Wasm `name` section. Development and
debug builds retain names; `FUSOR_KEEP_WASM_NAMES=1` retains them in optimized
builds for profiling. The measured release bundles use the default stripped names.
This reduces download bytes but also removes Rust function names from release stack
traces. No executable exports are removed by this option.

## Workloads and correctness

The common visible workload is a section with an aggregate output and a keyed list.
Each row has an ID, a displayed value, and a button. A “component” here means this
row implementation, not an equivalent count of internal framework allocations.

| Criterion | Measured work |
| --- | --- |
| Initial render | Create 1,000 or 10,000 rows after the runtime is loaded |
| Single update | Update row 5,000 in a 10,000-row tree |
| Bulk update | Increment the first 1,000 or all 10,000 values in one application operation |
| Insert / delete | Insert or delete one middle row in a 10,000-row keyed list |
| Swap | Exchange rows 1 and 9,998; retain every surviving row node |
| Computed | Change an input, recompute its doubled value, publish its text |
| Fan-out | One shared source updates 1,000 or 10,000 row outputs |
| Fan-in | Increment 10,000 inputs and recompute one aggregate sum |
| Lifecycle | Unmount a 10,000-row application; initial render measures creation separately |
| Events | Dispatch 10,000 native button clicks, then finish all resulting DOM updates |
| Memory | Empty baseline, live 10k/100k rows, cleanup, and ten 10k mount/unmount cycles |
| Bundle sizes | Independent Hello World, 20-task CRUD/filter Todo, and full benchmark application |
| SSR | Render 1,000 or 10,000 rows to a completed HTML string |
| Hydration | Attach behavior to the actual server renderer's 1k/10k rows |
| Startup | Fresh browser context navigation to 1,000 committed interactive rows |

Every DOM timing is followed by untimed checks of row count, IDs, text values, and
aggregate output when applicable. Keyed operations additionally check all retained
row identities and record inserted/removed/moved elements with MutationObserver.
We report the actual move count, including algorithms that move more than two rows.
MutationObserver bookkeeping also runs within the structural-operation timing
window. Its cost varies with the actual mutation count, so these timings cannot
isolate framework CPU cost from the instrumentation used to validate DOM work.
The MutationObserver overhead is included equally in structural tests.

Hydration uses a fresh document for every sample. Actual native/JavaScript SSR HTML
is inserted before timing; its official bootstrap is executed before attachment.
Every row must retain its identity, and the first button must increment exactly
once. Solid's generated hydration bootstrap is included in its SSR HTML size.
This measures warm-runtime attachment, separately from startup/download cost.
HTML parsing and network transfer are excluded from the hydration timer.

Hello World must display its heading. Each Todo must add a task, mark it complete,
filter to completed tasks, delete a completed task, and filter back to active tasks.
The fixtures build independently so a shared chunk from another page cannot hide
framework bytes. The Todo is a deliberately small realistic application, not a
full dashboard. Neither fixture imports benchmark control code.

## Timing boundaries

Browser timings use `performance.now()` around the operation and its framework's
DOM commit barrier. They exclude layout, paint, and user-perceived frame latency.
Setup/reset and validation are outside the clock. An individual event's scheduled
work follows that framework's normal event semantics; the final barrier drains the
operation. Different batching behavior is therefore visible in the event result.

React and Preact use `flushSync` for explicit imperative operations; Svelte uses
`flushSync`; Solid uses its synchronous signal propagation with `batch` for bulk
work; Vue awaits `nextTick`; fusor uses synchronous signal propagation and
`batch`. React/Preact hydration additionally waits for a layout-effect acknowledgment.
React warns that `flushSync` can hurt performance. It is an explicit measurement
barrier here, not an application architecture recommendation.

React/Preact use component-local state for independent rows and aggregate array
state for fan-in. Signal frameworks track individual reactive inputs. These tests
compare the visible application operation, not equal reactive graph implementations.
They must not be cited as proof that one graph algorithm dominates another.

Reported values are the median, nearest-rank P95, min, max, and every raw sample.
With 15 samples P95 is the maximum; it is not a stable tail-latency estimate.
Sub-0.10 ms values are displayed as `<0.10` because the browser clock quantizes
very short operations. Small apparent wins are noise, not a reliable ranking.
The report highlights minimum timings within 0.05 ms; it has no composite score.
Startup has 15 fresh-context samples and no discarded warmups (each is a cold
browser-context navigation). A warm OS/network cache and a reused browser process
remain; “cold context” does not mean a cold device.

Server timings use Rust's `Instant` or Node's `performance.now()`, after three
warmups, for 15 completed string renders. Rust and JavaScript execute in different
processes/runtimes and produce different metadata overhead. Each record includes
HTML bytes. Process startup, transport, parsing, and streaming are excluded.

The baseline runs sequentially on one local machine without network/CPU throttling,
thermal controls, or statistical confidence intervals. Startup fetches uncompressed
assets over localhost; gzip bundle figures are calculated separately. Do not
extrapolate this startup result to a constrained mobile connection. Browser
correctness is separately checked in Chromium, Firefox, and WebKit; performance
numbers in this report are Chromium-only.

## Memory and bundle accounting

Chromium CDP records `Runtime.getHeapUsage` and `Memory.getDOMCounters`, after two
animation frames and two forced collections. The report preserves every snapshot.
Heap deltas are relative to the same application's empty baseline. JS heap alone
is not total framework memory: Rust allocations live in WebAssembly linear memory.
Its capacity is reported separately, including its post-100k high-water allocation.
Freed Wasm allocations can be reused without shrinking that capacity. Capacity is
not live allocation size and must not be added to JS heap without accounting for
engine backing-store measurements. This suite does not measure process RSS or
precise live Rust allocation bytes, and cannot support a whole-process RAM ranking.

The template runtime retains at most 32 inert, validated template snapshots for
reuse. These bounded DOM certificates contain no application owners or listeners,
but their DOM and JavaScript retention is present in the absolute snapshots.
The empty memory baseline is collected after warm DOM timings, so template/string
caches are already warm. Their finite retention is therefore excluded from
subsequent baseline-relative deltas; zero cycle-node growth does not imply zero
cache overhead.

The runtime also retains at most 32 descriptor-derived primitive-string entries.
Common click/input/change event names borrow static handles; custom event names
remain owned. A separate diagnostic records live externrefs and retained table
capacity to explain one memory mechanism. Table capacity is not a live-handle
count, and the diagnostic is not a replacement for the fixed memory workload.

Ten mount/unmount cycles identify conspicuous retention; they cannot prove that
all applications are leak-free. The public page shows the final heap delta, Wasm
capacity and DOM-node delta; raw JSON includes the complete trend. Browser-owned
bookkeeping can leave a small heap delta even when all application nodes disappear.

Bundle bytes sum actual entry HTML and fetched JS/Wasm/CSS resources. Gzip uses
level 9 independently per file, with no cross-file dictionary. Wire headers/TLS,
Brotli, source maps, service workers, and CDN cache reuse are excluded. All transfer
categories remain present even if fusor's output is substantially larger.

## Primary implementation references

- [React flushSync](https://react.dev/reference/react-dom/flushSync)
- [React hydrateRoot](https://react.dev/reference/react-dom/client/hydrateRoot)
- [Svelte imperative component API](https://svelte.dev/docs/svelte/imperative-component-api)
- [Svelte flushSync](https://svelte.dev/docs/svelte/svelte#flushSync)
- [Solid batch](https://docs.solidjs.com/reference/reactive-utilities/batch)
- [Solid hydration script](https://docs.solidjs.com/reference/rendering/hydration-script)
- [Vue nextTick](https://vuejs.org/api/general.html#nexttick)
- [Preact API reference](https://preactjs.com/guide/v10/api-reference/)

Unsupported work is not assigned a zero. Streaming SSR, server async seeding,
mobile/network profiles, and process-wide memory are outside this baseline.
