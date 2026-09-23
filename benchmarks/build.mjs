import { build } from "vite";
import react from "@vitejs/plugin-react";
import vue from "@vitejs/plugin-vue";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import solid from "vite-plugin-solid";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
const root = fileURLToPath(new URL(".", import.meta.url));
function options(framework, ssr = true) {
  return {
    configFile: false,
    root: resolve(root, "workloads", framework),
    base: `/workloads/${framework}/`,
    plugins:
      framework === "react"
        ? [react()]
        : framework === "vue"
          ? [vue()]
          : framework === "svelte"
            ? [svelte()]
            : framework === "solid"
              ? [solid({ ssr })]
              : [],
    oxc:
      framework === "preact"
        ? { jsx: { runtime: "automatic", importSource: "preact" } }
        : undefined,
    logLevel: "warn",
  };
}
for (const framework of ["react", "svelte", "solid", "vue", "preact"]) {
  await build({
    ...options(framework),
    build: {
      outDir: resolve(root, "dist", framework),
      emptyOutDir: true,
      minify: true,
      sourcemap: false,
    },
  });
  const extension = ["react", "solid", "preact"].includes(framework)
    ? "jsx"
    : "js";
  await build({
    ...options(framework),
    build: {
      ssr: `src/ssr.${extension}`,
      outDir: resolve(root, "dist/server", framework),
      emptyOutDir: true,
      minify: true,
      rolldownOptions: { output: { entryFileNames: "render.js" } },
    },
  });
  for (const fixture of ["hello", "todo"])
    await build({
      ...options(framework, false),
      root: resolve(root, "workloads", framework, fixture),
      base: `/workloads/${framework}/${fixture}/`,
      build: {
        outDir: resolve(root, "dist", framework, fixture),
        emptyOutDir: true,
        minify: true,
        sourcemap: false,
      },
    });
  console.log("Built production browser + SSR + bundles", framework);
}
