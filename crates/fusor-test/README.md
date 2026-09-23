# fusor-test

Deterministic helpers for testing fusor applications.

`TestExecutor` runs framework futures on demand, `TestClock` moves time forward
without sleeping, `ControlledLoader` lets a test decide when each request
completes and with what, and `OwnerProbe` checks that cleanup ran. None of it
needs a browser.

Add it as a dev-dependency.

Part of [fusor](https://github.com/andreespirela/fusor-rs), which builds reactive web applications from HTML and ordinary Rust. Licensed under the [MIT License](LICENSE).
