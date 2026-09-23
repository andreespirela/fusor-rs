# fusor documentation

This is a complete fusor application. HTML templates own layout; external
Rust files own application state, search, theme, and typed routes. The managed
`App` handles startup and cleanup. There is no React/Markdown framework runtime.

`content/pages.json` holds curated prose, related links, and code examples. A
section's `source` loads an actual file into its code block at build time. The
scaffold walkthrough uses the CLI's own templates; the component, ownership,
async, and coherent-view guides use the compiling `tutorial/` app. The bindings
and routing lessons, plus optional typed context, in `tutorial/lessons/` are
applied to generated apps in tests.
`build.rs` converts the content to typed static Rust records, then invokes the
standard fusor HTML compiler. `content/resources.json` lists read-only source
references copied into the ignored `public/source/` asset directory, only when
their contents change. Do not hand-edit generated source copies.
Prose fields (`lead`, `body`, and `note`) support inline code in single backticks
and separate paragraphs with a blank line (`\n\n` in JSON). Use code formatting for
identifiers, types, paths, attributes, and expressions; explain one idea per
paragraph. Format a whole expression together, and keep ordinary words such as
“state” or “event” in prose unless they name a variable. Write built-in HTML tags
as `<App>` and Rust types as `App`. Put required setup and instructions in the
body; reserve notes for optional context or caveats.

This is a deliberately small format, not a general Markdown parser:
use section `links` for links and `code`/`source` for full code examples.
`build/prose.rs` validates delimiters and emits typed paragraph/span data;
`web/article.html` renders escaped text inside real `<p>` and `<code>` elements.
HTML-looking examples remain literal text. No raw HTML injection is used.
Keep attributes in the **Template attribute reference** and compiler-provided tags
in **Built-in components**; link their entries to the relevant task-oriented guide.

`web/article.html` renders reusable sections and a table of contents. The
introduction includes a real signal-driven Rust counter.

From the repository root, run `npm ci --prefix apps/docs --ignore-scripts`,
`just site`, then `just preview`. The docs are served beside the landing page
and benchmarks, as they are when deployed.
Open `http://127.0.0.1:8080/docs/`. For development, the normal CLI supports
`fusor dev -p fusor-docs`; the app's base path is `/docs/`.

The information architecture, restrained typography, persistent navigation and
code-first presentation take inspiration from [Deno's documentation](https://docs.deno.com/runtime/).
The design and content are authored for fusor. No remote fonts or UI services
are required. Browser tests cover routing/history, search, theme persistence,
mobile navigation, missing routes and the live example in all three engines.
Search includes section prose and code. Mobile pages expose a native disclosure
with section links. Related source links open plain text in a new tab.

Code is highlighted at build time by Syntect with the additional grammars from
`two-face` (including TOML). `build/highlight.rs` produces typed text tokens for
`web/code.html`; no source is injected as HTML and no highlighter ships in the
browser bundle. Light and dark token colors meet 4.5:1 contrast against the docs
code backgrounds. Keep those background values in sync when changing the theme.

The [showcase](http://127.0.0.1:8080/docs/showcase) has nine interactive examples,
each with a deep link, reset control, guide link, and source viewer. Gallery
metadata lives in `content/showcase.json`. Each slug names matching files in
`src/demos/` and `web/demos/`; those exact files become the highlighted source and
plain-text downloads during the build. JavaScript demos also name a matching
`web/demos/{slug}.js` and set `javascript: true` in their metadata. To add an
example, declare its module in `src/demos/mod.rs`, associate its discovered HTML
with `template!`, and add a content factory in `src/showcase.rs`, metadata, and
a relevant guide link. The route outlet
owns the demo's lifetime; changing pages or resetting it disposes owned work.

The async examples fetch `public/demo-data/` text fixtures over HTTP. Their
deliberate delays and one-time simulated error are labeled in the UI. They don't
require a backend or third-party API. The async/coherent comparison uses the same
loader on both sides: independent `Resource` results alongside an `AsyncBoundary`
with `AsyncValue` reads. Switching products demonstrates a fast price read, a slow stock read, and
cancellation when a selection changes. The shared reset control starts over.
`just test docs` checks all showcase demos,
source fidelity, syntax colors/contrast, row identity, timer cleanup, async
publication, deep links, and responsive navigation in the selected browsers.

Run `just test docs-examples` to build the documented lessons as independent
Cargo apps and exercise bindings, route selection, keyed identity, cleanup,
loading/error/retry, request disposal, and coherent publication in a browser.
Set `PLAYWRIGHT_BROWSERS=chromium,firefox,webkit` for all three engines.

## Guide trees and contextual references

Pages keep their existing URLs. A page may set `parent` to another page's `slug`
to appear as its child; nested slugs such as `async-data/resource` also work on
fresh page loads. Parent and child must have the same `group`. Page order controls
sibling order and previous/next navigation, so put children immediately after
their parent. The build rejects missing parents, cycles, duplicate slugs and
section IDs. Navigation and breadcrumbs are generated from this data, not numeric
page indices. Search reveals matching children and their ancestors; opening a
child directly expands its ancestors. Disclosure buttons work by keyboard.

Use `reference: true` for detailed lookup pages. Keep short explanations in the
guides and use section `links` for a recommended next step. `content/references.json`
is a curated token-to-section index: examples using those tokens automatically
get an “In this example” disclosure, rendered using the existing related-link
component. It scans authored code (including `source` files) at build time, not
application expressions at runtime. Tokens ending in `:` match directive families;
other tokens use identifier boundaries. Targets are validated during the build.
Reference pages omit these automatic links to avoid self-referential clutter.
When introducing a public directive/API, add its explanation and token here.

`just test docs` checks nested deep links, keyboard disclosure, search,
breadcrumbs, mobile navigation and the rendered reference links alongside the
existing guide/showcase checks. `just test docs-examples` additionally builds
and runs the optional `lessons/mounting` example, including removing its retained
scope. With an installed Chrome, set `PLAYWRIGHT_CHANNEL=chrome`.

To rebuild only this app without building benchmarks:

```sh
node --input-type=module -e 'import { buildPackage } from "./scripts/build.mjs"; await buildPackage("fusor-docs");'
```

## Native library showcases

Chart.js and Three.js use native component modules and `JsInputs`. The app’s
`package.json` pins its libraries and esbuild; install with npm explicitly before
a CLI browser build. Cargo’s native workspace checks still need no Node. Dynamic
imports keep both libraries off ordinary Rust-only documentation routes.

The Orbital Garden uses 24 deterministic curved ribbon meshes, standard physical
materials, a generated room environment, one renderer/output color transform,
and simple analytic motion. It uses no post-processing stack or custom shaders.
Rust owns bloom, palette, pause, and selected ribbon; native events return picks.
Pointer orbit, keyboard selection, responsive framing, reduced motion, and
visibility pausing are supported. Cleanup releases the RAF, subscriptions,
listeners, observer, geometries, materials, textures, environment, and renderer.

`tests/tooling/docs-showcase.mjs` verifies interactions, JavaScript source fidelity,
library deferral, resource cleanup, and mobile layout. Chromium screenshots in
`target/screenshots/` provide fixed reduced-motion views for visual review; frame
rates and GPU times are not correctness gates. WebGL 2 is required for the garden.

`just test docs-libraries` builds an independent temporary consumer from the
exact garden source. Two instances verify independent Rust state and events,
keyed renderer retention, survivor cleanup, and cancellation during library load.
