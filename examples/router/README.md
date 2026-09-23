# Nested HTML routing

From the repository root, after `cargo fusor install -p fusor-playground`:

```sh
  cargo fusor dev -p fusor-router-example
```

Open `/nested/dashboard/settings/preferences` on the printed development URL.
The entry HTML selects Dashboard; Dashboard's separate component template contains
another Router, and Settings contains a third. The parent counter survives child
navigation. Route captures are plain String inputs; query-dependent Filter uses
`Navigation::location()`.

`src/memory.rs` demonstrates a custom memory-backed router using public APIs only.
Its navigation does not change browser history. The `Broken` component and Wasm
exports are test probes for failed setup, staging and cleanup; normal applications
need neither. The guides' routing page has a minimal app.

```sh
PLAYWRIGHT_BROWSERS=chromium,firefox,webkit just test router
```
