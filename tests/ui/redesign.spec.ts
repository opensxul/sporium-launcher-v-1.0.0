import { expect, test } from '@playwright/test';

test('top search opens existing library filters and profile links to skin settings', async ({
  page,
}) => {
  await page.goto('/#/');
  const search = page.getByRole('search').getByRole('searchbox');
  await search.fill('Vanilla');
  await search.press('Enter');
  await expect(page).toHaveURL(/#\/instances\?q=Vanilla$/);
  await expect(page.locator('main').getByRole('searchbox')).toHaveValue('Vanilla');
  await page.locator('.sidebar').getByRole('link', { name: 'Профиль' }).click();
  await page.getByRole('link', { name: 'Настройки аккаунта' }).click();
  await expect(page).toHaveURL(/#\/settings\/accounts$/);
  await expect(page.getByRole('region', { name: 'Скин', exact: true })).toBeVisible();
});

test('existing screens fit the prescribed window sizes', async ({ page }, info) => {
  test.setTimeout(60_000);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  const routes = [
    '/',
    '/news',
    '/world',
    '/profile',
    '/library',
    '/catalog',
    '/instances',
    '/folders',
    '/downloads',
    '/settings/accounts',
    '/settings/minecraft',
    '/settings/java',
    '/settings/catalog',
    '/settings/downloads',
    '/settings/storage',
    '/settings/appearance',
    '/settings/animations',
    '/settings/advanced',
    '/settings/about',
  ];
  for (const [width, height] of [
    [1280, 720],
    [1366, 768],
    [1600, 900],
    [1920, 1080],
    [2560, 1440],
  ] as const) {
    await page.setViewportSize({ width, height });
    for (const route of routes) {
      await page.goto('/#' + route);
      await expect(page.locator('main h1')).toBeVisible();
      expect(
        await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
        route + ' at ' + width + '×' + height,
      ).toBe(true);
      if (width === 1280 && ['/', '/library', '/settings/accounts'].includes(route)) {
        await page.screenshot({
          path: info.outputPath((route.slice(1).replace('/', '-') || 'home') + '.png'),
          animations: 'disabled',
        });
      }
    }
  }
});
