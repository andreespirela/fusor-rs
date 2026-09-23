# fusor-islands

Independently delivered fusor islands.

An island is a component rendered to HTML on the server and activated in the
browser by its own small Wasm module, loaded only when it is needed. This crate
holds the shared descriptors that both sides agree on. Enable `browser` for the
runtime that loads and activates units.

See `fusor-server` for rendering the HTML side.

Part of [fusor](https://github.com/andreespirela/fusor-rs), which builds reactive web applications from HTML and ordinary Rust. Licensed under the [MIT License](LICENSE).
