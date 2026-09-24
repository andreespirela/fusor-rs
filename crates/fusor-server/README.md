# fusor-server

Server-side HTML rendering for fusor.

Renders fusor components and islands to an HTML string, synchronously, in native
Rust. Your application resolves its own data first and returns the string
through any HTTP framework or build-time tool.

Part of [fusor](https://github.com/fusor-rs/fusor), which builds reactive web applications from HTML and ordinary Rust. Licensed under the [MIT License](LICENSE).
