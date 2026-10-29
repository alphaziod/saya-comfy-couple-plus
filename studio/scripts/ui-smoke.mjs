// Optional test dependency; not used by the application or npm ci.
import { createRequire } from 'node:module';
import { mkdir } from 'node:fs/promises';
import assert from 'node:assert/strict';
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.SAYA_PLAYWRIGHT_MODULE || 'playwright');
const browser = await chromium.launch({ headless: true, executablePath: process.env.SAYA_CHROMIUM || undefined });
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 960 } });
  const errors = [], externalRequests = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => {
    if (!request.url().startsWith('http://127.0.0.1:1420/')) externalRequests.push(request.url());
  });
  await page.goto('http://127.0.0.1:1420');
  await page.getByText('Aperçu navigateur : ouvrir le binaire Saya Studio pour lire les données locales.').waitFor();
  for (const name of ['Personnages', 'ComfyUI', 'Workflows', 'Galerie']) {
    await page.locator('nav').getByRole('button', { name, exact: true }).click();
    await page.getByRole('heading', { name, exact: true }).waitFor();
  }
  assert.equal(await page.getByRole('button', { name: 'Indexer / actualiser' }).isDisabled(), true);
  await mkdir('test-results', { recursive: true });
  await page.screenshot({ path: 'test-results/interface-browser.png', fullPage: true });
  await page.setViewportSize({ width: 800, height: 650 });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  assert.deepEqual(errors, []);
  assert.deepEqual(externalRequests, []);
  console.log('PASS: 4 module views, browser disclosure, no page errors, no external requests, 800px layout.');
} finally {
  await browser.close();
}
