# fusor-async

Keyed async reads for fusor, with loading, error and cancellation.

A read is driven by a key made from signals. When the key changes, the previous
request is cancelled and the latest one wins; the owning component's cleanup
cancels whatever is still running. There is no implicit cache or retry.

Enable `browser` for the browser executor and a cancellable `fetch` adapter.
Add it to an application with `fusor add async`.

Part of [fusor](https://github.com/fusor-rs/fusor), which builds reactive web applications from HTML and ordinary Rust. Licensed under the [MIT License](LICENSE).
