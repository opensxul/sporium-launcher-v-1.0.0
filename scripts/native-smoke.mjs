import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdir, mkdtemp, readFile, access, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium, expect } from '@playwright/test';

const workspace = path.resolve(import.meta.dirname, '..');
const artifactDirectory = path.join(workspace, '.local/native-smoke');
await mkdir(artifactDirectory, { recursive: true });
const dataDirectory = await mkdtemp(path.join(artifactDirectory, 'data-'));
const executable = path.join(workspace, 'src-tauri/target/debug/sporium.exe');
const failures = [];

// Match the production adapter's bounded wait for a read competing with a background read.
async function snapshot(page) {
  return page.evaluate(async () => {
    for (let attempt = 0; ; attempt++) {
      try {
        return await window.__TAURI_INTERNALS__.invoke('library_snapshot');
      } catch (error) {
        if (attempt >= 12 || error?.code !== 'LIBRARY_BUSY')
          throw new Error(JSON.stringify(error), { cause: error });
        await new Promise((resolve) => setTimeout(resolve, 250));
      }
    }
  });
}

async function freePort() {
  const server = createServer();
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const port = server.address().port;
  await new Promise((resolve, reject) =>
    server.close((error) => (error ? reject(error) : resolve())),
  );
  return port;
}

async function launch() {
  const port = await freePort();
  const child = spawn(executable, [], {
    cwd: workspace,
    windowsHide: true,
    env: {
      ...process.env,
      SPORIUM_TEST_DATA_DIR: dataDirectory,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: 'ignore',
  });
  let launchError;
  child.on('error', (error) => {
    launchError = error;
  });
  const endpoint = `http://127.0.0.1:${port}`;
  let browser;
  try {
    for (let attempt = 0; attempt < 120; attempt++) {
      if (launchError) throw launchError;
      if (child.exitCode !== null) throw new Error(`Native app exited with code ${child.exitCode}`);
      try {
        const response = await fetch(`${endpoint}/json/version`);
        if (response.ok) {
          browser = await chromium.connectOverCDP(endpoint);
          break;
        }
      } catch {
        /* WebView2 is still starting. */
      }
      await delay(250);
    }
    assert(browser, 'WebView2 debugging endpoint did not start');
    const context = browser.contexts()[0];
    const page = context.pages()[0] ?? (await context.waitForEvent('page'));
    page.on('pageerror', (error) => failures.push(error.message));
    await page.waitForSelector('.app-shell', { timeout: 30_000 });
    assert(
      !(await page.locator('.preview-notice').count()),
      'Must exercise real Tauri IPC, not web preview',
    );
    return { child, browser, page };
  } catch (error) {
    await browser?.close();
    if (child.exitCode === null) child.kill();
    throw error;
  }
}

async function stop(session) {
  await session.browser.close();
  if (session.child.exitCode === null) {
    const exited = once(session.child, 'exit');
    session.child.kill();
    await exited;
  }
}

let session;
let originalId;
let duplicateId;
try {
  session = await launch();
  let page = session.page;
  // No verified candidate must never launch an installer; rejection must release the busy flag.
  for (let attempt = 0; attempt < 2; attempt++) {
    const rejected = await page.evaluate(async () => {
      try {
        await window.__TAURI_INTERNALS__.invoke('app_update_install');
        return 'unexpected success';
      } catch (error) {
        return error.code;
      }
    });
    assert.equal(rejected, 'NOT_FOUND');
  }
  await page.getByRole('heading', { name: 'Мои сборки', exact: true }).waitFor();
  await page.screenshot({
    path: path.join(artifactDirectory, 'home-native.png'),
    animations: 'disabled',
    fullPage: true,
  });
  await expect(page.getByRole('button', { name: 'Создать папку', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'Создать папку', exact: true }).click();
  await page.getByLabel('Название папки', { exact: true }).fill('С друзьями');
  await page.getByLabel('Описание', { exact: true }).fill('Проверка сохранения библиотеки');
  await page.getByLabel('Значок', { exact: true }).selectOption('users');
  await page.getByRole('dialog').getByRole('button', { name: 'Сохранить', exact: true }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await page.getByRole('button', { name: 'Создать сборку', exact: true }).first().click();
  await page.getByLabel('Название сборки', { exact: true }).fill('Лесной мир');
  await page
    .getByLabel('Версия Minecraft', { exact: true })
    .selectOption('1.21.4', { timeout: 60_000 });
  await page.getByLabel('Загрузчик', { exact: true }).selectOption('fabric');
  await page
    .getByLabel('Пользовательская папка', { exact: true })
    .selectOption({ label: 'С друзьями' });
  await page.screenshot({
    path: path.join(artifactDirectory, 'create-instance-native.png'),
    animations: 'disabled',
    fullPage: true,
  });
  await page.getByRole('dialog').getByRole('button', { name: 'Создать', exact: true }).click();
  await page.getByRole('heading', { name: 'Лесной мир', exact: true }).waitFor();
  await expect(page.getByRole('button', { name: 'Играть', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'В избранное', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'Убрать из избранного', exact: true }),
  ).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('button', { name: 'Изменить сборку', exact: true }).click();
  await page.getByLabel('Название сборки', { exact: true }).fill('Лесной мир — Fabric');
  await page.getByRole('dialog').getByRole('button', { name: 'Сохранить', exact: true }).click();
  await page.getByRole('heading', { name: 'Лесной мир — Fabric', exact: true }).waitFor();
  const original = (await snapshot(page)).instances[0];
  originalId = original.id;
  assert(original.favorite);
  assert(original.collectionId);
  const originalPath = path.join(dataDirectory, 'instances', originalId);
  await writeFile(path.join(originalPath, 'saves/smoke-world.dat'), 'precious world');
  await writeFile(path.join(originalPath, 'screenshots/smoke.png'), 'image fixture');
  await writeFile(path.join(originalPath, 'mods/smoke.jar'), 'original mod fixture');
  await page.screenshot({
    path: path.join(artifactDirectory, 'instance-native.png'),
    animations: 'disabled',
    fullPage: true,
  });
  await page.getByRole('button', { name: 'Дублировать', exact: true }).click();
  await page.getByLabel('Название сборки', { exact: true }).fill('Тестовая копия');
  await page.getByRole('dialog').getByRole('button', { name: 'Дублировать', exact: true }).click();
  await page.getByRole('heading', { name: 'Тестовая копия', exact: true }).waitFor();
  duplicateId = (await snapshot(page)).instances.find((item) => item.name === 'Тестовая копия').id;
  const duplicatePath = path.join(dataDirectory, 'instances', duplicateId);
  assert.equal(
    await readFile(path.join(duplicatePath, 'saves/smoke-world.dat'), 'utf8'),
    'precious world',
  );
  await writeFile(path.join(duplicatePath, 'mods/smoke.jar'), 'changed copy');
  assert.equal(
    await readFile(path.join(originalPath, 'mods/smoke.jar'), 'utf8'),
    'original mod fixture',
  );
  await page
    .getByRole('navigation', { name: 'Основная навигация' })
    .getByRole('link', { name: 'Главная', exact: true })
    .click();
  await expect(page.locator('.instance-card')).toHaveCount(2);
  await page.getByRole('searchbox').fill('Лесной мир');
  await expect(page.locator('.instance-card')).toHaveCount(1);
  await expect(page.locator('.empty-state')).toHaveCount(0);
  await page.getByRole('searchbox').fill('');
  await page.getByRole('button', { name: 'Создать папку', exact: true }).click();
  await page.getByLabel('Название папки', { exact: true }).fill('Черновики');
  await page.getByRole('dialog').getByRole('button', { name: 'Сохранить', exact: true }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  const sourceCard = page.locator('.instance-card').filter({ hasText: 'Лесной мир — Fabric' });
  await sourceCard.dragTo(page.locator('.collection-card').filter({ hasText: 'Черновики' }));
  await expect(sourceCard.locator('.collection-tag')).toHaveText('Черновики');
  await page.evaluate(() => {
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
    window.scrollTo(0, 0);
  });
  await page.screenshot({
    path: path.join(artifactDirectory, 'library-native.png'),
    animations: 'disabled',
    fullPage: true,
  });
  await page
    .getByRole('navigation', { name: 'Основная навигация' })
    .getByRole('link', { name: 'Настройки' })
    .click();
  await page.getByLabel('Язык интерфейса').selectOption('en-US');
  await page
    .getByRole('navigation', { name: 'Основная навигация' })
    .getByRole('link', { name: 'Главная' })
    .click();
  await page.getByRole('dialog').waitFor();
  await page.getByRole('button', { name: 'Остаться', exact: true }).click();
  assert.equal(await page.getByLabel('Язык интерфейса').inputValue(), 'en-US');
  await page.getByRole('button', { name: 'Сохранить изменения', exact: true }).first().click();
  await page.getByText('Settings saved', { exact: true }).waitFor();
  await page
    .getByRole('navigation', { name: 'Settings sections' })
    .getByRole('link', { name: 'Animations', exact: true })
    .click();
  await page.getByRole('switch').check();
  await page.getByRole('button', { name: 'Save changes', exact: true }).first().click();
  await page.getByText('Settings saved', { exact: true }).waitFor();
  assert.equal(await page.locator('html').getAttribute('data-motion'), 'reduced');
  await stop(session);
  session = undefined;

  session = await launch();
  page = session.page;
  await page.getByRole('heading', { name: 'My instances', exact: true }).waitFor();
  await expect(page.locator('.instance-card')).toHaveCount(2);
  const restarted = await snapshot(page);
  assert.equal(restarted.instances.length, 2);
  assert.equal(restarted.collections.length, 2);
  const restartedOriginal = restarted.instances.find((item) => item.id === originalId);
  assert(restartedOriginal.favorite);
  assert.equal(
    restarted.collections.find((item) => item.id === restartedOriginal.collectionId).name,
    'Черновики',
  );
  assert.equal(await page.locator('html').getAttribute('lang'), 'en-US');
  assert.equal(await page.locator('html').getAttribute('data-motion'), 'reduced');
  await page
    .getByRole('navigation', { name: 'Main navigation' })
    .getByRole('link', { name: 'Settings' })
    .click();
  await page.getByLabel('Interface scale').selectOption('110');
  await page.getByRole('button', { name: 'Save changes', exact: true }).first().click();
  await page.getByText('Settings saved', { exact: true }).waitFor();
  assert.equal(await page.locator('html').evaluate((element) => element.style.fontSize), '110%');
  await page.screenshot({
    path: path.join(artifactDirectory, 'settings-native.png'),
    animations: 'disabled',
    fullPage: true,
  });

  // Stale writes are exercised through actual IPC, then recovered through the UI.
  const bootstrap = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('bootstrap'));
  await page.evaluate(
    (request) => window.__TAURI_INTERNALS__.invoke('save_settings', { request }),
    {
      values: { ...bootstrap.settings.values, uiScale: 125 },
      expectedRevision: bootstrap.settings.revision,
    },
  );
  await page.getByLabel('Interface scale').selectOption('100');
  await page.getByRole('button', { name: 'Save changes', exact: true }).first().click();
  await page.getByRole('alert').filter({ hasText: 'Settings changed in another window' }).waitFor();
  await page.getByRole('button', { name: 'Reload saved settings' }).click();
  await page.waitForFunction(
    () => document.querySelector('select[aria-label="Interface scale"]').value === '125',
  );
  assert.equal(await page.getByLabel('Interface scale').inputValue(), '125');
  await page.getByLabel('Interface scale').selectOption('100');
  await page.getByRole('button', { name: 'Save changes', exact: true }).first().click();
  await page.getByText('Settings saved', { exact: true }).waitFor();
  await page
    .getByRole('navigation', { name: 'Main navigation' })
    .getByRole('link', { name: 'Home', exact: true })
    .click();
  await page.locator('.instance-card').filter({ hasText: 'Тестовая копия' }).click();
  await page.getByRole('button', { name: 'Delete', exact: true }).click();
  await page.getByLabel('Delete all files, including worlds').check();
  await page.getByRole('dialog').getByRole('button', { name: 'Delete', exact: true }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await assert.rejects(access(path.join(dataDirectory, 'instances', duplicateId)));
  await page.locator('.instance-card').filter({ hasText: 'Лесной мир — Fabric' }).click();
  await page.getByRole('button', { name: 'Delete', exact: true }).click();
  await expect(page.getByLabel('Keep worlds and screenshots in a backup folder')).toBeChecked();
  await page.getByRole('dialog').getByRole('button', { name: 'Delete', exact: true }).click();
  await page.getByText('Worlds and screenshots saved', { exact: true }).waitFor();
  const backupPath = path.join(dataDirectory, 'backups', originalId);
  assert.equal(
    await readFile(path.join(backupPath, 'saves/smoke-world.dat'), 'utf8'),
    'precious world',
  );
  await assert.rejects(access(path.join(backupPath, 'mods')));
  await page.screenshot({
    path: path.join(artifactDirectory, 'preserved-native.png'),
    animations: 'disabled',
    fullPage: true,
  });
  await page
    .getByRole('navigation', { name: 'Main navigation' })
    .getByRole('link', { name: 'Folders', exact: true })
    .click();
  await page.locator('.collection-card').filter({ hasText: 'С друзьями' }).click();
  await page.getByRole('button', { name: 'Edit folder', exact: true }).click();
  await page.getByLabel('Folder name', { exact: true }).fill('Friends renamed');
  await page.getByRole('dialog').getByRole('button', { name: 'Save', exact: true }).click();
  await page.getByRole('heading', { name: 'Friends renamed', exact: true }).waitFor();
  await page.getByRole('button', { name: 'Delete folder', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Delete', exact: true }).click();
  await expect(page.locator('.collection-card')).toHaveCount(1);
  await expect(page.locator('.collection-card')).toContainText('Черновики');
  assert.deepEqual(failures, []);
  await writeFile(
    path.join(artifactDirectory, 'result.json'),
    JSON.stringify(
      {
        passed: true,
        checks: [
          'native startup',
          'real IPC',
          'unverified launcher update rejected and busy flag released',
          'unsaved navigation guard',
          'SQLite restart persistence',
          'locale',
          'reduced motion',
          'scale',
          'concurrent settings conflict and recovery',
          'collection create, edit and delete',
          'instance create, rename and favorite through real UI',
          'uninstalled status and available loader launch',
          'independent duplicate bytes and worlds',
          'instance and collection persistence after restart',
          'full delete and preserve-worlds delete through real UI',
          'instance search and drag-and-drop folder assignment',
        ],
        dataDirectory,
      },
      null,
      2,
    ),
  );
  console.log(`Native smoke checks passed. Screenshots: ${artifactDirectory}`);
} catch (error) {
  if (session) {
    await session.page
      .screenshot({
        path: path.join(artifactDirectory, 'failure-native.png'),
        animations: 'disabled',
        fullPage: true,
      })
      .catch(() => {});
    await writeFile(
      path.join(artifactDirectory, 'failure-ui.txt'),
      await session.page.locator('body').innerText(),
    ).catch(() => {});
  }
  throw error;
} finally {
  if (session) await stop(session);
}
