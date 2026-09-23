// The harness measures from a requested operation through the framework's DOM
// commit barrier. Rendering work never happens in this shared test driver.
export async function install(api) {
  if(typeof document === 'undefined')return;
  globalThis.__bench = api;
  const query = new URLSearchParams(location.search);
  if (query.has('startup')) {
    await api.mount(Number(query.get('startup')), 'rows');
    await api.flush();
  }
  globalThis.__benchReady = performance.now();
}
