// Emit before editable server HTML. Only records ongoing IME sessions; it never
// intercepts input or submits/replays an interaction.
globalThis.__fusor_composing ??= new WeakSet();
document.addEventListener('compositionstart', event => globalThis.__fusor_composing.add(event.target), true);
document.addEventListener('compositionend', event => globalThis.__fusor_composing.delete(event.target), true);
