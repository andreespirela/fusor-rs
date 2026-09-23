const review = globalThis.__bridgeReview ??= { mounts: 0, values: [], secondary: [], cleanup: [], aborted: 0 };

const mode = new URLSearchParams(location.search).get('mode');
export const onMount = mode === 'async' ? async () => {
  review.startedAsync = true;
} : mode === 'bad-export' ? 42 : mount;

function mount({root, signal, inputs, onCleanup}) {
  review.inputShape = {
    nullPrototype: Object.getPrototypeOf(inputs) === null,
    frozen: Object.isFrozen(inputs),
    fields: Object.keys(inputs).sort(),
    proto: inputs.__proto__.get(),
    constructor: inputs.constructor.get(),
  };
  review.mounts++;
  const id = review.mounts;
  signal.addEventListener('abort', () => review.aborted++, {once:true});
  onCleanup(() => review.cleanup.push(`${id}:first:${root.isConnected}`));
  if (mode === 'failure') onCleanup(() => {
    review.cleanup.push(`${id}:throws:${root.isConnected}`);
    throw new Error('EXPECTED cleanup failure');
  });
  onCleanup(() => review.cleanup.push(`${id}:second:${root.isConnected}`));
  inputs.value.subscribe(value => {
    review.values.push(value);
    const detail = value === 2 ? 'rewrite' : value === 4 ? 'dispose' : 'read';
    root.dispatchEvent(new CustomEvent('probe-update', {detail}));
  });
  if (!signal.aborted) inputs.value.subscribe(value => review.secondary.push(value));
  if (mode === 'failure') throw new Error('EXPECTED setup failure');
  if (mode === 'promise') return Promise.resolve(() => {
    review.lostPromisedCleanup = true;
  });
  return () => review.cleanup.push(`${id}:returned:${root.isConnected}`);
}
