// Renders the brand SVG to PNGs (app icon source, README banner, social preview) with installed Edge.
// Run: node tests-ui/scripts/render-brand.mjs  (uses tests-ui's @playwright/test and installed Edge)
// Optional: put instrument-sans-latin-wght-normal.woff2 (OFL) next to this script for the brand font.
import { chromium } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join, dirname } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, '..', '..');
const mark = readFileSync(join(repo, 'assets/brand/logo-mark.svg'), 'utf8');
let fontCss = '';
try {
  const font = readFileSync(join(here, 'instrument-sans-latin-wght-normal.woff2')).toString('base64');
  fontCss = `@font-face{font-family:'Instrument Sans';src:url(data:font/woff2;base64,${font}) format('woff2');font-weight:400 700;}`;
} catch { /* falls back to the system sans-serif */ }

const b = await chromium.launch({ channel: 'msedge' });
async function shot(html, w, h, out, transparent = false) {
  const p = await b.newPage({ viewport: { width: w, height: h }, deviceScaleFactor: 1 });
  await p.setContent(`<!doctype html><style>${fontCss}html,body{margin:0}</style>${html}`);
  await p.evaluate(() => document.fonts.ready);
  await p.screenshot({ path: out, omitBackground: transparent });
  await p.close();
}

// 1024x1024 transparent app icon source
await shot(`<div style="width:1024px;height:1024px">${mark.replace('<svg ', '<svg width="1024" height="1024" ')}</div>`, 1024, 1024, join(repo, 'assets/brand/icon-1024.png'), true);

const lockup = (bg, ink, sub) => `
<div style="width:1280px;height:640px;background:${bg};display:flex;align-items:center;gap:56px;padding:0 112px;box-sizing:border-box;font-family:'Instrument Sans',sans-serif">
  <div style="width:280px;height:280px;flex:none">${mark.replace('<svg ', '<svg width="280" height="280" ')}</div>
  <div>
    <div style="font-size:96px;font-weight:700;letter-spacing:-0.03em;color:${ink};line-height:1">Claude Switcher</div>
    <div style="font-size:32px;color:${sub};margin-top:24px;white-space:nowrap">Several Claude Desktop accounts, side by side.</div>
    <div style="font-size:26px;color:${sub};margin-top:36px">by <b style="color:#EB7D00">Coding Folks</b> · unofficial · open source</div>
  </div>
</div>`;
await shot(lockup('#FFFFFF', '#12151C', '#404C64'), 1280, 640, join(repo, 'assets/brand/social-preview.png'));
await shot(lockup('#12151C', '#FFFFFF', '#CDD3DF'), 1280, 640, join(repo, 'assets/brand/social-preview-dark.png'));
await b.close();
console.log('rendered');
