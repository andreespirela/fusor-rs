globalThis.__emptyModuleLoads = (globalThis.__emptyModuleLoads ?? 0) + 1;
globalThis.__emptyMounts ??= [];

export function onMount({root, inputs}) {
  globalThis.__emptyMounts.push({id:root.id, fields:Object.keys(inputs)});
}
