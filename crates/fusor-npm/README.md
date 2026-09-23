# fusor-npm

Internal to the fusor CLI.

It bundles an application's JavaScript modules and their npm dependencies with
esbuild during `fusor build`. Applications do not depend on it or call it
directly, and its API can change in any release.

Part of [fusor](https://github.com/andreespirela/fusor-rs), which builds reactive web applications from HTML and ordinary Rust. Licensed under the [MIT License](LICENSE).
