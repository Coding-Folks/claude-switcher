// Renders the brand SVG to PNGs (app icon source, README banner, social preview) with installed Edge.
// Run: node tests-ui/scripts/render-brand.mjs  (uses tests-ui's @playwright/test and installed Edge)
// Fonts: the Coding Folks brand fonts bundled with the panel (ui/fonts, OFL).
import { chromium } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join, dirname } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, '..', '..');
const mark = readFileSync(join(repo, 'assets/brand/logo-mark.svg'), 'utf8');
const font = (f) => readFileSync(join(repo, 'ui/fonts', f)).toString('base64');
const fontCss =
  `@font-face{font-family:'Instrument Sans';src:url(data:font/woff2;base64,${font('instrument-sans-latin-wght-normal.woff2')}) format('woff2');font-weight:400 700;}` +
  `@font-face{font-family:'IBM Plex Mono';src:url(data:font/woff2;base64,${font('ibm-plex-mono-latin-500-normal.woff2')}) format('woff2');font-weight:500;}`;

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

// Social preview in the style of the Coding Folks og-image (landing-page/public/og-image.png):
// brand-blue field with rings, the company logo top left, a bold title with the orange full stop,
// and the address in IBM Plex Mono.
const cf = readFileSync(join(repo, 'assets/brand/coding-folks-horizontal-white.svg'), 'utf8')
  .replace(/<\?xml[^>]*>/, '')
  .replace('<svg ', '<svg width="176" height="60" ');
const lockup = (bg, rings, sub) => `
<div style="position:relative;overflow:hidden;width:1280px;height:640px;background:${bg};font-family:'Instrument Sans',sans-serif;color:#FFFFFF">
  <div style="position:absolute;right:-220px;top:-170px;width:980px;height:980px;border-radius:50%;border:2px solid ${rings}"></div>
  <div style="position:absolute;right:30px;top:80px;width:480px;height:480px;border-radius:50%;border:2px solid ${rings}"></div>
  <div style="position:absolute;right:80px;top:160px;width:320px;height:320px">${mark.replace('<svg ', '<svg width="320" height="320" ')}</div>
  <div style="position:absolute;left:88px;top:72px">${cf}</div>
  <div style="position:absolute;left:88px;top:196px;width:760px">
    <div style="font-size:100px;font-weight:652;letter-spacing:-0.035em;line-height:1;white-space:nowrap">Claude Switcher<span style="color:#F48200">.</span></div>
    <div style="font-size:34px;font-weight:456;line-height:1.35;margin-top:28px;width:620px;color:${sub}">Several Claude Desktop accounts, side by side, on Windows.</div>
    <div style="display:inline-block;margin-top:44px;padding:14px 22px;border:2px solid rgb(255 255 255 / 0.7);font-family:'IBM Plex Mono',monospace;font-weight:500;font-size:26px">github.com/Coding-Folks/claude-switcher</div>
  </div>
</div>`;
await shot(lockup('linear-gradient(120deg, #3D88FF 0%, #0D6AFF 45%, #0C60E8 100%)', 'rgb(255 255 255 / 0.18)', '#E7F0FF'), 1280, 640, join(repo, 'assets/brand/social-preview.png'));
await shot(lockup('#0D1117', 'rgb(93 155 255 / 0.22)', '#BBBEC5'), 1280, 640, join(repo, 'assets/brand/social-preview-dark.png'));
await b.close();
console.log('rendered');
