import { createEditor } from "../js/widgets.js";

export function onMount({ root, inputs, signal, onCleanup }) {
  let active = true;
  const editor = createEditor(root, inputs.text.get(), value => {
    if (active && !signal.aborted) {
      root.dispatchEvent(new CustomEvent("editor-change", { detail: value }));
    }
  });
  const dispose = () => {
    if (!active) return;
    active = false;
    editor.destroy();
  };
  onCleanup(dispose);
  // A deliberate partial-setup failure exercises cleanup and component retry.
  if (inputs.fail.get()) {
    dispose();
    throw new Error("expected widget setup failure");
  }
  onCleanup(inputs.text.subscribe(value => editor.update(value)));
}
