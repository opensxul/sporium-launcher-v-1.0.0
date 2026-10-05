import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:net';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { DatabaseSync } from 'node:sqlite';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium, expect as baseExpect } from '@playwright/test';
const expect = baseExpect.configure({ timeout: 30_000 });
const workspace = path.resolve(import.meta.dirname, '..');
const artifacts = path.join(workspace, '.local/ui-polish-smoke');
await mkdir(artifacts, { recursive: true });
const dataDirectory = await mkdtemp(path.join(artifacts, 'data-'));
let child, browser, page;
const errors = [],
  checks = [];
async function launch() {
  const server = createServer();
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const port = server.address().port;
  await new Promise((resolve) => server.close(resolve));
  child = spawn(path.join(workspace, 'src-tauri/target/debug/sporium.exe'), [], {
    cwd: workspace,
    windowsHide: true,
    stdio: 'ignore',
    env: {
      ...process.env,
      SPORIUM_TEST_DATA_DIR: dataDirectory,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
  });
  for (let attempt = 0; attempt < 120; attempt++) {
    assert.equal(child.exitCode, null);
    try {
      browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
      break;
    } catch {
      await delay(250);
    }
  }
  assert(browser);
  const context = browser.contexts()[0];
  page = context.pages()[0] ?? (await context.waitForEvent('page'));
  page.setDefaultTimeout(30_000);
  page.on('pageerror', (error) => errors.push(error.message));
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.waitForSelector('.app-shell');
  assert.equal(await page.locator('.preview-notice').count(), 0);
}
async function stop() {
  await browser?.close();
  browser = null;
  if (child && child.exitCode === null) {
    const exit = once(child, 'exit');
    child.kill();
    await exit;
  }
}
async function invoke(command, args = {}) {
  for (let attempt = 0; ; attempt++) {
    try {
      return await page.evaluate(
        async ({ command, args }) => {
          try {
            return await window.__TAURI_INTERNALS__.invoke(command, args);
          } catch (error) {
            throw new Error(JSON.stringify(error), { cause: error });
          }
        },
        { command, args },
      );
    } catch (error) {
      if (attempt >= 50 || !/LIBRARY_BUSY|INSTANCE_BUSY/.test(error.message)) throw error;
      await delay(100);
    }
  }
}
async function go(route) {
  await page.evaluate((route) => {
    window.location.hash = route;
  }, route);
}
async function screenshot(name, width = 1400) {
  await page.setViewportSize({ width, height: 900 });
  assert.equal(
    await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth),
    false,
  );
  await page.screenshot({ path: path.join(artifacts, `${name}-${width}.png`) });
}
try {
  await launch();
  assert.equal(await page.locator('.recent-section').count(), 0);
  const ids = [];
  for (const name of ['Лесной мир', 'Исследования', 'Строительство', 'Без запусков'])
    ids.push(
      (
        await invoke('create_instance', {
          request: { name, minecraftVersion: '1.21.4', loader: 'vanilla', collectionId: null },
        })
      ).affectedId,
    );
  await stop();
  // Explicit test-only historical timestamps and an existing cached icon exercise rendering
  // without claiming a Minecraft launch or bypassing the production native file picker.
  const icon = Buffer.from(
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==',
    'base64',
  );
  const hash = createHash('sha256').update(icon).digest('hex');
  await mkdir(path.join(dataDirectory, 'shared/instance-icons'), { recursive: true });
  await writeFile(path.join(dataDirectory, 'shared/instance-icons', `${hash}.png`), icon);
  const db = new DatabaseSync(path.join(dataDirectory, 'launcher/sporium.sqlite3'));
  try {
    for (const [index, id] of ids.entries()) {
      const record = JSON.parse(
        db.prepare('SELECT payload FROM instances WHERE id=?').get(id).payload,
      );
      record.lastPlayedAt = index < 3 ? Date.now() - index * 3600_000 : null;
      record.favorite = index === 0;
      if (index === 0) {
        record.iconRef = `custom:${hash}`;
        record.iconSource = 'custom';
      }
      db.prepare('UPDATE instances SET payload=? WHERE id=?').run(JSON.stringify(record), id);
    }
  } finally {
    db.close();
  }
  await launch();
  await expect(page.locator('.recent-item')).toHaveCount(3);
  await expect(page.locator('.recent-item').first()).toContainText('Лесной мир');
  await expect(
    page.locator('.recent-item').first().getByRole('button', { name: 'Играть: Лесной мир' }),
  ).toBeEnabled();
  await expect(page.locator('.recent-item').first().locator('img')).toBeVisible();
  await expect
    .poll(() =>
      page
        .locator('.recent-item')
        .first()
        .locator('img')
        .evaluate((img) => img.complete && img.naturalWidth > 0),
    )
    .toBe(true);
  assert.equal(await page.locator('.recent-item').filter({ hasText: 'Без запусков' }).count(), 0);
  checks.push(
    'Continue uses persisted launch ordering, excludes never-played instances and displays cached custom icons',
  );
  await screenshot('home');
  await screenshot('home', 1000);
  await go('/instances');
  await page.getByLabel('Порядок сборок').selectOption('name');
  await expect(page.locator('.instance-card').first()).toContainText('Без запусков');
  await page.getByRole('button', { name: 'Избранное', exact: true }).click();
  await expect(page.locator('.instance-card')).toHaveCount(1);
  checks.push('Library sorting and Favorites filter keep the shared instances');
  await go(`/instance/${ids[0]}`);
  await expect(page.getByRole('tab')).toHaveCount(9);
  await expect(page.getByRole('tab', { name: 'Обзор', exact: true })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await expect(page.locator('.instance-metrics')).toContainText('0 / 0');
  await expect(page.locator('.instance-id')).not.toBeVisible();
  await screenshot('overview');
  await screenshot('overview', 1000);
  await page.getByRole('tab', { name: 'Обзор', exact: true }).focus();
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('tab', { name: 'Моды', exact: true })).toBeFocused();
  await expect(page.locator('.content-installed')).toHaveAttribute('data-view', 'mod');
  await expect(page.locator('.world-content')).not.toBeVisible();
  await expect(page.locator('.content-history')).not.toBeVisible();
  checks.push(
    'Nine accessible URL-backed tabs support keyboard navigation and hide unrelated content',
  );
  await page.getByRole('tab', { name: 'Ресурс-паки', exact: true }).click();
  await page.getByRole('button', { name: 'Добавить контент', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  await expect(
    dialog.locator('button[aria-pressed=true]').filter({ hasText: 'Ресурспаки' }),
  ).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  checks.push(
    'Embedded Modrinth browser opens in the selected resource-pack category and closes back to the instance',
  );
  await page.getByRole('tab', { name: 'Миры', exact: true }).click();
  await expect(page.locator('.world-content')).toBeVisible();
  await page.locator('.world-content').getByRole('button').first().click();
  await expect(dialog).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await page.getByRole('tab', { name: 'История', exact: true }).click();
  await page.getByRole('button', { name: 'Точки восстановления', exact: true }).click();
  await expect(dialog).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  checks.push(
    'World import and restore-point dialogs remain visible and dismissible inside their tabs',
  );
  await page.getByRole('tab', { name: 'Настройки', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'Загрузить изображение', exact: true }),
  ).toBeEnabled();
  await expect(page.getByRole('button', { name: 'Случайный логотип', exact: true })).toBeDisabled();
  await expect(
    page.getByRole('button', { name: 'Создать ярлык на рабочем столе', exact: true }),
  ).toBeDisabled();
  await expect(page.locator('.appearance-preview img')).toBeVisible();
  assert.deepEqual(await invoke('instance_icon_catalog'), []);
  await screenshot('settings');
  await screenshot('settings', 1000);
  await page.getByRole('button', { name: 'Сбросить иконку', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Сбросить иконку', exact: true })).toBeDisabled();
  assert.equal(
    (await invoke('library_snapshot')).instances.find((value) => value.id === ids[0]).iconSource,
    'automatic',
  );
  checks.push(
    'Custom icon reset writes native storage; missing final artwork and future Windows shortcuts are honestly disabled',
  );
  await page.getByRole('tab', { name: 'Логи', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'Открыть папку логов', exact: true }),
  ).toBeEnabled();
  await page.reload();
  await expect(page.getByRole('tab', { name: 'Логи', exact: true })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await go('/settings/accounts');
  await expect(page.locator('.profile-card[data-active=true]')).toHaveCount(1);
  await screenshot('profiles', 1000);
  checks.push(
    'Log tab survives reload; global active profile is highlighted; 1400/1000 layouts have no horizontal overflow',
  );
  await stop();
  await launch();
  assert.equal(
    (await invoke('library_snapshot')).instances.find((value) => value.id === ids[0]).iconRef,
    null,
  );
  await expect(page.locator('.recent-item')).toHaveCount(3);
  assert.equal(
    await readFile(path.join(dataDirectory, 'shared/instance-icons', `${hash}.png`), 'base64'),
    icon.toString('base64'),
  );
  checks.push(
    'Restart preserves recent launches and reset without deleting shared cached image bytes',
  );
  assert.deepEqual(errors, []);
  await writeFile(
    path.join(artifacts, 'result.json'),
    JSON.stringify({ passed: true, dataDirectory, ids, checks, errors }, null, 2),
  );
  console.log(`UI polish native: ${checks.length} checks passed.`);
} catch (error) {
  await page?.screenshot({ path: path.join(artifacts, 'failure.png') }).catch(() => {});
  await writeFile(
    path.join(artifacts, 'result.json'),
    JSON.stringify({ passed: false, dataDirectory, checks, errors, error: String(error) }, null, 2),
  );
  throw error;
} finally {
  await stop();
}
