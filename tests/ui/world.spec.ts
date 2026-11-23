import { expect, test } from '@playwright/test';
import type { Bootstrap } from '../../src/bindings/core';

test('news and gallery open supplied content, restore focus, and render plain text', async ({
  page,
}) => {
  await page.goto('/');
  const bootstrap: Bootstrap = await page.evaluate(async () => {
    const moduleUrl = '/src/services/backend.ts';
    return (await import(moduleUrl)).browserPreview();
  });
  bootstrap.settings.values.world.feed = 'https://example.invalid/feed.json';
  bootstrap.settings.values.world.address = '';
  await page.goto('about:blank');
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.addInitScript(
    ({ bootstrap }) => {
      Object.assign(window, {
        isTauri: true,
        __TAURI_INTERNALS__: {
          metadata: {
            currentWindow: { label: 'main' },
            currentWebview: { label: 'main', windowLabel: 'main' },
          },
          transformCallback: () => 1,
          unregisterCallback: () => {},
          invoke: async (name: string) => {
            switch (name) {
              case 'bootstrap':
                return bootstrap;
              case 'library_snapshot':
                return { instances: [], collections: [] };
              case 'game_state':
                return { job: null, sessions: [] };
              case 'automatic_reports':
              case 'pack_opening':
                return [];
              case 'world_feed':
                return {
                  news: [
                    {
                      title: 'Test article',
                      date: '2026-01-01',
                      summary: 'Summary',
                      body: 'Full text <b>in plain text</b>',
                      image: null,
                    },
                  ],
                  gallery: [{ title: 'Test photo', image: 'photo', thumbnail: 'thumb' }],
                };
              case 'world_image':
                return (
                  'data:image/png;base64,' +
                  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/UV4AAAAASUVORK5CYII='
                );
              case 'profile_skin':
                return { status: 'missing', png: null, slim: false, fetchedAt: 0 };
              case 'plugin:event|listen':
                return 1;
              default:
                return null;
            }
          },
        },
        __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} },
      });
    },
    { bootstrap },
  );
  await page.goto('/#/news');
  const article = page.getByRole('button', { name: /Test article/ });
  await article.click();
  await expect(page.getByRole('dialog')).toContainText('Full text <b>in plain text</b>');
  await page.keyboard.press('Escape');
  await expect(article).toBeFocused();
  await page.locator('aside nav a[href="#/world"]').click();
  const photo = page.getByRole('button', { name: /Test photo/ });
  await photo.click();
  await expect(page.getByRole('dialog').getByRole('img', { name: 'Test photo' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(photo).toBeFocused();
  expect(errors).toEqual([]);
});
