// Delivery protocol v1. This is the only loader/scheduler used by HTML and Rust.
// Unit state owns code; instance state owns behavior. Fetching is never mounting.
const VERSION = 1;
const schedulerDefaults = Object.freeze({ rootMargin: '0px', idleTimeout: 2000 });
const activationPolicies = new Set(['load', 'visible', 'idle', 'interaction', 'manual']);
const prefetchPolicies = new Set(['none', 'load', 'visible', 'idle']);
const metadataNames = ['id', 'data-rf-island', 'data-rf-unit', 'data-rf-generation', 'data-rf-schema', 'data-rf-hash', 'data-rf-activate', 'data-rf-prefetch'];

function failure(code, message, cause) {
  return Object.assign(new Error(message, cause ? { cause } : undefined), { code });
}
function cancelled() { return failure('cancelled', 'The island request no longer has a live caller.'); }
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

export function install(manifest, { root = document } = {}) {
  if (globalThis.__fusor_islands) throw failure('protocol-mismatch', 'An island registry is already installed.');
  if (!manifest || manifest.version !== VERSION || typeof manifest.generation !== 'string' || !manifest.generation
    || !manifest.units || typeof manifest.units !== 'object' || Array.isArray(manifest.units)) throw failure('protocol-mismatch', 'Unsupported island manifest.');
  const generation = manifest.generation;
  const units = new Map(), instances = new Map(), ids = new Map(), elements = new WeakMap();
  const descriptors = new Set();
  // Validate and retain the same definitions before acquiring any global resources.
  for (const [name, value] of Object.entries(manifest.units)) {
    if (!name || !value || !Array.isArray(value.dependencies === undefined ? [] : value.dependencies) || !Array.isArray(value.entries) || !value.entries.length) throw failure('protocol-mismatch', 'Invalid delivery unit entries.');
    const definition = { javascript: value.javascript, wasm: value.wasm, dependencies: [...(value.dependencies ?? [])], entries: value.entries.map(entry => ({ ...entry })) };
    for (const path of [definition.javascript, definition.wasm, ...definition.dependencies]) {
      if (typeof path !== 'string' || !path.startsWith('/') || path.startsWith('//') || path.includes('..') || /[?#\\]/.test(path)) throw failure('protocol-mismatch', 'Delivery URLs must be immutable same-origin paths.');
      const url = new URL(path, location.href);
      if (url.origin !== location.origin || url.search || url.hash) throw failure('protocol-mismatch', 'Delivery URLs must be immutable same-origin paths.');
    }
    for (const entry of definition.entries) {
      if (entry.unit !== name || !['descriptor', 'props_schema', 'template_hash'].every(field => typeof entry[field] === 'string' && entry[field])
        || !['attach', 'preview'].includes(entry.mode) || descriptors.has(entry.descriptor)) throw failure('protocol-mismatch', 'Invalid delivery unit entries.');
      descriptors.add(entry.descriptor);
    }
    units.set(name, { definition, state: 'absent', generation: 0, claims: new Set(), available: null, initialization: null, module: null, controller: null, error: null, recoverable: true });
  }
  let sequence = 0n;
  const previousComposing = globalThis.__fusor_composing;
  const composing = previousComposing ?? new WeakSet();
  const onCompositionStart = event => composing.add(event.target);
  const onCompositionEnd = event => composing.delete(event.target);

  function state(instance, next, error = null) {
    if (instance.state === 'disposed') return;
    instance.state = next; instance.error = error;
    instance.host.setAttribute('data-rf-status', next);
    if (error) instance.host.setAttribute('data-rf-error', error.code || 'load-failed');
    else instance.host.removeAttribute('data-rf-error');
    instance.host.dispatchEvent(new CustomEvent('fusor:island-status', { detail: { id: instance.id, status: next, error }, bubbles: true }));
  }
  function valid(instance) {
    return instances.get(instance.token) === instance && instance.host.isConnected
      && metadataNames.every(name => instance.host.getAttribute(name) === instance.metadata[name])
      && instance.propsNode.parentElement === instance.host && instance.propsNode.matches('script[type="application/json"][data-rf-props]') && instance.propsNode.textContent === instance.props;
  }
  function target(token) {
    const instance = instances.get(token);
    if (!instance || !valid(instance)) throw failure('stale-instance', 'This island registration was removed or changed.');
    return instance;
  }
  function wantsActivation(instance) {
    return [...instance.claims].some(claim => claim.kind === 'activate' && !claim.done);
  }

  function preloadModule(url, signal) {
    const link = document.createElement('link');
    if (!link.relList.supports('modulepreload')) {
      // Older engines still get a safe HTTP-cache prefetch. They may refetch
      // glue on import; Wasm bytes and all activation ownership remain shared.
      return fetch(url, { signal, credentials: 'same-origin' }).then(response => {
        if (!response.ok) throw failure('load-failed', `Cannot prefetch module ${url}: HTTP ${response.status}.`);
        return response.arrayBuffer();
      });
    }
    return new Promise((resolve, reject) => {
      const cleanup = () => { link.remove(); signal.removeEventListener('abort', abort); link.onload = link.onerror = null; };
      const abort = () => { cleanup(); reject(cancelled()); };
      link.rel = 'modulepreload'; link.href = url; link.crossOrigin = 'anonymous';
      link.onload = () => { cleanup(); resolve(); };
      link.onerror = () => {
        cleanup();
        reject(Object.assign(failure('load-failed', `Cannot preload module ${url}; reload after correcting this generation.`), { needsReload: true }));
      };
      signal.addEventListener('abort', abort, { once: true });
      if (signal.aborted) { abort(); return; }
      document.head.append(link);
    });
  }

  async function available(unit) {
    if (unit.state === 'failed') throw unit.error;
    if (unit.available) return unit.available;
    const generation = ++unit.generation;
    const controller = unit.controller = new AbortController();
    unit.state = 'fetching';
    unit.available = (async () => {
      const [, bytes] = await Promise.all([
        Promise.all([unit.definition.javascript, ...unit.definition.dependencies].map(url => preloadModule(url, controller.signal))),
        fetch(unit.definition.wasm, { signal: controller.signal, credentials: 'same-origin' }).then(response => {
          if (!response.ok) throw failure('load-failed', `Cannot load immutable Wasm ${unit.definition.wasm}: HTTP ${response.status}.`);
          return response.arrayBuffer();
        }),
      ]);
      if (unit.generation !== generation) throw cancelled();
      unit.state = 'available'; unit.controller = null;
      return bytes;
    })().catch(error => {
      if (unit.generation === generation) {
        unit.controller = null; unit.available = null;
        if (controller.signal.aborted) { unit.state = 'absent'; }
        else { unit.state = 'failed'; unit.recoverable = !error.needsReload; unit.error = failure('load-failed', error.message, error); }
      }
      throw error;
    });
    return unit.available;
  }
  async function initialize(unit) {
    if (unit.state === 'failed') throw unit.error;
    if (unit.initialization) return unit.initialization;
    const bytes = await available(unit);
    if (unit.initialization) return unit.initialization;
    unit.state = 'initializing';
    unit.initialization = (async () => {
      const module = await import(unit.definition.javascript);
      if (typeof module.default !== 'function' || typeof module.__rf_manifest !== 'function' || typeof module.__rf_activate !== 'function' || typeof module.__rf_dispose !== 'function') throw failure('load-failed', 'A delivery unit is missing its typed entry exports.');
      await module.default({ module_or_path: bytes });
      const witness = JSON.parse(module.__rf_manifest());
      const fields = ['unit', 'descriptor', 'props_schema', 'template_hash', 'mode'];
      if (witness.version !== VERSION || !Array.isArray(witness.entries) || witness.entries.length !== unit.definition.entries.length
        || unit.definition.entries.some(expected => !witness.entries.some(actual => fields.every(field => actual[field] === expected[field])))) throw failure('descriptor-mismatch', 'Loaded Wasm registrations differ from this page’s immutable manifest.');
      unit.module = module; unit.state = 'ready';
      return module;
    })().catch(error => {
      unit.state = 'failed'; unit.recoverable = false;
      unit.error = failure('load-failed', 'Unit initialization failed. Publish a corrected generation and reload; failed module evaluation cannot be retried safely at the same URL.', error);
      throw unit.error;
    });
    return unit.initialization;
  }
  function stopUnusedFetch(unit) {
    if (!unit.claims.size && unit.state === 'fetching') {
      ++unit.generation;
      unit.controller?.abort(); unit.controller = null;
      unit.available = null; unit.state = 'absent';
    }
  }
  function waitForComposition(instance, generation) {
    const active = document.activeElement;
    if (!active || !instance.host.contains(active) || !composing.has(active)) return Promise.resolve();
    const waiting = deferred();
    const cleanup = () => { document.removeEventListener('compositionend', end, true); instance.compositionWaiters.delete(cancel); };
    const cancel = () => { cleanup(); waiting.reject(cancelled()); };
    const end = event => {
      if (event.target !== active) return;
      cleanup();
      // Run after the composition event's native input handlers have completed.
      queueMicrotask(() => valid(instance) && instance.operation === generation ? waiting.resolve() : waiting.reject(cancelled()));
    };
    instance.compositionWaiters.add(cancel);
    document.addEventListener('compositionend', end, true);
    return waiting.promise;
  }
  function activate(instance) {
    if (instance.state === 'active') return Promise.resolve();
    if (instance.work) return instance.work;
    const generation = ++instance.operation;
    state(instance, 'requested');
    const work = (async () => {
      const module = await initialize(instance.unit);
      if (!valid(instance) || instance.operation !== generation || !wantsActivation(instance)) throw cancelled();
      await waitForComposition(instance, generation);
      if (!valid(instance) || instance.operation !== generation || !wantsActivation(instance)) throw cancelled();
      state(instance, 'binding');
      const bindingToken = `${instance.token}/attempt-${generation}`;
      instance.bindingToken = bindingToken;
      try {
        // No await between inspecting the native DOM and synchronous Rust bind.
        // Generated Rust adopts live control values before its binding effects.
        const binding = module.__rf_activate(instance.entry.descriptor, instance.host, instance.props, bindingToken);
        // Preview mode waits for its first coherent publication. Attach mode
        // has already adopted native state synchronously before this await.
        await binding;
      } catch (error) {
        module.__rf_dispose(bindingToken);
        throw failure('binding-failed', 'The island could not attach to its initial HTML.', error);
      }
      if (!valid(instance) || instance.operation !== generation || !wantsActivation(instance)) {
        module.__rf_dispose(bindingToken); throw cancelled();
      }
      state(instance, 'active');
    })().catch(error => {
      if (instances.get(instance.token) === instance && instance.operation === generation) {
        state(instance, error.code === 'cancelled' ? 'dormant' : 'failed', error.code === 'cancelled' ? null : error);
      }
      throw error;
    }).finally(() => { if (instance.operation === generation) instance.work = null; });
    instance.work = work;
    return work;
  }

  function request(token, kind) {
    const instance = target(token);
    if (!['prefetch', 'activate', 'retry'].includes(kind)) throw failure('invalid-operation', 'Unknown island operation.');
    if (kind === 'retry') {
      if (instance.unit.state === 'failed') {
        if (!instance.unit.recoverable) throw instance.unit.error;
        instance.unit.state = 'absent'; instance.unit.available = null; instance.unit.error = null;
      }
      if (instance.state === 'failed') { ++instance.operation; instance.work = null; state(instance, 'dormant'); }
      kind = 'activate';
    } else if (instance.state === 'failed') { throw instance.error; }
    const waiting = deferred();
    const claim = { kind, done: false, cancel: null };
    instance.claims.add(claim); instance.unit.claims.add(claim);
    function finish(error) {
      if (claim.done) return;
      claim.done = true; instance.claims.delete(claim); instance.unit.claims.delete(claim);
      if (kind === 'activate' && !wantsActivation(instance) && ['requested', 'binding'].includes(instance.state)) {
        ++instance.operation; instance.work = null;
        for (const cancel of [...instance.compositionWaiters]) cancel();
        if (instance.bindingToken) instance.unit.module?.__rf_dispose(instance.bindingToken);
        instance.bindingToken = null;
        state(instance, 'dormant');
      }
      stopUnusedFetch(instance.unit);
      if (error) waiting.reject(error); else waiting.resolve();
    }
    claim.cancel = () => finish(cancelled());
    const work = kind === 'prefetch' ? available(instance.unit) : activate(instance);
    work.then(() => finish(null), finish);
    // A Rust future can be dropped before JsFuture starts polling the Promise.
    // Keep cancellation rejections handled without swallowing the caller result.
    waiting.promise.catch(() => {});
    return { promise: waiting.promise, cancel: claim.cancel };
  }

  function schedule(instance, policy, kind) {
    const start = () => {
      if (!valid(instance)) return;
      try { const operation = request(instance.token, kind); operation.promise.catch(error => { if (error.code !== 'cancelled') console.error(error); }); }
      catch (error) { state(instance, 'failed', error); }
    };
    if (policy === 'load') { start(); }
    else if (policy === 'visible') {
      if (!globalThis.IntersectionObserver) { start(); return; }
      const observer = new IntersectionObserver(entries => {
        if (entries.some(entry => entry.isIntersecting)) { observer.disconnect(); instance.triggers.delete(cancel); start(); }
      }, { rootMargin: schedulerDefaults.rootMargin });
      const cancel = () => observer.disconnect();
      instance.triggers.add(cancel); observer.observe(instance.host);
    } else if (policy === 'idle') {
      let cancel;
      const run = () => { instance.triggers.delete(cancel); start(); };
      if (globalThis.requestIdleCallback) { const id = requestIdleCallback(run, { timeout: schedulerDefaults.idleTimeout }); cancel = () => cancelIdleCallback(id); }
      else { const id = setTimeout(run, 0); cancel = () => clearTimeout(id); }
      instance.triggers.add(cancel);
    }
  }

  function register(host) {
    if (elements.has(host)) return;
    if (host.parentElement?.closest('[data-rf-island]')) throw failure('nested-island', 'Nested independent islands are unsupported.');
    const metadata = Object.fromEntries(metadataNames.map(name => [name, host.getAttribute(name)]));
    const id = metadata.id, unit = units.get(metadata['data-rf-unit']);
    const entry = unit?.definition.entries.find(entry => entry.descriptor === metadata['data-rf-island']);
    if (!id || ids.has(id) || document.getElementById(id) !== host || document.querySelectorAll(`[id="${CSS.escape(id)}"]`).length !== 1) throw failure('duplicate-instance', 'Island IDs must be unique in the document.');
    if (!entry || metadata['data-rf-generation'] !== generation || entry.props_schema !== metadata['data-rf-schema'] || entry.template_hash !== metadata['data-rf-hash']) throw failure('descriptor-mismatch', `Island ${id} does not match this page’s generation.`);
    const policy = metadata['data-rf-activate'] || 'load', prefetch = metadata['data-rf-prefetch'] || 'none';
    if (!activationPolicies.has(policy) || !prefetchPolicies.has(prefetch)) throw failure('protocol-mismatch', 'Unsupported island scheduling policy.');
    const propsNodes = [...host.children].filter(node => node.matches('script[type="application/json"][data-rf-props]'));
    if (propsNodes.length !== 1) throw failure('descriptor-mismatch', 'An island needs one inert props payload.');
    const token = `island-${++sequence}`;
    const instance = { id, token, host, entry, unit, metadata, propsNode: propsNodes[0], props: propsNodes[0].textContent, state: 'dormant', bindingToken: null, operation: 0, error: null, work: null, claims: new Set(), triggers: new Set(), compositionWaiters: new Set() };
    instances.set(token, instance); ids.set(id, instance); elements.set(host, instance);
    state(instance, 'dormant');
    schedule(instance, prefetch, 'prefetch'); schedule(instance, policy, 'activate');
  }
  function discover(node) {
    const hosts = [...(node.matches?.('[data-rf-island]') ? [node] : []), ...node.querySelectorAll?.('[data-rf-island]') || []];
    for (const host of hosts) {
      try { register(host); }
      catch (error) { host.setAttribute('data-rf-error', error.code || 'protocol-mismatch'); console.error(error); }
    }
  }
  function dispose(instance) {
    if (instances.get(instance.token) !== instance) return;
    // Reentrant cleanup must see an expired token before any Rust destructor runs.
    instances.delete(instance.token); ids.delete(instance.id); elements.delete(instance.host);
    ++instance.operation;
    for (const cancel of instance.triggers) cancel(); instance.triggers.clear();
    for (const cancel of [...instance.compositionWaiters]) cancel();
    for (const claim of [...instance.claims]) claim.cancel();
    if (instance.bindingToken) instance.unit.module?.__rf_dispose(instance.bindingToken);
    instance.bindingToken = null;
    if (!elements.has(instance.host)) state(instance, 'disposed');
    instance.work = null; instance.props = ''; instance.error = null;
  }
  const onClick = event => {
    const button = event.target.closest?.('button[data-rf-activate-target]');
    if (!button || button.disabled) return;
    const instance = ids.get(button.getAttribute('data-rf-activate-target'));
    if (!instance || instance.metadata['data-rf-activate'] !== 'interaction') return;
    event.preventDefault();
    // Another deliberate click is an explicit retry; automatic policies never loop.
    try { request(instance.token, instance.state === 'failed' ? 'retry' : 'activate').promise.catch(error => { if (error.code !== 'cancelled') console.error(error); }); }
    catch (error) { state(instance, 'failed', error); }
  };
  const observer = new MutationObserver(records => {
    queueMicrotask(() => {
      // Check connectivity after the complete mutation batch: keyed DOM moves
      // retain behavior, while external removal disposes it.
      for (const instance of [...instances.values()]) if (!valid(instance)) dispose(instance);
      for (const record of records) for (const node of record.addedNodes) if (node.isConnected) discover(node);
    });
  });
  const api = {
    version: VERSION,
    lookup(id, descriptor, schema) {
      const instance = ids.get(id);
      if (!instance) throw failure('unknown-instance', `Unknown island ${id}.`);
      if (instance.entry.descriptor !== descriptor || instance.entry.props_schema !== schema) throw failure('descriptor-mismatch', 'Island handle descriptor/schema mismatch.');
      return target(instance.token).token;
    },
    status(token) { return target(token).state; },
    request,
    prefetch(id) { const instance = ids.get(id); if (!instance) throw failure('unknown-instance', `Unknown island ${id}.`); return request(instance.token, 'prefetch'); },
    activate(id) { const instance = ids.get(id); if (!instance) throw failure('unknown-instance', `Unknown island ${id}.`); return request(instance.token, 'activate'); },
    retry(id) { const instance = ids.get(id); if (!instance) throw failure('unknown-instance', `Unknown island ${id}.`); return request(instance.token, 'retry'); },
    dispose(id) { const instance = ids.get(id); if (instance) dispose(instance); },
    disposeTree(root) { for (const instance of [...instances.values()]) if (root === instance.host || root.contains(instance.host)) dispose(instance); },
    register: discover,
    inspect() { return { units: Object.fromEntries([...units].map(([name, unit]) => [name, unit.state])), instances: [...instances.values()].map(instance => ({ id: instance.id, state: instance.state, waiters: instance.claims.size })) }; },
    destroy() {
      observer.disconnect(); document.removeEventListener('click', onClick);
      document.removeEventListener('compositionstart', onCompositionStart, true); document.removeEventListener('compositionend', onCompositionEnd, true);
      for (const instance of [...instances.values()]) dispose(instance);
      if (globalThis.__fusor_islands === api) delete globalThis.__fusor_islands;
      if (globalThis.__fusor_composing === composing) {
        if (previousComposing === undefined) delete globalThis.__fusor_composing;
        else globalThis.__fusor_composing = previousComposing;
      }
    },
  };
  try {
    globalThis.__fusor_composing = composing;
    document.addEventListener('compositionstart', onCompositionStart, true);
    document.addEventListener('compositionend', onCompositionEnd, true);
    document.addEventListener('click', onClick);
    observer.observe(root, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: [...metadataNames, 'type', 'data-rf-props'] });
    globalThis.__fusor_islands = api;
    discover(root);
    return api;
  } catch (error) {
    api.destroy();
    throw error;
  }
}
