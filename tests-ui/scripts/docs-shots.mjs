// README screenshots from mock data. Run: node tests-ui/serve.mjs & node tests-ui/scripts/docs-shots.mjs
import { chromium } from '@playwright/test';
import { fileURLToPath } from 'node:url';
const out = (f) => fileURLToPath(new URL('../../docs/assets/' + f, import.meta.url));
const b = await chromium.launch({ channel: 'msedge' });
for (const scheme of ['light', 'dark']) {
  const p = await b.newPage({ viewport: { width: 320, height: 560 }, deviceScaleFactor: 2, colorScheme: scheme });
  await p.goto('http://127.0.0.1:4317/index.html?mock=1'); await p.waitForSelector('.account');
  await p.screenshot({ path: out(`panel-${scheme}.png`), fullPage: true });
  if (scheme === 'light') {
    await p.getByRole('button', { name: 'Tambah akun' }).click();
    await p.waitForTimeout(200);
    await p.screenshot({ path: out('panel-add.png'), fullPage: true });
  }
}
await b.close();
