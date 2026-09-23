import { isLowerCase } from 'is-lower-case';
import { Mutex } from 'async-mutex';
import '@shoelace-style/shoelace/dist/components/button/button.js';
import './style.css';

/** @param {import('../.fusor/types/web-index-html-App').MountContext} context */
export function onMount({ root, signal }) {
  const report = detail => root.dispatchEvent(new CustomEvent('utility-result', { detail }));
  report(`isLowerCase("rust"): ${isLowerCase('rust')}`);
  root.querySelector('#promise').addEventListener('click', async () => {
    const mutex = new Mutex();
    await mutex.waitForUnlock();
    if (!signal.aborted) report(`Promise resolved; mutex locked: ${mutex.isLocked()}`);
  }, { signal });
  root.querySelector('#register').addEventListener('click', async () => {
    await import('@shoelace-style/shoelace/dist/components/animation/animation.js');
    if (signal.aborted) return;
    report('Shoelace registered; queued keyframes applied');
    root.dispatchEvent(new CustomEvent('animation-ready'));
  }, { signal });
}
