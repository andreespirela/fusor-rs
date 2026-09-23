# fusor benchmark report

A fusor app that fetches a recorded report through `AsyncValue`, publishes it
through a coherent boundary, and renders/filter its tables with native Rust
signals and keyed HTML components. It does not run benchmarks in visitors' tabs.

Its `assets-build` step copies the published report, methodology and history
pages into `public/`, so `just site` builds it with current data. `just preview` serves it at
`http://127.0.0.1:8080/benchmarks/`. The renderer distinguishes missing results,
timing resolution, gzip sizes and separate JS/Wasm memory accounting.

The independent workloads and runner are in [`../../benchmarks`](../../benchmarks/).
They must finish successfully before replacing a published report. All raw samples
remain downloadable; there is no combined score or unsupported performance claim.
