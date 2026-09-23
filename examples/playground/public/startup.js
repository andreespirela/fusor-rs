// The CLI owns module loading; the playground presents its startup error event.
document.addEventListener("fusor:error", () => {
  const status = document.querySelector("#runtime-state");
  if (status) status.textContent = "Unable to start WebAssembly";
  const message = document.querySelector("#load-error");
  if (message) {
    message.hidden = false;
    message.textContent = "The Rust module could not load. Run cargo fusor build -p fusor-playground and serve the dist directory over HTTP.";
  }
});
