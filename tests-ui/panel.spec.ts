import { test, expect, Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

const PANEL = '/index.html?mock=1';
const AA = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'];

async function ready(page: Page, query = '') {
  await page.goto(PANEL + query);
  await expect(page.locator('.account')).toHaveCount(5);
}
const row = (page: Page, label: string) => page.locator('.account', { has: page.locator('.name', { hasText: label }) });

for (const scheme of ['light', 'dark'] as const) {
  test(`axe AA ${scheme}`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await ready(page);
    const r = await new AxeBuilder({ page }).withTags(AA).analyze();
    expect(r.violations).toEqual([]);
  });
}

test('axe forced-colors', async ({ page }) => {
  await page.emulateMedia({ forcedColors: 'active' });
  await ready(page);
  const r = await new AxeBuilder({ page }).withTags(AA).analyze();
  expect(r.violations).toEqual([]);
});

test('axe with menu and dialog open', async ({ page }) => {
  await ready(page);
  await row(page, 'Kantor').getByRole('button', { name: 'Opsi untuk Kantor' }).click();
  expect((await new AxeBuilder({ page }).withTags(AA).analyze()).violations).toEqual([]);
  await page.getByRole('menuitem', { name: 'Hapus' }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
  expect((await new AxeBuilder({ page }).withTags(AA).analyze()).violations).toEqual([]);
});

test('first focus is the first account action', async ({ page }) => {
  await ready(page);
  await expect(page.getByRole('button', { name: 'Tampilkan Utama' })).toBeFocused();
});

test('keyboard: arrow moves between rows, Enter opens', async ({ page }) => {
  await ready(page);
  await page.keyboard.press('ArrowDown');
  await expect(page.getByRole('button', { name: 'Tampilkan Kantor' })).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await expect(page.getByRole('button', { name: 'Buka Klien' })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(row(page, 'Klien')).toContainText('Berjalan');
  await expect(page.locator('#live')).toHaveText('Klien dibuka.');
});

test('Tab past the last control wraps to the first, Shift+Tab wraps back (focus never leaves the panel)', async ({ page }) => {
  await ready(page);
  await page.locator('#autostart').focus();
  await page.keyboard.press('Tab');
  await expect(page.locator('#updates')).toBeFocused(); // the update-checks toggle
  await page.keyboard.press('Tab');
  await expect(page.locator('#lang')).toBeFocused(); // the language picker is the last control
  await page.keyboard.press('Tab');
  await expect(page.getByRole('button', { name: 'Tampilkan Utama' })).toBeFocused();
  await page.keyboard.press('Shift+Tab');
  await expect(page.locator('#lang')).toBeFocused();
});

test('the keyboard shortcut that opens the panel is stated in it', async ({ page }) => {
  await ready(page);
  await expect(page.locator('.foot')).toContainText('Buka panel: Ctrl+Alt+K');
});

test('escape closes menu then returns focus to its button', async ({ page }) => {
  await ready(page);
  const kebab = row(page, 'Kantor').getByRole('button', { name: 'Opsi untuk Kantor' });
  await kebab.focus();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('menu')).toBeVisible();
  await expect(page.getByRole('menuitem').first()).toBeFocused();
  await expect(kebab).toHaveAttribute('aria-expanded', 'true');
  await page.keyboard.press('ArrowDown');
  await expect(page.getByRole('menuitem', { name: 'Ganti nama' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('menu')).toBeHidden();
  await expect(kebab).toBeFocused();
  await expect(kebab).toHaveAttribute('aria-expanded', 'false');
});

test('import history asks first, then announces', async ({ page }) => {
  await ready(page);
  await row(page, 'Kantor').getByRole('button', { name: 'Opsi untuk Kantor' }).click();
  await page.getByRole('menuitem', { name: 'Impor riwayat' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('Impor riwayat ke Kantor?');
  await expect(dialog).toContainText('ditutup');
  await expect(dialog.getByRole('button', { name: 'Batal' })).toBeFocused();
  await dialog.getByRole('button', { name: 'Impor' }).click();
  await expect(page.locator('#live')).toHaveText('Riwayat diimpor ke Kantor. Buka lagi akunnya untuk melihatnya.');
});

test('import before signing in explains what to do', async ({ page }) => {
  await ready(page, '&fail=import');
  await row(page, 'Klien').getByRole('button', { name: 'Opsi untuk Klien' }).click();
  await page.getByRole('menuitem', { name: 'Impor riwayat' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Impor' }).click();
  await expect(page.locator('#alert')).toHaveText('Login dulu ke Claude di akun ini, lalu impor riwayat lagi.');
});

test('main account has no remove option', async ({ page }) => {
  await ready(page);
  await row(page, 'Utama').getByRole('button', { name: 'Opsi untuk Utama' }).click();
  await expect(page.getByRole('menuitem', { name: 'Hapus' })).toHaveCount(0);
});

test('add account announces progress in live region', async ({ page }) => {
  await ready(page);
  await page.getByRole('button', { name: 'Tambah akun' }).click();
  await expect(page.getByLabel('Nama akun')).toBeFocused();
  await page.getByRole('radio', { name: 'User Windows terpisah' }).check();
  await page.getByLabel('Nama akun').fill('Tim Desain');
  const seen: string[] = [];
  await page.exposeFunction('__seen', (s: string) => seen.push(s));
  await page.evaluate(() => new MutationObserver(() => (window as any).__seen(document.getElementById('live')!.textContent)).observe(document.getElementById('live')!, { childList: true, characterData: true, subtree: true }));
  await page.getByRole('button', { name: 'Buat akun' }).click();
  await expect(page.locator('#live')).toHaveText('Akun Tim Desain siap. Login di jendela Claude yang terbuka.');
  expect(seen.some((s) => /Membuat user Windows/.test(s))).toBe(true);
  await expect(page.locator('.account')).toHaveCount(6);
  await expect(page.getByRole('button', { name: 'Tambah akun' })).toBeFocused();
});

test('add form offers both modes, "User Windows ini" selected by default', async ({ page }) => {
  await ready(page);
  await page.getByRole('button', { name: 'Tambah akun' }).click();
  const group = page.getByRole('group', { name: 'Jalankan di' });
  await expect(group.getByRole('radio', { name: 'User Windows ini' })).toBeChecked();
  await expect(group.getByRole('radio', { name: 'User Windows terpisah' })).not.toBeChecked();
  expect((await new AxeBuilder({ page }).withTags(AA).analyze()).violations).toEqual([]);
});

test('arrow keys switch mode and the hint follows', async ({ page }) => {
  await ready(page);
  await page.getByRole('button', { name: 'Tambah akun' }).click();
  const same = page.getByRole('radio', { name: 'User Windows ini' });
  await same.focus();
  await expect(same).toHaveAccessibleDescription(/tanpa izin admin/);
  await page.keyboard.press('ArrowDown');
  const sep = page.getByRole('radio', { name: 'User Windows terpisah' });
  await expect(sep).toBeChecked();
  await expect(sep).toHaveAccessibleDescription(/Perlu izin admin/);
});

test('same-user add never mentions the admin prompt', async ({ page }) => {
  await ready(page);
  const seen: string[] = [];
  await page.exposeFunction('__seen2', (s: string) => seen.push(s));
  await page.evaluate(() => new MutationObserver(() => (window as any).__seen2(document.getElementById('live')!.textContent)).observe(document.getElementById('live')!, { childList: true, characterData: true, subtree: true }));
  await page.getByRole('button', { name: 'Tambah akun' }).click();
  await expect(page.locator('#add-hint')).not.toContainText(/admin/);
  await page.getByLabel('Nama akun').fill('Slot A');
  await page.getByRole('button', { name: 'Buat akun' }).click();
  await expect(page.locator('#live')).toHaveText('Akun Slot A siap. Login di jendela Claude yang terbuka.');
  expect(seen.some((s) => /Menyiapkan folder akun/.test(s))).toBe(true);
  expect(seen.filter((s) => /admin|UAC/.test(s))).toEqual([]);
});

test('separate-user add still asks for the admin prompt', async ({ page }) => {
  await ready(page);
  const seen: string[] = [];
  await page.exposeFunction('__seen3', (s: string) => seen.push(s));
  await page.evaluate(() => new MutationObserver(() => (window as any).__seen3(document.getElementById('live')!.textContent)).observe(document.getElementById('live')!, { childList: true, characterData: true, subtree: true }));
  await page.getByRole('button', { name: 'Tambah akun' }).click();
  await page.getByRole('radio', { name: 'User Windows terpisah' }).check();
  await page.getByLabel('Nama akun').fill('Kantor 2');
  await page.getByRole('button', { name: 'Buat akun' }).click();
  await expect(page.locator('#live')).toHaveText('Akun Kantor 2 siap. Login di jendela Claude yang terbuka.');
  expect(seen.some((s) => /setujui prompt admin/.test(s))).toBe(true);
});

test('rows say which kind they are', async ({ page }) => {
  await ready(page);
  await expect(row(page, 'Kantor')).toContainText('User terpisah');
  await expect(row(page, 'Slot B')).toContainText('User ini');
  await expect(row(page, 'Utama')).not.toContainText('User ');
  await expect(row(page, 'Slot B').getByRole('button', { name: 'Buka Slot B' })).toHaveAccessibleDescription(/User ini/);
});

test('import on a same-user account does not mention admin', async ({ page }) => {
  await ready(page);
  await row(page, 'Slot B').getByRole('button', { name: 'Opsi untuk Slot B' }).click();
  await page.getByRole('menuitem', { name: 'Impor riwayat' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('Impor riwayat ke Slot B?');
  await expect(dialog).not.toContainText(/admin/);
});

test('removing a same-user account says its login and data go', async ({ page }) => {
  await ready(page);
  await row(page, 'Slot B').getByRole('button', { name: 'Opsi untuk Slot B' }).click();
  await page.getByRole('menuitem', { name: 'Hapus' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('Login dan data Claude di akun ini ikut terhapus. Akun Utama tidak terpengaruh.');
  await expect(dialog).not.toContainText('User Windows');
});

test('an unsupported slot explains itself and points to the separate-user mode', async ({ page }) => {
  await ready(page, '&slot=unsupported');
  await row(page, 'Slot B').getByRole('button', { name: 'Perbaiki Slot B' }).click();
  await expect(page.locator('#alert')).toHaveText('Versi Claude ini tidak lagi memakai folder akun ini. Hapus akun ini, lalu tambahkan lagi dengan pilihan User Windows terpisah.');
  await expect(row(page, 'Slot B')).toContainText('Bermasalah');
});

test('opening a slot whose folder was deleted says so', async ({ page }) => {
  await ready(page, '&slot=missing');
  await page.getByRole('button', { name: 'Buka Slot B' }).click();
  await expect(page.locator('#live')).toContainText('dibuat ulang');
});

test('with many accounts the open add form shows its submit button', async ({ page }) => {
  await ready(page);
  await page.getByRole('button', { name: 'Tambah akun' }).click();
  await expect(page.getByRole('button', { name: 'Buat akun' })).toBeInViewport();
  await expect(page.getByRole('group', { name: 'Jalankan di' })).toBeInViewport();
});

test('each row keeps its options button beside the main button', async ({ page }) => {
  await page.setViewportSize({ width: 303, height: 480 }); // 320px window minus a classic Windows scrollbar
  await ready(page);
  for (const r of await page.locator('.account').all()) {
    const main = (await r.locator('[data-role="primary"]').boundingBox())!;
    const kebab = (await r.locator('[data-role="menu"]').boundingBox())!;
    expect(Math.abs(main.y + main.height / 2 - (kebab.y + kebab.height / 2)), await r.locator('.name').innerText()).toBeLessThan(6);
  }
});

test('the tray menu "Tambah akun…" opens the add form directly', async ({ page }) => {
  await ready(page);
  await page.evaluate(() => (window as any).__MOCK__.emit('open-add', null));
  await expect(page.getByLabel('Nama akun')).toBeFocused();
  await expect(page.getByRole('button', { name: 'Buat akun' })).toBeInViewport();
});

test('add with empty name explains the field', async ({ page }) => {
  await ready(page);
  await page.getByRole('button', { name: 'Tambah akun' }).click();
  await page.getByRole('button', { name: 'Buat akun' }).click();
  await expect(page.locator('#add-error')).toHaveText('Isi nama akun dulu.');
  await expect(page.getByLabel('Nama akun')).toBeFocused();
});

test('add failure shows role=alert and keeps form values', async ({ page }) => {
  await ready(page, '&fail=add');
  await page.getByRole('button', { name: 'Tambah akun' }).click();
  await page.getByRole('radio', { name: 'User Windows terpisah' }).check();
  await page.getByLabel('Nama akun').fill('Kantor 2');
  await page.getByRole('button', { name: 'Buat akun' }).click();
  await expect(page.getByRole('alert').filter({ hasText: 'Dibatalkan' })).toHaveText('Dibatalkan. Tidak ada yang berubah.');
  await expect(page.getByLabel('Nama akun')).toHaveValue('Kantor 2');
  await expect(page.locator('.account')).toHaveCount(5);
});

test('remove asks confirmation naming the account, cancel is focused first', async ({ page }) => {
  await ready(page);
  await row(page, 'Kantor').getByRole('button', { name: 'Opsi untuk Kantor' }).click();
  await page.getByRole('menuitem', { name: 'Hapus' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('Hapus akun Kantor?');
  await expect(dialog).toContainText('claude-kantor');
  await expect(dialog.getByRole('button', { name: 'Batal' })).toBeFocused();
  await dialog.getByRole('button', { name: 'Hapus akun' }).click();
  await expect(page.locator('.account')).toHaveCount(4);
  await expect(page.locator('#live')).toHaveText('Akun Kantor dihapus.');
});

test('low RAM asks before opening', async ({ page }) => {
  await ready(page, '&ram=low');
  await page.getByRole('button', { name: 'Buka Klien' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('RAM bebas tinggal 1,5 GB');
  await dialog.getByRole('button', { name: 'Tetap buka' }).click();
  await expect(row(page, 'Klien')).toContainText('Berjalan');
});

test('status is text, not color only', async ({ page }) => {
  await ready(page);
  await expect(row(page, 'Utama')).toContainText('Berjalan · 1,4 GB');
  await expect(row(page, 'Klien')).toContainText('Mati');
  await expect(row(page, 'Riset')).toContainText('Bermasalah');
});

test('a problem account offers Perbaiki as its main action', async ({ page }) => {
  await ready(page);
  await row(page, 'Riset').getByRole('button', { name: 'Perbaiki Riset' }).click();
  await expect(page.locator('#live')).toHaveText('Riset sudah diperbaiki.');
  await expect(row(page, 'Riset')).toContainText('Mati');
});

test('RAM finding and strip are computed from the rows', async ({ page }) => {
  await ready(page);
  // free 9.2 GB, reserve 2 GB, average of running (1.42 + 1.18) / 2 = 1.30 GB -> floor(7.2 / 1.3) = 5
  await expect(page.locator('#ram-finding')).toHaveText('RAM bebas 9,2 GB · cukup untuk ±5 akun lagi');
  await expect(page.locator('#ram-strip')).toHaveAttribute('aria-label', /Claude 2,6 GB/);
  await expect(page.locator('#ram-strip .seg-claude')).toHaveCount(2);
});

test('a corrupt account list is reported once with its backup', async ({ page }) => {
  await ready(page, '&corrupt=1');
  const alert = page.locator('#alert');
  await expect(alert).toContainText('Daftar akun rusak dan sudah dicadangkan');
  await expect(alert).toContainText('accounts.json.bad-20261112-170000');
  await page.getByRole('button', { name: 'Buka Klien' }).click();
  await expect(alert).toBeEmpty(); // shown once, cleared by the next action
});

test('targets at least 24px', async ({ page }) => {
  await ready(page);
  for (const b of await page.locator('button:visible, input:visible').all()) {
    const box = (await b.boundingBox())!;
    expect(box.height, await b.evaluate((e) => e.outerHTML.slice(0, 80))).toBeGreaterThanOrEqual(24);
    expect(box.width).toBeGreaterThanOrEqual(24);
  }
});

test('200% text zoom does not clip labels', async ({ page }) => {
  await ready(page);
  await page.addStyleTag({ content: 'html { font-size: 200%; }' });
  const clipped = await page.evaluate(() =>
    [...document.querySelectorAll('button, label, h1, .finding')].filter((e) => e.scrollWidth > (e as HTMLElement).clientWidth + 1).map((e) => e.textContent));
  expect(clipped).toEqual([]);
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
  expect(overflow).toBe(false);
});

// ---------- English ----------

test('English: axe AA and every string is translated', async ({ page }) => {
  await ready(page, '&lang=en');
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Claude accounts');
  await expect(page.getByRole('button', { name: 'Add account' })).toBeVisible();
  expect((await new AxeBuilder({ page }).withTags(AA).analyze()).violations).toEqual([]);
  await page.getByRole('button', { name: 'Add account' }).click();
  await expect(page.getByRole('radio', { name: 'This Windows user' })).toBeChecked();
  expect((await new AxeBuilder({ page }).withTags(AA).analyze()).violations).toEqual([]);
});

test('switching language redraws the panel, renames a default Main and keeps focus on the picker', async ({ page }) => {
  await ready(page);
  await expect(row(page, 'Utama')).toBeVisible();
  const picker = page.getByRole('combobox', { name: 'Bahasa' });
  await picker.focus();
  await picker.press('Enter');
  await page.getByRole('option', { name: 'English' }).click();
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  await expect(page.getByRole('combobox', { name: 'Language' })).toBeFocused();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Claude accounts');
  await expect(row(page, 'Main')).toBeVisible();
  await expect(page.locator('#count')).toHaveText('2 running');
  await page.getByRole('combobox', { name: 'Language' }).click();
  await page.getByRole('option', { name: 'Bahasa Indonesia' }).click();
  await expect(row(page, 'Utama')).toBeVisible();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Akun Claude');
});

test('language names are marked with their own language', async ({ page }) => {
  await ready(page);
  await expect(page.locator('#lang-list [role="option"][data-value="en"]')).toHaveAttribute('lang', 'en');
  await expect(page.locator('#lang-list [role="option"][data-value="id"]')).toHaveAttribute('lang', 'id');
});

test('English: 200% text zoom does not clip labels', async ({ page }) => {
  await ready(page, '&lang=en');
  await page.getByRole('button', { name: 'Add account' }).click();
  await page.addStyleTag({ content: 'html { font-size: 200%; }' });
  const clipped = await page.evaluate(() =>
    [...document.querySelectorAll('button, label, h1, .finding')].filter((e) => e.scrollWidth > (e as HTMLElement).clientWidth + 1).map((e) => e.textContent));
  expect(clipped).toEqual([]);
  expect(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)).toBe(false);
});

test('every language has every key', async ({ page }) => {
  await ready(page);
  const missing = await page.evaluate(() => {
    const all = (window as any).STRINGS_BY_LANG as Record<string, Record<string, string>>;
    const keys = new Set(Object.values(all).flatMap((d) => Object.keys(d)));
    return Object.entries(all).flatMap(([lang, d]) => [...keys].filter((k) => !(k in d)).map((k) => `${lang}:${k}`));
  });
  expect(missing).toEqual([]);
  const langs = await page.evaluate(() => (window as any).LANGS.map((l: string[]) => l[0]).sort());
  expect(langs).toEqual(Object.keys(await page.evaluate(() => (window as any).STRINGS_BY_LANG)).sort());
});

// ---- Coding Folks brand kit (docs/design/branding-coding-folks.md) ----

test('brand fonts load from the app itself', async ({ page }) => {
  await ready(page);
  await page.evaluate(() => document.fonts.ready);
  expect(await page.evaluate(() => document.fonts.check('16px "Instrument Sans"'))).toBe(true);
  expect(await page.evaluate(() => document.fonts.check('12px "IBM Plex Mono"'))).toBe(true);
  const family = await page.locator('#add-open').evaluate((el) => getComputedStyle(el).fontFamily);
  expect(family.startsWith('"Instrument Sans"')).toBe(true);
  const num = await page.locator('#count').evaluate((el) => getComputedStyle(el).fontFamily);
  expect(num.startsWith('"IBM Plex Mono"')).toBe(true);
});

for (const scheme of ['light', 'dark'] as const) {
  test(`primary button uses the brand blue (${scheme})`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await ready(page);
    const bg = await page.locator('#add-open').evaluate((el) => getComputedStyle(el).backgroundColor);
    expect(bg).toBe('rgb(13, 106, 255)');
  });
}

test('footer says the app is by Coding Folks, in both languages', async ({ page }) => {
  await ready(page);
  await expect(page.locator('.foot')).toContainText('oleh Coding Folks');
  await ready(page, '&lang=en');
  await expect(page.locator('.foot')).toContainText('by Coding Folks');
});

test('a 40-character account name does not widen the panel', async ({ page }) => {
  await ready(page);
  await row(page, 'Kantor').getByRole('button', { name: 'Opsi untuk Kantor' }).click();
  await page.getByRole('menuitem', { name: 'Ganti nama' }).click();
  await page.locator('#confirm-input').fill('W'.repeat(40));
  await page.locator('#confirm-ok').click();
  await expect(page.locator('.name', { hasText: 'WWWW' })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(320);
});

// Contrast measured on the computed colours (axe does not see hover states).
function contrast(a: string, b: string) {
  const lum = (c: string) => {
    const [r, g, bl] = c.match(/\d+/g)!.slice(0, 3).map((v) => {
      const x = Number(v) / 255;
      return x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
    });
    return 0.2126 * r + 0.7152 * g + 0.0722 * bl;
  };
  const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

for (const scheme of ['light', 'dark'] as const) {
  test(`hovered primary button text stays AA (${scheme})`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await ready(page);
    const btn = page.locator('#add-open');
    await btn.hover();
    await page.waitForTimeout(200); // the 120 ms background transition
    const [fg, bg] = await btn.evaluate((el) => [getComputedStyle(el).color, getComputedStyle(el).backgroundColor]);
    expect(contrast(fg, bg)).toBeGreaterThanOrEqual(4.5);
  });
}

test('the stopped dot and the "other" RAM colour stay visible on light surfaces (3:1)', async ({ page }) => {
  await ready(page);
  const other = await page.evaluate(() => {
    const d = document.createElement('i'); d.style.color = 'var(--other)'; document.body.append(d);
    const c = getComputedStyle(d).color; d.remove(); return c;
  });
  expect(contrast(other, 'rgb(255, 255, 255)')).toBeGreaterThanOrEqual(3);
});

// ---- Components like the landing page (docs/design/update-and-panel-polish.md) ----

test('buttons are pills of the landing page sizes', async ({ page }) => {
  await ready(page);
  for (const b of await page.locator('.primary:visible, .secondary:visible').all()) {
    const r = await b.evaluate((el) => parseFloat(getComputedStyle(el).borderTopLeftRadius));
    expect(r, await b.evaluate((e) => e.outerHTML.slice(0, 60))).toBeGreaterThanOrEqual(18);
  }
  expect((await page.locator('#add-open').boundingBox())!.height).toBe(44);
  expect((await page.getByRole('button', { name: 'Buka Klien' }).boundingBox())!.height).toBe(36);
});

test('text inputs are 44 px with a focus ring', async ({ page }) => {
  await ready(page);
  await page.locator('#add-open').click();
  const input = page.locator('#add-label');
  await input.focus();
  await page.waitForTimeout(250); // the 150 ms focus transition
  expect((await input.boundingBox())!.height).toBe(44);
  const shadow = await input.evaluate((el) => getComputedStyle(el).boxShadow);
  expect(shadow).toContain('rgb(231, 240, 255)');
});

test('autostart is a pill toggle', async ({ page }) => {
  await ready(page);
  const t = page.locator('#autostart');
  const box = (await t.boundingBox())!;
  expect([box.width, box.height]).toEqual([40, 24]);
  expect(await t.evaluate((el) => getComputedStyle(el).appearance)).toBe('none');
  if (!(await t.isChecked())) await t.click();
  await page.waitForTimeout(250); // the 150 ms colour transition
  expect(await t.evaluate((el) => getComputedStyle(el).backgroundColor)).toBe('rgb(13, 106, 255)');
});

test('the RAM track differs from the panel ground in light mode', async ({ page }) => {
  await ready(page);
  const [track, ground] = await page.evaluate(() => [
    getComputedStyle(document.querySelector('.strip')!).backgroundColor,
    getComputedStyle(document.body).backgroundColor,
  ]);
  expect(track).not.toBe(ground);
  expect(track).toBe('rgb(209, 211, 216)');
});

// ---- Language picker like the landing page Select (APG select-only combobox) ----

test('language picker opens with Enter, arrows move, Esc closes and keeps the language', async ({ page }) => {
  await ready(page);
  const picker = page.getByRole('combobox', { name: 'Bahasa' });
  await picker.focus();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('listbox')).toBeVisible();
  await expect(picker).toHaveAttribute('aria-expanded', 'true');
  await expect(picker).toHaveAttribute('aria-activedescendant', 'lang-opt-id');
  await page.keyboard.press('ArrowDown');
  await expect(picker).toHaveAttribute('aria-activedescendant', 'lang-opt-en');
  await page.keyboard.press('Escape');
  await expect(page.getByRole('listbox')).toBeHidden();
  await expect(picker).toHaveAttribute('aria-expanded', 'false');
  await expect(picker).toBeFocused();
  await expect(page.locator('html')).toHaveAttribute('lang', 'id');
});

test('typing a letter jumps to that language, Enter chooses it', async ({ page }) => {
  await ready(page);
  const picker = page.getByRole('combobox', { name: 'Bahasa' });
  await picker.focus();
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('e');
  await expect(picker).toHaveAttribute('aria-activedescendant', 'lang-opt-en');
  await page.keyboard.press('Enter');
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  await expect(page.getByRole('combobox', { name: 'Language' })).toBeFocused();
  await expect(page.getByRole('combobox', { name: 'Language' })).toContainText('English');
});

test('the chosen language is the selected option', async ({ page }) => {
  await ready(page);
  await page.getByRole('combobox', { name: 'Bahasa' }).click();
  await expect(page.getByRole('option', { name: 'Bahasa Indonesia' })).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByRole('option', { name: 'English' })).toHaveAttribute('aria-selected', 'false');
});

test('axe AA with the language list open', async ({ page }) => {
  for (const scheme of ['light', 'dark'] as const) {
    await page.emulateMedia({ colorScheme: scheme });
    await ready(page);
    await page.getByRole('combobox', { name: 'Bahasa' }).click();
    expect((await new AxeBuilder({ page }).withTags(AA).analyze()).violations).toEqual([]);
  }
});

test('the language list stays inside the window and closes on an outside click', async ({ page }) => {
  await ready(page);
  const picker = page.getByRole('combobox', { name: 'Bahasa' });
  await picker.scrollIntoViewIfNeeded();
  await picker.click();
  const list = (await page.getByRole('listbox').boundingBox())!;
  const h = await page.evaluate(() => window.innerHeight);
  expect(list.y).toBeGreaterThanOrEqual(0);
  expect(list.y + list.height).toBeLessThanOrEqual(h);
  await page.locator('h1').click();
  await expect(page.getByRole('listbox')).toBeHidden();
});

// ---- Many accounts: the list scrolls, search above eight ----

async function readyMany(page: Page) {
  await page.goto(PANEL + '&many=12');
  await expect(page.locator('.account')).toHaveCount(12);
}

test('twelve accounts: the list scrolls and the panel does not grow', async ({ page }) => {
  await ready(page);
  const five = await page.evaluate(() => document.querySelector('.panel')!.scrollHeight);
  await readyMany(page);
  const twelve = await page.evaluate(() => document.querySelector('.panel')!.scrollHeight);
  expect(twelve).toBeLessThanOrEqual(five + 120); // capped list + the search field
  const [sh, ch] = await page.locator('#accounts').evaluate((el) => [el.scrollHeight, el.clientHeight]);
  expect(sh).toBeGreaterThan(ch);
});

test('arrow keys reach the last account and scroll it into view', async ({ page }) => {
  await readyMany(page);
  await page.getByRole('button', { name: 'Tampilkan Utama' }).focus();
  for (let i = 0; i < 11; i++) await page.keyboard.press('ArrowDown');
  const last = page.locator('.account').last().locator('[data-role="primary"]');
  await expect(last).toBeFocused();
  const [btn, list] = [await last.boundingBox(), await page.locator('#accounts').boundingBox()];
  expect(btn!.y + btn!.height).toBeLessThanOrEqual(list!.y + list!.height + 1);
});

test('no search with five accounts', async ({ page }) => {
  await ready(page);
  await expect(page.getByRole('searchbox')).toBeHidden();
});

test('search appears above eight accounts and filters by name, ignoring case', async ({ page }) => {
  await readyMany(page);
  const search = page.getByRole('searchbox', { name: 'Cari akun' });
  await search.fill('klien');
  await expect(page.locator('.account')).toHaveCount(3);
  await search.fill('zzz');
  await expect(page.locator('.account')).toHaveCount(0);
  await expect(page.locator('#no-match')).toHaveText('Tidak ada akun bernama “zzz”.');
});

test('Esc in the search clears it and keeps the panel open', async ({ page }) => {
  await readyMany(page);
  const search = page.getByRole('searchbox', { name: 'Cari akun' });
  await search.fill('tim');
  await page.keyboard.press('Escape');
  await expect(search).toHaveValue('');
  await expect(page.locator('.account')).toHaveCount(12);
  await expect(search).toBeFocused();
});

test('renaming while filtered applies the filter again', async ({ page }) => {
  await readyMany(page);
  await page.getByRole('searchbox', { name: 'Cari akun' }).fill('tim');
  await expect(page.locator('.account')).toHaveCount(2);
  await row(page, 'Tim Data').getByRole('button', { name: 'Opsi untuk Tim Data' }).click();
  await page.getByRole('menuitem', { name: 'Ganti nama' }).click();
  await page.locator('#confirm-input').fill('Keuangan Dua');
  await page.locator('#confirm-ok').click();
  await expect(page.locator('.account')).toHaveCount(1);
});

test('axe AA with twelve accounts and a search', async ({ page }) => {
  await readyMany(page);
  await page.getByRole('searchbox', { name: 'Cari akun' }).fill('klien');
  expect((await new AxeBuilder({ page }).withTags(AA).analyze()).violations).toEqual([]);
});

// ---- Update from GitHub ----

test('no update row without an update', async ({ page }) => {
  await ready(page);
  await expect(page.locator('#update')).toBeHidden();
});

test('the update row shows the version and Perbarui', async ({ page }) => {
  await ready(page, '&update=1');
  const row = page.locator('#update');
  await expect(row).toBeVisible();
  await expect(row).toContainText('Versi 0.3.0 tersedia');
  await expect(row.getByRole('button', { name: 'Perbarui' })).toBeEnabled();
  expect((await new AxeBuilder({ page }).withTags(AA).analyze()).violations).toEqual([]);
});

test('Perbarui shows the download progress and disables the button', async ({ page }) => {
  await ready(page, '&update=1');
  const button = page.locator('#update').getByRole('button', { name: 'Perbarui' });
  await button.click();
  await expect(button).toBeDisabled();
  await expect(page.locator('#update-msg')).toContainText('Mengunduh pembaruan…');
  await expect(page.locator('#update-msg')).toContainText('100%');
});

test('a failed update says so and keeps the row', async ({ page }) => {
  await ready(page, '&update=fail');
  await page.locator('#update').getByRole('button', { name: 'Perbarui' }).click();
  await expect(page.locator('#alert')).toContainText('Pembaruan gagal');
  await expect(page.locator('#update')).toBeVisible();
  await expect(page.locator('#update').getByRole('button', { name: 'Perbarui' })).toBeEnabled();
  await expect(page.locator('#update-msg')).toContainText('Versi 0.3.0 tersedia');
});

// ---- Fixes from the final review ----

test('forced-colors: a focused text field shows a focus outline', async ({ page }) => {
  await page.emulateMedia({ forcedColors: 'active' });
  await ready(page);
  await page.locator('#add-open').click();
  await page.locator('#add-label').focus();
  const style = await page.locator('#add-label').evaluate((el) => getComputedStyle(el).outlineStyle);
  expect(style).not.toBe('none');
});

test('renaming the focused account out of the search keeps focus in the panel (on the search)', async ({ page }) => {
  await readyMany(page);
  await page.getByRole('searchbox', { name: 'Cari akun' }).fill('tim');
  await row(page, 'Tim Data').getByRole('button', { name: 'Opsi untuk Tim Data' }).click();
  await page.getByRole('menuitem', { name: 'Ganti nama' }).click();
  await page.locator('#confirm-input').fill('Keuangan Dua');
  await page.locator('#confirm-ok').click();
  await expect(page.locator('.account')).toHaveCount(1);
  await expect(page.getByRole('searchbox', { name: 'Cari akun' })).toBeFocused();
});

// ---- Update checks only with the user's consent: asked once, then a toggle ----

test('asks once whether to check for new versions; Ya turns the toggle on', async ({ page }) => {
  await ready(page, '&updates=unasked');
  const ask = page.locator('#updates-ask');
  await expect(ask).toBeVisible();
  await expect(ask).toContainText('Beri tahu saya bila ada versi baru?');
  expect((await new AxeBuilder({ page }).withTags(AA).analyze()).violations).toEqual([]);
  await ask.getByRole('button', { name: 'Ya, beri tahu' }).click();
  await expect(ask).toBeHidden();
  await expect(page.getByRole('switch', { name: 'Cek versi baru' })).toBeChecked();
});

test('Tidak hides the question and leaves the toggle off', async ({ page }) => {
  await ready(page, '&updates=unasked');
  await page.locator('#updates-ask').getByRole('button', { name: 'Tidak' }).click();
  await expect(page.locator('#updates-ask')).toBeHidden();
  await expect(page.getByRole('switch', { name: 'Cek versi baru' })).not.toBeChecked();
});

test('no question and no toggle when this build has no update key', async ({ page }) => {
  await ready(page, '&updates=unavailable');
  await expect(page.locator('#updates-ask')).toBeHidden();
  await expect(page.getByRole('switch', { name: 'Cek versi baru' })).toBeHidden();
});

test('turning the checks off hides an offered update', async ({ page }) => {
  await ready(page, '&update=1');
  await expect(page.locator('#update')).toBeVisible();
  await page.getByRole('switch', { name: 'Cek versi baru' }).click();
  await expect(page.locator('#update')).toBeHidden();
});

// ---- claude: links: ask the user to pick Claude Switcher once in Windows' Default apps ----

test('when Windows opens claude links with Claude itself, the panel says how to fix it', async ({ page }) => {
  await ready(page, '&link=other');
  const card = page.locator('#link-ask');
  await expect(card).toBeVisible();
  await expect(card).toContainText('Login Google masuk ke akun Utama?');
  expect((await new AxeBuilder({ page }).withTags(AA).analyze()).violations).toEqual([]);
  await card.getByRole('button', { name: 'Buka pengaturan Windows' }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__MOCK__.calls.open_link_settings ?? 0)).toBe(1);
});

test('Nanti hides the link card until the panel opens again', async ({ page }) => {
  await ready(page, '&link=other');
  await page.locator('#link-ask').getByRole('button', { name: 'Nanti' }).click();
  await expect(page.locator('#link-ask')).toBeHidden();
});

test('no link card when Claude Switcher already opens claude links, or there is no account of this user', async ({ page }) => {
  await ready(page, '&link=ours');
  await expect(page.locator('#link-ask')).toBeHidden();
  await ready(page);
  await expect(page.locator('#link-ask')).toBeHidden();
});
