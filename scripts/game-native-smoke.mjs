import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { promisify } from 'node:util';
import { once } from 'node:events';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium, expect } from '@playwright/test';

// Uses only the explicit game-probe fixtures. It never touches normal launcher data.
const workspace = path.resolve(import.meta.dirname, '..');
const dataDirectory = path.join(workspace, '.local/game-smoke');
const artifacts = path.join(workspace, '.local/game-native-smoke');
const release = JSON.parse(await readFile(path.join(dataDirectory, 'results/26.3.json'), 'utf8'));
await mkdir(artifacts, { recursive: true });
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
let page;
let launchError;
child.on('error', (error) => {
  launchError = error;
});
const failures = [];
const checks = [];
try {
  for (let attempt = 0; attempt < 120; attempt++) {
    if (launchError) throw launchError;
    assert.equal(child.exitCode, null, 'Launcher exited before WebView2 started');
    try {
      browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
      break;
    } catch {
      await delay(250);
    }
  }
  assert(browser, 'WebView2 endpoint unavailable');
  page = browser.contexts()[0].pages()[0];
  page.setDefaultTimeout(30_000);
  page.on('pageerror', (error) => failures.push(error.message));
  await page.waitForSelector('.app-shell');
  const go = async (route) =>
    page.evaluate((route) => {
      window.location.hash = route;
    }, route);
  const invoke = (command, args = {}) =>
    page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), {
      command,
      args,
    });
  assert.equal(await page.locator('.preview-notice').count(), 0);

  await go('/settings/minecraft');
  const visibilitySave = page
    .getByRole('button', { name: 'Сохранить изменения', exact: true })
    .first();
  for (const name of ['Снапшоты, pre-release и RC', 'Beta', 'Alpha']) {
    await page.getByRole('switch', { name, exact: true }).uncheck();
  }
  if (await visibilitySave.isEnabled()) {
    await visibilitySave.click();
    await page.getByText('Настройки сохранены', { exact: true }).waitFor();
  }
  await expect(page.getByRole('switch', { name: 'Релизы', exact: true })).toBeChecked();
  for (const name of ['Снапшоты, pre-release и RC', 'Beta', 'Alpha']) {
    await page.getByRole('switch', { name, exact: true }).check();
  }
  await page.getByRole('button', { name: 'Сохранить изменения', exact: true }).first().click();
  await page.getByText('Настройки сохранены', { exact: true }).waitFor();
  const saved = await invoke('bootstrap');
  assert(
    saved.settings.values.versionVisibility.beta &&
      saved.settings.values.versionVisibility.alpha &&
      saved.settings.values.versionVisibility.snapshots,
  );
  await page.screenshot({ path: path.join(artifacts, 'versions.png'), fullPage: true });
  checks.push('version visibility saved through real settings IPC');

  await go('/');
  await page.getByRole('button', { name: 'Создать сборку', exact: true }).first().click();
  const select = page.getByLabel('Версия Minecraft', { exact: true });
  for (const version of ['26.3', '26.4-snapshot-1', 'b1.7.3', 'a1.2.6']) {
    await select.selectOption(version, { timeout: 60_000 });
    assert.equal(await select.inputValue(), version);
  }
  await page.getByRole('dialog').getByRole('button', { name: 'Отмена', exact: true }).click();
  checks.push('official release, snapshot, beta and alpha picker');

  await go('/settings/java');
  await page.getByRole('button', { name: 'Найти установленные Java', exact: true }).click();
  await expect(page.locator('.runtime-list')).toContainText('Java 25', { timeout: 30_000 });
  await expect(page.locator('.runtime-list')).toContainText('Java 1.8', { timeout: 30_000 });
  const javaPath = await page.locator('.runtime-list code').first().innerText();
  await page.getByLabel('Путь к java.exe', { exact: true }).fill(javaPath);
  await page.getByRole('button', { name: 'Проверить Java', exact: true }).click();
  await page.getByText(/Java проверена:/).waitFor();
  await page.getByRole('button', { name: 'Вернуть автоматический выбор', exact: true }).click();
  await page.screenshot({ path: path.join(artifacts, 'java.png'), fullPage: true });
  checks.push('real managed Java 8/25 discovery and custom path inspection');

  await go('/settings/accounts');
  const sharedLibrary = await invoke('library_snapshot');
  await page.getByRole('button', { name: 'Изменить ник', exact: true }).first().click();
  const nicknameField = page.getByLabel('Ник локального профиля', { exact: true });
  await nicknameField.fill('invalid name');
  await expect(
    page.getByRole('dialog').getByRole('button', { name: 'Сохранить', exact: true }),
  ).toBeDisabled();
  await nicknameField.fill('LocalSmoke_26');
  await page.getByRole('dialog').getByRole('button', { name: 'Сохранить', exact: true }).click();
  await expect(page.locator('.account-widget')).toContainText('LocalSmoke_26');
  await page.reload();
  await page.waitForSelector('.app-shell');
  await expect(page.locator('.profile-card').first()).toContainText('LocalSmoke_26');
  const alternate = `Alt_${Date.now().toString().slice(-10)}`;
  await page.getByRole('button', { name: 'Добавить профиль', exact: true }).click();
  await page.getByLabel('Ник локального профиля', { exact: true }).fill(alternate);
  await page.getByRole('dialog').getByRole('button', { name: 'Сохранить', exact: true }).click();
  const alternateCard = page
    .locator('.profile-card')
    .filter({ has: page.getByRole('heading', { name: alternate, exact: true }) });
  await alternateCard.getByRole('button', { name: 'Выбрать профиль', exact: true }).click();
  await expect(page.locator('.account-widget')).toContainText(alternate);
  assert.deepEqual(
    await invoke('library_snapshot'),
    sharedLibrary,
    'Global nickname switch must preserve the entire shared library',
  );
  await go(`/instance/${release.instanceId}`);
  await expect(page.locator('.instance-status-panel')).toContainText(alternate);
  await go('/settings/accounts');
  await alternateCard.getByRole('button', { name: 'Удалить', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Удалить', exact: true }).click();
  await expect(page.locator('.account-widget')).toContainText('LocalSmoke_26');
  assert.deepEqual(await invoke('library_snapshot'), sharedLibrary);
  const skin = await invoke('profile_skin', { nickname: 'Notch', refresh: true });
  assert.equal(skin.status, 'found');
  assert(skin.png.startsWith('data:image/png;base64,'));
  await page.screenshot({ path: path.join(artifacts, 'local-profile.png'), fullPage: true });
  checks.push(
    'global profile create/switch/rename/delete, nickname validation and SQLite persistence; unchanged shared library; real public skin lookup',
  );

  await go(`/instance/${release.instanceId}`);
  await page.getByRole('button', { name: 'Играть', exact: true }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await page
    .getByRole('button', { name: 'Остановить игру', exact: true })
    .waitFor({ timeout: 120_000 });
  await expect(page.getByRole('button', { name: 'Дублировать', exact: true })).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Удалить', exact: true })).toBeDisabled();
  const state = await invoke('game_state');
  const session = state.sessions.find(
    (item) => item.instanceId === release.instanceId && item.running,
  );
  assert(session, 'Real Java process must be tracked');
  const commandInspection = await promisify(execFile)(
    'powershell.exe',
    [
      '-NoProfile',
      '-Command',
      `$p = Get-CimInstance Win32_Process -Filter "ParentProcessId = ${child.pid}" | Where-Object { $_.Name -eq 'java.exe' }; if (@($p).Count -ne 1) { throw 'Expected one owned Java process' }; @{ demo = [bool]($p.CommandLine -match '(^|\\s)--demo(\\s|$)'); localName = [bool]($p.CommandLine -match 'LocalSmoke_26') } | ConvertTo-Json -Compress`,
    ],
    { windowsHide: true },
  );
  assert.deepEqual(JSON.parse(commandInspection.stdout.trim()), { demo: false, localName: true });
  const bootstrap = await invoke('bootstrap');
  assert.equal(bootstrap.settings.values.localNickname, 'LocalSmoke_26');
  let log = '';
  for (let attempt = 0; attempt < 180; attempt++) {
    log = await readFile(session.logPath, 'utf8');
    if (log.includes('OpenAL initialized') && log.includes('textures/atlas/gui.png-atlas')) break;
    await delay(500);
  }
  assert(
    log.includes('OpenAL initialized') && log.includes('textures/atlas/gui.png-atlas'),
    'Minecraft must initialize audio and render its actual interface',
  );
  await page.screenshot({ path: path.join(artifacts, 'running.png'), fullPage: true });
  checks.push(
    'local client renderer/audio launched through UI, no --demo in owned Java arguments, saved launch nickname; duplicate/delete blocked while running',
  );
  await page.getByRole('button', { name: 'Остановить игру', exact: true }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Остановить игру', exact: true })
    .click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Дублировать', exact: true })).toBeEnabled();
  await page.getByText(/Игра завершилась. Код выхода/).waitFor();
  checks.push('confirmed stop reaps the owned process and releases the instance lease');

  // A fresh operation can be cancelled and retried without losing the cache or installed state.
  await page.getByRole('button', { name: 'Проверить файлы', exact: true }).click();
  await page.getByRole('button', { name: 'Отменить операцию', exact: true }).first().click();
  await page.getByText('Операция отменена', { exact: true }).first().waitFor({ timeout: 60_000 });
  await page.getByRole('button', { name: 'Проверить файлы', exact: true }).click();
  await page.getByText('Операция завершена', { exact: true }).first().waitFor({ timeout: 120_000 });
  checks.push('cancel and retry reuses verified files');
  await page.screenshot({ path: path.join(artifacts, 'installed.png'), fullPage: true });
  assert.deepEqual(failures, []);
  await writeFile(
    path.join(artifacts, 'result.json'),
    JSON.stringify({ passed: true, checks, logPath: session.logPath, dataDirectory }, null, 2),
  );
  console.log(`Game native smoke passed: ${checks.length} checks. ${artifacts}`);
} catch (error) {
  if (page) {
    await page
      .screenshot({ path: path.join(artifacts, 'failure.png'), fullPage: true })
      .catch(() => {});
    await writeFile(
      path.join(artifacts, 'failure-ui.txt'),
      await page
        .locator('body')
        .innerText()
        .catch(() => 'Native test window was closed.'),
    ).catch(() => {});
  }
  throw error;
} finally {
  await browser?.close();
  if (child.exitCode === null) {
    const exited = once(child, 'exit');
    child.kill();
    await exited;
  }
}
