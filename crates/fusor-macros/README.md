# fusor-macros

Derive macros for fusor components.

`#[derive(FromInputs)]` turns a component's `#[input]` and `#[local]` fields
into its typed constructor inputs. `#[derive(JsInputs)]` does the same for the
values a component passes to its JavaScript module.

Use them through `fusor`, which re-exports both with its `dom` feature. You do
not need to depend on this crate directly.

Part of [fusor](https://github.com/andreespirela/fusor-rs), which builds reactive web applications from HTML and ordinary Rust. Licensed under the [MIT License](LICENSE).
