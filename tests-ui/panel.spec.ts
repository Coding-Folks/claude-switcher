import { test, expect, Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

const PANEL = '/index.html?mock=1';
const AA = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'];

async function ready(page: Page, query = '') {
  await page.goto(PANEL + query);
  await expect(page.locator('.account')).toHaveCount(4);
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
  await expect(page.getByRole('button', { name: 'Tampilkan Utama' })).toBeFocused();
  await page.keyboard.press('Shift+Tab');
  await expect(page.locator('#autostart')).toBeFocused();
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
  await page.getByLabel('Nama akun').fill('Tim Desain');
  const seen: string[] = [];
  await page.exposeFunction('__seen', (s: string) => seen.push(s));
  await page.evaluate(() => new MutationObserver(() => (window as any).__seen(document.getElementById('live')!.textContent)).observe(document.getElementById('live')!, { childList: true, characterData: true, subtree: true }));
  await page.getByRole('button', { name: 'Buat akun' }).click();
  await expect(page.locator('#live')).toHaveText('Akun Tim Desain siap. Login di jendela Claude yang terbuka.');
  expect(seen.some((s) => /Membuat user Windows/.test(s))).toBe(true);
  await expect(page.locator('.account')).toHaveCount(5);
  await expect(page.getByRole('button', { name: 'Tambah akun' })).toBeFocused();
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
  await page.getByLabel('Nama akun').fill('Kantor 2');
  await page.getByRole('button', { name: 'Buat akun' }).click();
  await expect(page.getByRole('alert').filter({ hasText: 'Dibatalkan' })).toHaveText('Dibatalkan. Tidak ada yang berubah.');
  await expect(page.getByLabel('Nama akun')).toHaveValue('Kantor 2');
  await expect(page.locator('.account')).toHaveCount(4);
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
  await expect(page.locator('.account')).toHaveCount(3);
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
