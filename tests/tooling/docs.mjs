import { createServer } from "node:http";
import { readFile, mkdir } from "node:fs/promises";
import { resolve, extname, sep } from "node:path";
import { chromium, firefox, webkit, expect } from "@playwright/test";
import assert from "node:assert/strict";
import { checkShowcase, checkHighlighting } from "./docs-showcase.mjs";
const root = resolve("dist/docs");
const guides = JSON.parse(
  await readFile("apps/docs/content/pages.json", "utf8"),
);
// Public syntax must remain discoverable when the compiler adds a directive.
const referenceIndex = JSON.parse(await readFile("apps/docs/content/references.json", "utf8"));
const compilerSyntax = (await Promise.all([
  "crates/fusor-build/src/bindings/parse.rs",
  "crates/fusor-build/src/lib.rs",
].map(path => readFile(path, "utf8")))).join("\n");
for (const directive of new Set(compilerSyntax.match(/rust:[a-z]+(?:-[a-z]+)*/g))) {
  if (["rust:each", "rust:row", "rust:app", "rust:mount", "rust:outlet", "rust:attach", "rust:island", "rust:props", "rust:activate", "rust:prefetch", "rust:activate-target"].includes(directive)) continue; // Rejected legacy syntax, retained only in migration diagnostics.
  if (["rust:content", "rust:slot", "rust:async", "rust:await"].includes(directive)) continue; // Runtime compatibility APIs; authoring docs teach Children, Async, and Await.
  assert(referenceIndex.some(reference => reference.token === directive), `missing directive reference: ${directive}`);
}
for (const token of ["hydrate", "hydrate:id", "hydrate:prefetch", "hydrate:target", "If", "Else", "Match", "Case"]) {
  assert(referenceIndex.some(reference => reference.token === token), `missing hydration reference: ${token}`);
}
for (const reference of referenceIndex) {
  const [slug, id] = reference.href.slice("/docs/".length).split("#");
  assert(guides.find(guide => guide.slug === slug)?.sections.some(section => section.id === id), `unknown reference target: ${reference.href}`);
}
const demos = JSON.parse(
  await readFile("apps/docs/content/showcase.json", "utf8"),
);
const server = createServer(async (req, res) => {
  const url = new URL(req.url, "http://localhost");
  if (url.pathname === "/favicon.ico") {
    res.writeHead(204);
    res.end();
    return;
  }
  let relative = url.pathname.replace(/^\/docs\//, "");
  if (!extname(relative)) relative = "index.html";
  const path = resolve(root, relative);
  if (!path.startsWith(root + sep)) {
    res.writeHead(404);
    res.end();
    return;
  }
  try {
    const bytes = await readFile(path);
    res.setHeader(
      "content-type",
      {
        ".css": "text/css",
        ".js": "text/javascript",
        ".wasm": "application/wasm",
        ".svg": "image/svg+xml",
        ".html": "text/html",
      }[extname(path)] || "text/plain",
    );
    res.end(bytes);
  } catch {
    res.writeHead(404);
    res.end();
  }
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
let browser;
try {
  for (const name of (process.env.PLAYWRIGHT_BROWSERS || "chromium").split(
    ",",
  )) {
    browser = await { chromium, firefox, webkit }[name].launch({
      ...(name === "chromium" && process.env.PLAYWRIGHT_CHANNEL
        ? { channel: process.env.PLAYWRIGHT_CHANNEL } : {}),
      // Linux Firefox needs Xvfb (provided by CI) for the real WebGL 2 demos.
      headless: !(name === "firefox" && process.platform === "linux"),
    });
    const context = await browser.newContext({
      viewport: { width: 1440, height: 1050 },
    });
    const page = await context.newPage();
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("console", (message) => {
      if (message.type() === "error") errors.push(message.text());
    });
    const origin = `http://127.0.0.1:${server.address().port}/docs/`;
    if (!process.env.DOCS_SHOWCASE_ONLY) {
    await page.goto(origin);
    await page
      .getByRole("heading", { name: "Build for the web. Write Rust." })
      .waitFor();
    await page.getByRole("button", { name: "Increase count" }).click();
    assert.equal(await page.locator(".counter output").textContent(), "1");
    await page
      .locator(".sidebar")
      .getByRole("link", { name: "Routing and navigation" })
      .click();
    await page
      .getByRole("heading", { name: "Routing and navigation", exact: true })
      .waitFor();
    assert(page.url().endsWith("/routing"));
    await page.goBack();
    await page
      .getByRole("heading", { name: "Build for the web. Write Rust." })
      .waitFor();
    await page
      .getByRole("searchbox", { name: "Search documentation" })
      .fill("async");
    assert(
      await page
        .locator(".sidebar a", { hasText: "Coherent async views" })
        .isVisible(),
    );
    assert(
      !(await page
        .locator(".sidebar a", { hasText: "Installation" })
        .isVisible()),
    );
    await page
      .getByRole("searchbox", { name: "Search documentation" })
      .fill("no-such-topic");
    assert(await page.locator(".search-empty").isVisible());
    await page
      .getByRole("searchbox", { name: "Search documentation" })
      .fill("");
    await page
      .getByRole("searchbox", { name: "Search documentation" })
      .fill("ForEach");
    await expect(
      page
        .locator(".sidebar")
        .getByRole("link", { name: "Lists with ForEach", exact: true }),
    ).toBeVisible();
    await page
      .getByRole("searchbox", { name: "Search documentation" })
      .fill("");
    await page.getByRole("button", { name: "Toggle color theme" }).click();
    const sidebar = page.locator(".sidebar");
    const branch = sidebar.getByRole("button", { name: "Toggle HTML and Rust subpages", exact: true });
    const attributeLink = sidebar.getByRole("link", { name: "Template attribute reference", exact: true });
    await expect(attributeLink).not.toBeVisible();
    await branch.focus();
    await page.keyboard.press("Enter");
    await expect(branch).toHaveAttribute("aria-expanded", "true");
    await expect(attributeLink).toBeVisible();
    await attributeLink.click();
    await expect(page.locator("h1")).toHaveText("Template attribute reference");
    await expect(page.getByRole("navigation", { name: "Breadcrumb" }).getByRole("link", { name: "HTML and Rust", exact: true })).toBeVisible();
    await page.reload();
    await expect(attributeLink).toHaveAttribute("aria-current", "page");
    await branch.click();
    await expect(attributeLink).not.toBeVisible();
    await page.getByRole("searchbox").fill("Children");
    await expect(sidebar.getByRole("link", { name: "Built-in components", exact: true })).toBeVisible();
    await expect(branch).toHaveAttribute("aria-expanded", "true");
    await page.getByRole("searchbox").fill("");
    await page.goto(origin + "async-data#resource");
    await page.locator("#resource .api-references summary").click();
    await page.locator("#resource .api-references").getByRole("link", { name: "browser::resource · three arguments", exact: false }).click();
    await expect(page.locator("h1")).toHaveText("Resource API");
    assert(page.url().endsWith("async-data/resource#resource"));
    await expect(sidebar.getByRole("button", { name: "Toggle Async data loading subpages", exact: true })).toHaveAttribute("aria-expanded", "true");
    await page.goBack();
    await expect(page.locator("h1")).toHaveText("Async data loading");
    for (const guide of guides) {
      await page.goto(origin + guide.slug);
      await expect(page.locator("h1")).toHaveText(guide.title);
      await expect(page.locator(".sidebar").getByRole("link", {
        name: guide.slug === "" ? "Introduction" : guide.title,
        exact: true,
      })).toHaveAttribute("href", `/docs/${guide.slug}`);
      await page.goto(origin + guide.slug);
      await expect(page.locator("h1")).toHaveText(guide.title);
      for (const reference of await page.locator(".api-references a").evaluateAll(nodes => nodes.map(node => node.getAttribute("href")))) {
        const [slug, id] = reference.slice("/docs/".length).split("#");
        assert(guides.find(guide => guide.slug === slug)?.sections.some(section => section.id === id), `broken contextual reference ${reference}`);
      }
      async function checkProse(locator, source = "") {
        const paragraphs = source.split("\n\n").map(text => text.trim()).filter(Boolean);
        assert.deepEqual(await locator.locator(".prose > p").allTextContents(),
          paragraphs.map(text => text.replaceAll("`", "")), "prose text and paragraph boundaries");
        assert.deepEqual(await locator.locator("code.inline-code").allTextContents(),
          [...source.matchAll(/`([^`]+)`/g)].map(match => match[1]), "inline code is literal text");
        assert.equal(await locator.locator("script").count(), 0);
      }
      await checkProse(page.locator(".article > .lead"), guide.lead);
      for (const section of guide.sections) {
        await expect(page.locator(`#${section.id}`)).toHaveCount(1);
        await checkProse(page.locator(`#${section.id} > .body-copy`), section.body);
        await checkProse(page.locator(`#${section.id} > .note`), section.note);
        if (section.source || section.code) {
          const authored = section.source
            ? await readFile(resolve("apps/docs", section.source), "utf8")
            : section.code;
          assert.equal(
            await page.locator(`#${section.id} pre code`).textContent(),
            authored,
          );
        }
        for (const link of section.links || []) {
          const url = new URL(link.href, origin);
          if (url.pathname.startsWith("/docs/source/")) {
            const response = await page.request.get(url.href);
            assert.equal(response.status(), 200, link.href);
            assert((await response.text()).length > 0, link.href);
          } else if (url.pathname.startsWith("/docs/showcase/")) {
            assert(
              demos.some(
                (demo) => url.pathname === `/docs/showcase/${demo.slug}`,
              ),
              `unknown linked demo ${link.href}`,
            );
          } else {
            const target = guides.find(
              (item) => "/docs/" + item.slug === url.pathname,
            );
            assert(target, `unknown linked guide ${link.href}`);
            if (url.hash)
              assert(
                target.sections.some((item) => "#" + item.id === url.hash),
                `unknown linked section ${link.href}`,
              );
          }
        }
      }
    }
    await page.goto(origin);
    await expect(page.locator("h1")).toHaveText(
      "Build for the web. Write Rust.",
    );
    assert(
      await page
        .locator(".site")
        .evaluate((node) => node.classList.contains("dark")),
    );
    await page.reload();
    await page
      .getByRole("heading", { name: "Build for the web. Write Rust." })
      .waitFor();
    assert(
      await page
        .locator(".site")
        .evaluate((node) => node.classList.contains("dark")),
    );
    await page.getByRole("button", { name: "Toggle color theme" }).click();
    await checkHighlighting(page, origin);
    await page.goto(origin + "html-and-rust");
    for (const dark of [false, true]) {
      if ((await page.locator(".site").getAttribute("class")).includes("dark") !== dark)
        await page.getByRole("button", { name: "Toggle color theme" }).click();
      const inlineCode = page.locator("#modules .inline-code").first();
      await expect(inlineCode).toBeVisible();
      const style = await inlineCode.evaluate(node => {
        const css = getComputedStyle(node);
        return { font: css.fontFamily, background: css.backgroundColor, color: css.color };
      });
      assert.match(style.font, /monospace/);
      assert.notEqual(style.background, "rgba(0, 0, 0, 0)");
      assert.notEqual(style.color, style.background);
      if (name === "chromium") {
        await mkdir("target/screenshots", { recursive: true });
        await page.locator("#modules").screenshot({ path: `target/screenshots/docs-prose-${dark ? "dark" : "light"}.png` });
      }
    }
    await page.getByRole("button", { name: "Toggle color theme" }).click();
    await page.setViewportSize({ width: 390, height: 844 });
    for (const guide of guides) {
      await page.goto(origin + guide.slug);
      await expect(page.locator("h1")).toHaveText(guide.title);
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
        `mobile page overflows: ${guide.slug}`);
    }
    await page.setViewportSize({ width: 1440, height: 1050 });
    await page.goto(origin);
    await expect(page.locator("h1")).toHaveText(
      "Build for the web. Write Rust.",
    );
    if (name === "chromium") {
      await mkdir("target/screenshots", { recursive: true });
      await page.screenshot({
        path: "target/screenshots/docs-desktop.png",
        fullPage: true,
      });
    }
    await page.goto(origin + "coherent-async");
    await page
      .getByRole("heading", { name: "Coherent async views", exact: true })
      .waitFor();
    await page.setViewportSize({ width: 1160, height: 844 });
    await expect(page.locator(".mobile-toc")).toBeVisible();
    await expect(page.locator(".toc")).not.toBeVisible();
    await page.setViewportSize({ width: 390, height: 844 });
    await expect(page.locator(".mobile-toc")).toBeVisible();
    await page.locator(".mobile-toc summary").click();
    await page
      .locator(".mobile-toc")
      .getByRole("link", { name: "Let each child declare its own read" })
      .click();
    assert(page.url().endsWith("#read-declaration"));
    await page.getByRole("button", { name: "Toggle navigation" }).click();
    await page
      .locator(".sidebar")
      .getByRole("link", { name: "Introduction" })
      .click();
    await page
      .getByRole("heading", { name: "Build for the web. Write Rust." })
      .waitFor();
    assert(!(await page.locator(".sidebar").isVisible()));
    assert(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    );
    if (name === "chromium")
      await page.screenshot({
        path: "target/screenshots/docs-mobile.png",
        fullPage: true,
      });
    if (name === "chromium") {
      await page.goto(origin + "components");
      await expect(page.locator("h1")).toHaveText("Reusable HTML");
      await page.screenshot({
        path: "target/screenshots/docs-components-mobile.png",
        fullPage: true,
      });
      await page.setViewportSize({ width: 1440, height: 1050 });
      await page.screenshot({
        path: "target/screenshots/docs-components-desktop.png",
        fullPage: true,
      });
    }
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto(origin + "ownership/mounting");
    await expect(page.locator("h1")).toHaveText("Owners, constructors, and Rust mounting");
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.getByRole("button", { name: "Toggle navigation" }).click();
    await expect(sidebar.getByRole("link", { name: "Owners, constructors, and Rust mounting", exact: true })).toBeVisible();
    await sidebar.getByRole("link", { name: "Owners and cleanup", exact: true }).click();
    await expect(page.locator("h1")).toHaveText("Owners and cleanup");
    await expect(sidebar).not.toBeVisible();
    await page.setViewportSize({ width: 1440, height: 1050 });
    await page.goto(origin + "missing");
    await page.getByRole("heading", { name: "Page not found" }).waitFor();
    }
    await checkShowcase(page, origin, name, demos);
    assert.deepEqual(errors, []);
    console.log(
      `PASS ${name}: ${guides.length} guides, exact prose/code and mobile layout on every page, highlighted source in both themes, ${demos.length} live showcases, related links, search, history, async states, keyed identity, timer cleanup, responsive navigation and missing routes`,
    );
    await context.close();
    await browser.close();
    browser = null;
  }
} finally {
  await browser?.close();
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
}
