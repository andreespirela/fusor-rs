<p>
  <img src="apps/landing/public/brand/fusor-horizontal.svg" alt="fusor" width="200">
</p>

# Build web interfaces with Rust and HTML

fusor is a reactive framework for the web. Write HTML templates and native Rust modules; fusor compiles the Rust to WebAssembly and connects your state to the DOM. When a signal changes, the bindings that read it update.

**In development · v0.1** — This is experimental software. APIs will change; don't rely on it for production yet.

[Get started](#get-started) · [How it works](#how-it-works) · [Examples](#examples) · [Contributing](CONTRIBUTING.md)

## A component in two files

The HTML is the template. Expressions and event handlers are Rust:

```html
<!-- web/components/counter.html -->
<template rust:component="Counter">
  <div>
    <output>{{ state.count.get() }}</output>
    <button on:click="state.count.update(|n| *n += 1)">Increment</button>
  </div>
</template>
```

The Rust module owns the state and associates it with the template:

```rust
// src/counter.rs
use fusor::prelude::*;

#[derive(FromInputs)]
pub struct Counter {
    #[local(init = signal(0))]
    count: Signal<i32>,
}

fusor::template!("web/components/counter.html");
```

The button updates a `Signal<i32>`. The output reads that signal, so fusor updates its text when the value changes. Rust compiles through Cargo and runs in the browser as WebAssembly. This example comes from the [landing app](apps/landing/web/components/counter.html). The documentation site has a full walkthrough of templates, modules, and application startup.

## Get started

You need Rust 1.85 or newer. The framework crates are not published yet, so start from a [repository checkout](https://github.com/andreespirela/fusor-rs):

```sh
cargo install --path crates/fusor-cli --locked
fusor new ../my-app --framework-path "$PWD"
cd ../my-app
fusor dev
```

Open the URL printed by `fusor dev` (normally `http://127.0.0.1:4173`). The starter has a working counter component. Edit its HTML or Rust and the dev server rebuilds the app. Run `fusor check` for compiler errors or `fusor build` for a static build in `dist/`.

The [documentation app](apps/docs/) contains the installation walkthrough and authoring guides. To run it from this checkout:

```sh
cargo fusor dev -p fusor-docs
```

## How it works

fusor keeps the boundaries visible:

- **HTML stays HTML.** Components use native markup with Rust expressions for text, attributes, conditions, lists, and events.
- **Rust stays Rust.** State, methods, and imports live in ordinary modules. Cargo and rustc check the generated bindings alongside your code.
- **Updates follow signal reads.** A binding tracks the signals it reads and updates its own DOM target when they change. There is no virtual DOM.
- **Ownership handles cleanup.** Removing a component disposes its listeners, subscriptions, and owned children.

The default application is a client side WebAssembly app. Optional packages add routing, async resources, shared queries, native JavaScript integrations, and server rendered islands. See the [guides](apps/docs/) for their setup and current limits.

## Examples

- [Landing app](apps/landing/) — a complete fusor site with live counter, search, lists, and async examples.
- [Playground](examples/playground/) — Rust written directly inside an HTML page with reactive bindings.
- [Reader](examples/navigation/) — typed navigation and async data across owned views.
- [Editor](examples/editor/README.md) — typed fields, explicit saves, and state shared across views.
- [Integrations](examples/integrations/) — CodeMirror and Chart.js connected to Rust state.

The [documentation showcase](apps/docs/) has more runnable examples. Performance measurements and their methodology live in [benchmarks](benchmarks/README.md).

## Contributing

fusor is an open source project in early development. The [contributing guide](CONTRIBUTING.md) covers the repository setup, checks, and development workflow. The framework and tooling live in `crates/`; runnable applications live in `apps/` and `examples/`.

## License

Licensed under the [MIT License](LICENSE).
