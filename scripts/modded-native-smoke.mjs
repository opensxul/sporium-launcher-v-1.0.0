import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium, expect } from '@playwright/test';

// Run sequentially after other native suites: WebView2 shares one browser profile.
const workspace = path.resolve(import.meta.dirname, '..');
const dataDirectory = path.join(workspace, '.local/modded-smoke');
const artifacts = path.join(workspace, '.local/modded-native-smoke');
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
const checks = [];
const errors = [];
try {
  for (let attempt = 0; attempt < 120; attempt++) {
    assert.equal(child.exitCode, null);
    try {
      browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
      break;
    } catch {
      await delay(250);
    }
  }
  assert(browser, 'WebView2 endpoint unavailable');
  page = browser.contexts()[0].pages()[0];
  page.setDefaultTimeout(90_000);
  page.on('pageerror', (error) => errors.push(error.message));
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
  const bootstrap = await invoke('bootstrap');
  assert.equal(bootstrap.info.databaseSchema, 3);
  const active = bootstrap.profiles.profiles.find(
    (p) => p.id === bootstrap.profiles.activeProfileId,
  );
  assert.equal(active.nickname, 'Notch');
  await go('/settings/accounts');
  const skin = await invoke('profile_skin', { nickname: 'Notch', refresh: true });
  assert.equal(skin.status, 'found');
  await page.getByRole('button', { name: 'Обновить скин', exact: true }).first().click();
  await expect(page.locator('.skin-preview canvas')).toHaveCount(1);
  const skinPixels = await page
    .locator('.skin-preview canvas')
    .evaluate((canvas) =>
      [...canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data].some(
        (v) => v !== 0,
      ),
    );
  assert(skinPixels, 'Skin preview must actually paint pixels');
  const headPixels = await page
    .locator('.skin-preview canvas')
    .evaluate((canvas) =>
      [...canvas.getContext('2d').getImageData(24, 0, 48, 48).data].some(
        (v, i) => i % 4 !== 3 && v > 40,
      ),
    );
  assert(headPixels, 'Legacy opaque hat must not obscure the skin face');
  const toggle = page.getByRole('switch', { name: /^Скины по нику в игре/ });
  if (await toggle.isChecked()) await toggle.click();
  await expect
    .poll(async () => (await invoke('bootstrap')).settings.values.nicknameSkins)
    .toBe(false);
  await go('/settings/appearance');
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await page.reload();
  await page.waitForSelector('.app-shell');
  await go('/settings/accounts');
  await expect(toggle).not.toBeChecked();
  await toggle.click();
  await expect
    .poll(async () => (await invoke('bootstrap')).settings.values.nicknameSkins)
    .toBe(true);
  await page.screenshot({ path: path.join(artifacts, 'profiles.png'), fullPage: true });
  checks.push(
    'real Mojang nickname skin lookup and painted preview; global skin toggle persists and does not dirty unrelated settings',
  );
  const fixtures = [
    ['fabric-1.21.1', '0.19.5'],
    ['forge-1.12.2', '1.12.2-14.23.5.2864'],
    ['forge-1.20.1', '1.20.1-47.4.23'],
    ['neoforge-1.21.1', '21.1.252'],
  ];
  for (const [name, version] of fixtures) {
    const fixture = JSON.parse(
      await readFile(path.join(dataDirectory, `results/${name}.json`), 'utf8'),
    );
    await go(`/instance/${fixture.instanceId}`);
    await expect(page.locator('.instance-status-panel')).toContainText('Notch');
    await expect(page.getByRole('button', { name: 'Играть', exact: true })).toBeEnabled();
    const picker = page.locator('.launch-settings select');
    await expect(picker).toHaveValue(version);
    await expect(picker).toBeEnabled({ timeout: 90_000 });
    assert((await picker.locator('option').count()) > 1);
    await expect(page.getByLabel('Профиль запуска', { exact: true })).toHaveCount(0);
    await page.screenshot({ path: path.join(artifacts, `${name}.png`), fullPage: true });
    checks.push(`${name}: compatible version picker, pinned version and shared global nickname`);
  }
  assert.deepEqual(errors, []);
  await writeFile(
    path.join(artifacts, 'result.json'),
    JSON.stringify({ passed: true, checks, dataDirectory }, null, 2),
  );
  console.log('Modded native checks passed.');
} catch (error) {
  await page
    ?.screenshot({ path: path.join(artifacts, 'failure.png'), fullPage: true })
    .catch(() => {});
  throw error;
} finally {
  await browser?.close();
  if (child.exitCode === null) {
    const exited = once(child, 'exit');
    child.kill();
    await exited;
  }
}
