# Native JavaScript and existing Web Components

This example keeps its historical directory name, but uses ordinary JavaScript
modules. The guides' native JavaScript page explains the contract.

```sh
npm ci --ignore-scripts
fusor dev
```

Run `fusor install` once if the Wasm target/bindgen CLI is missing.
The application enables Fusor's optional `javascript` feature. Its build.rs
only calls `fusor_build::compile_app()`; the CLI bundles installed imports.
No export inventory, npm Rust bindings, or library-specific Rust adapter exists.

- `web/app.js` imports is-lower-case and async-mutex directly inside the App.
- `web/components/chart.js` uses Chart.js inside a reusable ChartPanel. A typed
  `count` input subscription updates the existing chart; cleanup destroys it.
- Shoelace's button uses a side-effect import. Its animation uses dynamic import
  and retains the existing native custom-element property and event behavior.
- `web/style.css` is part of the module build graph and published with the bundle.

Fusor owns surrounding DOM; Chart.js owns its canvas drawing and listeners.
The CLI writes editor declarations to `.fusor/types` during `check` or
`build`, without inspecting library APIs. External TypeScript is transpiled;
TypeScript validation remains an optional separate `tsc --noEmit` step.
After `fusor check`, `npm run typecheck` checks this example's JSDoc
annotations against the generated Rust input declarations and library types.

The example is an independent Cargo workspace. Ordinary Rust applications and
native Cargo checks do not require Node or installed npm dependencies.
