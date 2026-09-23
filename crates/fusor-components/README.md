# fusor-components

First-party structural HTML components for Fusor. `<App>` starts and retains
a browser application with inferred Rust state; enable the `browser` feature.
`<ForEach>` remains available without browser dependencies.

The list built-in,
`<ForEach>`, repeats inline HTML with stable keyed identity and reactive item and
index bindings. Application code owns the collection; this crate projects each
row's value and position for the compiler-generated bindings.

The guides' ForEach page covers syntax, examples and the current rendering
constraints. The existing core reconciler handles DOM identity, lifecycle,
coherent updates, and hydration.

`<Children></Children>` places caller-supplied HTML without a wrapper or Rust
input field.

`<Async>` coordinates descendant reads with an automatic boundary. `<Await
value="{{ state.read }}" let="result">` names the successful value and works
independently when no Async encloses it. Both require one native HTML root and
add no DOM wrapper.
