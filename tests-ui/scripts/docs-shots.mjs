// Screenshots for the README (mock data). From tests-ui: node serve.mjs & node ../scripts/docs-shots.mjs
import { chromium } from '@playwright/test';
const b = await chromium.launch({ channel: 'msedge' });
for (const scheme of ['light', 'dark']) {
  const p = await b.newPage({ viewport: { width: 320, height: 560 }, deviceScaleFactor: 2, colorScheme: scheme });
  await p.goto('http://127.0.0.1:4317/index.html?mock=1'); await p.waitForSelector('.account');
  await p.screenshot({ path: `../docs/assets/panel-${scheme}.png`, fullPage: true });
  if (scheme === 'light') {
    await p.getByRole('button', { name: 'Tambah akun' }).click();
    await p.waitForTimeout(200);
    await p.screenshot({ path: '../docs/assets/panel-add.png', fullPage: true });
  }
}
await b.close();
