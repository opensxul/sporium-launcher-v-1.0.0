import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdir, readFile, writeFile, access } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { createServer } from 'node:net';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium, expect } from '@playwright/test';

// Only the explicit probe fixture. Never copy or edit normal launcher data.
const workspace = path.resolve(import.meta.dirname, '..');
const dataDirectory = path.join(workspace, '.local/game-smoke');
const artifacts = path.join(workspace, '.local/download-native-smoke');
await mkdir(artifacts, { recursive: true });
const fixture = JSON.parse(await readFile(path.join(dataDirectory, 'results/26.3.json'), 'utf8'));
const catalog = JSON.parse(
  await readFile(path.join(dataDirectory, 'shared/cache/version-catalog.json'), 'utf8'),
);
const entry = catalog.manifest.versions.find((v) => v.id === '26.3');
const metadata = JSON.parse(
  await readFile(path.join(dataDirectory, `shared/cache/versions/${entry.sha1}.json`), 'utf8'),
);
const logging = metadata.logging.client.file;
assert(/^[A-Za-z0-9._-]+$/.test(logging.id));
const damaged = path.join(dataDirectory, 'shared/assets/log_configs', logging.id);
const original = await readFile(damaged);
assert.equal(createHash('sha1').update(original).digest('hex'), logging.sha1);
const world = path.join(
  dataDirectory,
  'instances',
  fixture.instanceId,
  'saves/phase8-preserve.dat',
);
const mod = path.join(dataDirectory, 'instances', fixture.instanceId, 'mods/phase8-preserve.dat');
await writeFile(world, 'world sentinel');
await writeFile(mod, 'user mod sentinel');
const checks = [];
const errors = [];
let session;
async function launch() {
  const server = createServer();
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const port = server.address().port;
  await new Promise((resolve) => server.close(resolve));
  const child = spawn(path.join(workspace, 'src-tauri/target/debug/sporium.exe'), [], {
    cwd: workspace,
    windowsHide: true,
    stdio: 'ignore',
    env: {
      ...process.env,
      SPORIUM_TEST_DATA_DIR: dataDirectory,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
  });
  let browser;
  try {
    for (let attempt = 0; attempt < 160; attempt++) {
      assert.equal(child.exitCode, null);
      try {
        browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
        break;
      } catch {
        await delay(250);
      }
    }
    assert(browser, 'WebView2 unavailable');
    const page = browser.contexts()[0].pages()[0];
    page.setDefaultTimeout(60_000);
    page.on('pageerror', (error) => errors.push(error.message));
    await page.waitForSelector('.app-shell');
    const invoke = (command, args = {}) =>
      page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), {
        command,
        args,
      });
    const go = (route) =>
      page.evaluate((route) => {
        window.location.hash = route;
      }, route);
    return { child, browser, page, invoke, go };
  } catch (error) {
    await browser?.close();
    child.kill();
    throw error;
  }
}
async function stop() {
  if (!session) return;
  await session.browser.close();
  if (session.child.exitCode === null) {
    const exited = once(session.child, 'exit');
    session.child.kill();
    await exited;
  }
  session = null;
}
try {
  session = await launch();
  let { page, invoke, go } = session;
  await go('/settings/downloads');
  await page.getByLabel('Одновременных загрузок', { exact: true }).selectOption('1');
  const save = page.getByRole('button', { name: 'Сохранить изменения', exact: true }).first();
  if (await save.isEnabled()) await save.click();
  await expect
    .poll(async () => (await invoke('bootstrap')).settings.values.downloadConcurrency)
    .toBe(1);
  await page.screenshot({ path: path.join(artifacts, 'download-settings.png'), fullPage: true });
  checks.push('configurable concurrency saved by real settings UI');
  await writeFile(damaged, 'corrupt managed file');
  await go('/downloads');
  await invoke('start_game', { request: { id: fixture.instanceId, action: 'install' } });
  await page.getByRole('button', { name: 'Приостановить', exact: true }).first().click();
  await expect.poll(async () => (await invoke('game_state')).job.paused).toBe(true);
  await delay(350);
  const paused = (await invoke('game_state')).job;
  await delay(600);
  const still = (await invoke('game_state')).job;
  assert.equal(still.completedFiles, paused.completedFiles);
  assert.equal(still.bytesPerSecond, 0);
  const cleanupError = await page.evaluate(async () => {
    try {
      await window.__TAURI_INTERNALS__.invoke('download_cache', { cleanup: true });
      return null;
    } catch (error) {
      return error.code;
    }
  });
  assert.equal(cleanupError, 'INSTANCE_BUSY');
  await page.screenshot({ path: path.join(artifacts, 'paused.png'), fullPage: true });
  checks.push('pause freezes queue; cleanup cannot race active installation');
  // Terminate only the owned test launcher, then verify its real persisted operation.
  await stop();
  await writeFile(damaged, 'corrupt managed file after interruption');
  session = await launch();
  ({ page, invoke, go } = session);
  const interrupted = await invoke('game_state');
  assert.equal(interrupted.job.phase, 'interrupted');
  assert.equal(interrupted.sessions.length, 0);
  await go('/downloads');
  await page.getByRole('button', { name: 'Продолжить установку', exact: true }).first().waitFor();
  await page.screenshot({ path: path.join(artifacts, 'recovered.png'), fullPage: true });
  await page.getByRole('button', { name: 'Продолжить установку', exact: true }).first().click();
  await page.getByText('Операция завершена', { exact: true }).first().waitFor({ timeout: 180_000 });
  const final = (await invoke('game_state')).job;
  assert.equal(final.phase, 'completed');
  assert(final.cachedFiles > 0);
  assert(final.repairedFiles >= 1);
  assert.equal(
    createHash('sha1')
      .update(await readFile(damaged))
      .digest('hex'),
    logging.sha1,
  );
  assert.equal(await readFile(world, 'utf8'), 'world sentinel');
  assert.equal(await readFile(mod, 'utf8'), 'user mod sentinel');
  await assert.rejects(() => access(path.join(dataDirectory, 'launcher/download-operation.json')));
  await page.screenshot({ path: path.join(artifacts, 'repaired.png'), fullPage: true });
  checks.push(
    'restart exposes interrupted operation without launch; explicit resume repairs corrupt managed file and preserves world/mod bytes',
  );
  const marker = path.join(dataDirectory, 'shared/cache/parts/native-cleanup.part');
  await mkdir(path.dirname(marker), { recursive: true });
  await writeFile(marker, 'partial');
  await go('/settings/storage');
  await page.getByRole('button', { name: 'Пересчитать размер', exact: true }).click();
  await page.getByLabel('Автоматический предел кэша', { exact: true }).selectOption('64');
  const storageSave = page
    .getByRole('button', { name: 'Сохранить изменения', exact: true })
    .first();
  if (await storageSave.isEnabled()) await storageSave.click();
  await expect.poll(async () => (await invoke('bootstrap')).settings.values.cacheLimitMb).toBe(64);
  await page.getByRole('button', { name: 'Очистить кэш', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Очистить кэш', exact: true }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await assert.rejects(() => access(marker));
  assert.equal(await readFile(world, 'utf8'), 'world sentinel');
  assert.equal(await readFile(mod, 'utf8'), 'user mod sentinel');
  assert.equal(
    createHash('sha1')
      .update(await readFile(damaged))
      .digest('hex'),
    logging.sha1,
  );
  await access(
    path.join(dataDirectory, 'shared/cache/clients', `${metadata.downloads.client.sha1}.jar`),
  );
  await page.screenshot({ path: path.join(artifacts, 'storage.png'), fullPage: true });
  checks.push('cache size, persisted limit, confirmed cleanup preserves game client and user data');
  assert.deepEqual(errors, []);
  await writeFile(
    path.join(artifacts, 'result.json'),
    JSON.stringify({ passed: true, checks, job: final, dataDirectory }, null, 2),
  );
  console.log(`Download native smoke passed: ${checks.length} checks.`);
} catch (error) {
  await session?.page
    .screenshot({ path: path.join(artifacts, 'failure.png'), fullPage: true })
    .catch(() => {});
  throw error;
} finally {
  await stop();
  await writeFile(damaged, original);
}
