# Explicit-save editor

Three typed fields, two simultaneous HTML views, shared queries, and an editing
session retained across routes.

From the repository root, with Node 22+:

```sh
cargo fusor install -p fusor-playground
cargo build -p fusor-cli --locked
cargo fusor build -p fusor-editor --locked
node examples/editor/server.mjs
```

Open <http://127.0.0.1:4180/editor/project>. The backend waits 900 ms before completing
a save. Change Thursday to Friday and press Enter, then type Monday while saving.
Friday becomes the baseline; Monday remains unsaved. Save again to confirm Monday.
`Reserved` demonstrates server validation. Browser reload creates a new session;
closing the demo server resets its in-memory project.

| File | Responsibility |
| --- | --- |
| `src/app.rs`, `web/index.html` | App, routes, query observation, bounded session retention |
| `src/session.rs` | Typed command, fields, save acknowledgment, explicit reviewed reload |
| `src/api.rs` | Application JSON/Fetch transport and response checks |
| `src/views.rs`, `web/views.html` | Reusable HTML forms and distinct IDs per mounted view |
| `backend.mjs`, `server.mjs` | Controlled in-memory backend and local preview proxy |
| `src/browser_tests.rs` | Optional acceptance hooks, excluded from normal builds |

The backend has no authentication or durable storage. Production persistence must
enforce its version precondition atomically and define uncertain-operation lookup.
The framework does not generate backend business rules.

`just test editor` compiles a separate application copy and checks conflicts,
lost responses, refetch, composition, navigation and disposal in real browsers.
Synthetic composition checks do not replace the manual IME check in the guide.
