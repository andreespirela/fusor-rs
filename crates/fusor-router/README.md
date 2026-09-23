# fusor-router

Typed routes and URLs for fusor, with History API navigation.

Routes are an ordinary Rust enum implementing `Route`: parsing and formatting
are plain functions, and every URL is relative to the application's base path.
Enable `browser` for the router, outlets and navigation through the browser's
history.

Add it to an application with `fusor add router`.

Part of [fusor](https://github.com/andreespirela/fusor-rs), which builds reactive web applications from HTML and ordinary Rust. Licensed under the [MIT License](LICENSE).
