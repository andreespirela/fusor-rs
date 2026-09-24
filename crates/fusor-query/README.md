# fusor-query

Shared async queries for fusor, with explicit freshness and retention.

A `QueryClient` binds one loader to its key space. Components that read the same
key share one request and one cached result, which stays fresh and is kept for as
long as you configure. There is no global registry, automatic retry or mutation
layer.

Add it to an application with `fusor add query`, which also adds `fusor-async`.

Part of [fusor](https://github.com/fusor-rs/fusor), which builds reactive web applications from HTML and ordinary Rust. Licensed under the [MIT License](LICENSE).
