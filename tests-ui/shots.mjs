import { chromium } from '@playwright/test';
const b = await chromium.launch({ channel: 'msedge' });
for (const scheme of ['light', 'dark']) {
  const p = await b.newPage({ viewport: { width: 320, height: 520 }, deviceScaleFactor: 2, colorScheme: scheme });
  await p.goto('http://127.0.0.1:4317/index.html?mock=1'); await p.waitForSelector('.account');
  await p.screenshot({ path: `../.taste-skill/panel-${scheme}.png`, fullPage: true });
  if (scheme === 'light') {
    await p.getByRole('button', { name: 'Opsi untuk Kantor' }).click();
    await p.screenshot({ path: '../.taste-skill/panel-menu.png', fullPage: true });
  }
}
await b.close();
