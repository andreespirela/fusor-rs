// Read the actual compiled unit's typed witness without importing browser JS
// or running the wasm-bindgen start export. This ABI follows the pinned binder.
import { readFile } from 'node:fs/promises';
const module = await WebAssembly.compile(await readFile(process.argv[1]));
const imports = Object.create(null);
for (const entry of WebAssembly.Module.imports(module)) {
  if (entry.kind !== 'function') throw Error(`Unsupported metadata import ${entry.module}.${entry.name}: ${entry.kind}`);
  imports[entry.module] ??= Object.create(null);
  imports[entry.module][entry.name] = () => { throw Error(`Island registration executed browser code: ${entry.module}.${entry.name}`); };
}
const { exports } = await WebAssembly.instantiate(module, imports);
if (typeof exports.__rf_manifest !== 'function' || !(exports.memory instanceof WebAssembly.Memory)) throw Error('Unit does not export the fusor registration protocol.');
const result = exports.__rf_manifest();
if (!Array.isArray(result) || result.length !== 2) throw Error('Unsupported wasm-bindgen metadata ABI; rebuild with the pinned toolchain.');
const [pointer, length] = result;
const witness = new TextDecoder('utf-8', { fatal: true }).decode(new Uint8Array(exports.memory.buffer, pointer, length));
JSON.parse(witness);
process.stdout.write(witness);
