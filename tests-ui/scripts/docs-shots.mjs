// README screenshots from mock data, in English (README.md) and Bahasa Indonesia (README.id.md, "-id").
// Run: node tests-ui/serve.mjs & node tests-ui/scripts/docs-shots.mjs
import { chromium } from '@playwright/test';
import { fileURLToPath } from 'node:url';
const out = (f) => fileURLToPath(new URL('../../docs/assets/' + f, import.meta.url));
const b = await chromium.launch({ channel: 'msedge' });
for (const [lang, suffix, add] of [['en', '', 'Add account'], ['id', '-id', 'Tambah akun']]) {
  for (const scheme of ['light', 'dark']) {
    const p = await b.newPage({ viewport: { width: 320, height: 560 }, deviceScaleFactor: 2, colorScheme: scheme });
    await p.goto(`http://127.0.0.1:4317/index.html?mock=1&lang=${lang}`); await p.waitForSelector('.account');
    await p.screenshot({ path: out(`panel-${scheme}${suffix}.png`), fullPage: true });
    if (scheme === 'light') {
      await p.getByRole('button', { name: add }).click();
      await p.waitForTimeout(200);
      await p.screenshot({ path: out(`panel-add${suffix}.png`), fullPage: true });
    }
    await p.close();
  }
}
await b.close();
