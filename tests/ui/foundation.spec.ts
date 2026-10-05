import { expect, test } from '@playwright/test';

test('home opens real library routes, search is local, unsupported actions are explicit', async ({
  page,
}) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Мои сборки', exact: true })).toBeVisible();
  await expect(page.getByRole('note')).toContainText('Веб-предпросмотр');
  await expect(page.locator('.library-card')).toHaveCount(6);
  await page.getByRole('searchbox').fill('fabric');
  await expect(page.locator('.library-card')).toHaveCount(1);
  await page.getByRole('link', { name: 'Fabric — Открыть библиотеку' }).click();
  await expect(page).toHaveURL(/#\/instances\/fabric$/);
  await expect(page.getByRole('heading', { name: 'Fabric', exact: true })).toBeVisible();
  const library = page
    .locator('.page')
    .filter({ has: page.getByRole('heading', { name: 'Fabric', exact: true }) });
  await expect(library.getByRole('button', { name: 'Создать сборку', exact: true })).toBeDisabled();
  await expect(page.getByText('Здесь начнётся новая история')).toBeVisible();
  expect(errors).toEqual([]);
});

test('appearance is only in settings and browser preview never reports a successful save', async ({
  page,
}) => {
  await page.goto('/');
  await expect(page.getByLabel('Язык интерфейса')).toHaveCount(0);
  await page
    .getByRole('navigation', { name: 'Основная навигация' })
    .getByRole('link', { name: 'Настройки' })
    .click();
  await expect(page).toHaveURL(/#\/settings\/appearance$/);
  await expect(page.getByLabel('Язык интерфейса')).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Сохранить изменения' })).toBeDisabled();
  await page
    .getByRole('navigation', { name: 'Разделы настроек' })
    .getByRole('link', { name: 'Анимации', exact: true })
    .click();
  await expect(page.getByRole('switch')).toBeDisabled();
  await expect(page.getByText('Настройки сохранены')).toHaveCount(0);
});

test('unknown library and unknown route have a safe way home', async ({ page }) => {
  for (const path of ['/#/instances/unknown', '/#/missing', '/#/settings/missing']) {
    await page.goto(path);
    await expect(page.getByRole('heading', { name: 'Здесь пока нет тропинки' })).toBeVisible();
    await page.getByRole('link', { name: 'На главную', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Мои сборки', exact: true })).toBeVisible();
  }
});

test('shell fits supported widths and respects system reduced motion', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto('/');
  await expect(page.locator('.library-card')).toHaveCount(6);
  for (const width of [1400, 1000, 900, 600, 390]) {
    await page.setViewportSize({ width, height: 900 });
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
    ).toBe(true);
  }
  expect(
    await page.locator('.page').evaluate((element) => getComputedStyle(element).animationName),
  ).toBe('none');
  await page.keyboard.press('Tab');
  await expect(page.getByRole('link', { name: 'Перейти к содержимому' })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.locator('main')).toBeFocused();
});
