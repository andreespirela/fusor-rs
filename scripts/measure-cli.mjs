// Local dev-loop timings with a prepared toolchain/registry and an empty target.
// Run after cargo build --release -p fusor-cli; see the CLI validation report.
import { readFile, writeFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { performance } from 'node:perf_hooks';
import { chromium } from '@playwright/test';
import { root, exec, startProcess, stopProcess, waitFor, reservePort, temporaryDirectory } from './build.mjs';
const scratch = await temporaryDirectory('fusor-cli-latency-');
const app = join(scratch, 'app');
const binary = join(root, 'target/release', `fusor${process.platform === 'win32' ? '.exe' : ''}`);
const environment = { ...process.env, RUSTUP_TOOLCHAIN: process.env.RUSTUP_TOOLCHAIN || 'stable', CARGO_NET_OFFLINE: 'true', CARGO_TARGET_DIR: join(scratch, 'target') };
const measurements = {};
let server, browser;
const timed = async (name, operation) => { const start = performance.now(); await operation(); measurements[name] = Math.round(performance.now() - start); };
try {
  await exec(binary, ['new', app, '--skip-install', '--framework-path', root], { env: environment });
  await timed('lock_resolution_cached_registry_ms', () => exec('cargo', ['generate-lockfile', '--offline'], { cwd: app, env: environment }));
  browser = await chromium.launch({ channel: process.env.PLAYWRIGHT_CHANNEL || undefined });
  const page = await browser.newPage();
  const start = async () => {
    const port = await reservePort();
    server = startProcess(binary, ['dev', '--offline', '--port', String(port)], { cwd: app, env: environment });
    await waitFor(() => server.output.includes('watching for changes'), 'dev ready', { timeout: 300000, process: server });
    await page.goto(`http://127.0.0.1:${port}`);
    await page.locator('.counter').first().waitFor();
  };
  await timed('empty_target_first_interactive_page_ms', start);
  await stopProcess(server);
  await timed('warm_target_first_interactive_page_ms', start);
  const html = join(app, 'web/index.html');
  const source = await readFile(html, 'utf8');
  await timed('html_refresh_ms', async () => {
    await writeFile(html, source.replace('Rust, inside HTML.', 'CLI latency fixture.'));
    await page.getByRole('heading', { name: 'CLI latency fixture.', exact: true }).waitFor();
  });
  const rust = join(app, 'src/app.rs');
  const original = await readFile(rust, 'utf8');
  const boot = await page.locator('script[type="module"]').getAttribute('src');
  await timed('rust_rebuild_to_page_ms', async () => {
    await writeFile(rust, original.replace('signal(0)', 'signal(7)'));
    await page.waitForFunction(previous => document.querySelector('script[type="module"]')?.getAttribute('src') !== previous, boot);
    await page.locator('.counter').first().waitFor();
  });
  console.log(JSON.stringify({ profile: 'release', rustToolchain: environment.RUSTUP_TOOLCHAIN, ...measurements }, null, 2));
} finally {
  await browser?.close();
  await stopProcess(server);
  await rm(scratch, { recursive: true, force: true });
}
