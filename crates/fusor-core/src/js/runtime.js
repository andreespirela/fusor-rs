// Optional native JavaScript component bridge. No scanning, polling, or graph.
export function inputs_create() { return Object.create(null); }
export function input_create(read, observe) {
  const state = { read, observe, listeners: new Map(), next: 0, closed: false };
  const api = Object.freeze({
    get() {
      if (state.closed) throw new Error('fusor: JavaScript input belongs to a disposed component');
      return state.read();
    },
    subscribe(callback) {
      if (typeof callback !== 'function') throw new TypeError('fusor: inputs.subscribe requires a function');
      if (state.closed) throw new Error('fusor: cannot subscribe to a disposed component');
      const id = ++state.next;
      state.listeners.set(id, callback);
      const unsubscribe = () => {
        if (!state.listeners.delete(id) || state.closed) return;
        if (state.listeners.size === 0) state.observe(false);
      };
      try {
        if (state.listeners.size === 1) state.observe(true);
        if (state.closed) throw new Error('fusor: cannot subscribe to a disposed component');
        callback(state.read());
      } catch (error) { unsubscribe(); throw error; }
      return unsubscribe;
    },
  });
  state.api = api;
  return state;
}
export function input_api(state) { return state.api; }
export function input_publish(state, value) {
  if (state.closed) return;
  for (const [id, callback] of [...state.listeners]) {
    if (state.closed) break;
    if (!state.listeners.has(id)) continue;
    try { callback(value); } catch (error) { console.error(error); }
  }
}
export function input_dispose(state) {
  state.closed = true;
  state.listeners.clear();
  state.read = state.observe = null;
}
export function module_create(root, inputs, id) {
  const controller = new AbortController();
  return { root, inputs: Object.freeze(inputs), id, controller, closed: false, cleanups: [] };
}
function cleanup(fn) {
  try { fn(); } catch (error) { console.error(error); }
}
export function module_activate(state) {
  if (state.closed) return;
  const modules = globalThis[Symbol.for('fusor.javascript.modules.v1')];
  const module = modules?.get(state.id);
  if (!module) throw new Error(`fusor: component module ${state.id} is missing; build with cargo fusor build and enable JavaScript tooling`);
  if (!('onMount' in module)) return;
  if (typeof module.onMount !== 'function') throw new TypeError(`fusor: ${state.id} exports onMount, but it is not a function`);
  if (module.onMount.constructor?.name === 'AsyncFunction') throw new TypeError(`fusor: ${state.id} onMount must be synchronous; start async work inside onMount and use signal`);
  const onCleanup = fn => {
    if (typeof fn !== 'function') throw new TypeError('fusor: onCleanup requires a function');
    if (state.closed) cleanup(fn); else state.cleanups.push(fn);
  };
  const result = module.onMount(Object.freeze({ root: state.root, signal: state.controller.signal, inputs: state.inputs, onCleanup }));
  if (result !== undefined) {
    if (typeof result === 'function') onCleanup(result);
    else {
      if (result != null && typeof result.then === 'function') Promise.resolve(result).catch(error => console.error(error));
      throw new TypeError(`fusor: ${state.id} onMount must return nothing or a cleanup function; promises are unsupported`);
    }
  }
}
export function module_dispose(state) {
  if (state.closed) return;
  state.closed = true;
  state.controller.abort();
  while (state.cleanups.length) cleanup(state.cleanups.pop());
  state.root = state.inputs = null;
}
