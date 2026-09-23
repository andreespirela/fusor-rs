# Control-flow integration example

This application exercises `If`/`Else` and `Match`/`Case` with retained component
state, nested lists, projected children, independent and coherent async reads,
and server-rendered fragments.

Run the complete example with its controlled API responses and browser checks:

```sh
just test control-flow
# Include all installed browser engines:
PLAYWRIGHT_BROWSERS=chromium,firefox,webkit just test control-flow
```

The test server deliberately holds `/api/*` responses so tests can inspect the
previous UI before releasing new data. Running the app with the ordinary dev
server exercises the synchronous controls, but does not supply that test API.

For the small, standalone getting-started example, see the docs app's
[control-flow lesson](../../apps/docs/tutorial/lessons/control-flow/) and the
**Conditions and pattern matching** guide at `/docs/control-flow`.
