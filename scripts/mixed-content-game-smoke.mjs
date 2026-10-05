// Launch the isolated mixed local/Modrinth instance produced by test:world-native.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:net';
import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium, expect as baseExpect } from '@playwright/test';
const expect = baseExpect.configure({ timeout: 30_000 });
const workspace = path.resolve(import.meta.dirname, '..');
const artifacts = path.join(workspace, '.local/world-content-smoke');
const fixture = JSON.parse(await readFile(path.join(artifacts, 'result.json'), 'utf8'));
assert(fixture.passed && fixture.second);
const testRoot = path.resolve(fixture.dataDirectory);
assert(testRoot.startsWith(`${path.resolve(artifacts)}${path.sep}`));
const id = fixture.second;
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
    SPORIUM_TEST_DATA_DIR: testRoot,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
  },
});
let browser, page, logPath;
const errors = [];
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
      if (attempt >= 30 || !/LIBRARY_BUSY|INSTANCE_BUSY/.test(error.message)) throw error;
      await delay(100);
    }
  }
}
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
  assert(browser);
  page = browser.contexts()[0].pages()[0];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.waitForSelector('.app-shell');
  const before = await invoke('installed_content', { id });
  assert.equal(before.length, 3);
  assert.equal(before.filter((r) => r.record.provider === 'local').length, 1);
  assert.equal(before.filter((r) => r.record.provider === 'modrinth').length, 2);
  await page.evaluate((value) => {
    window.location.hash = `/instance/${value}`;
  }, id);
  await expect(page.locator('.content-records > li')).toHaveCount(3);
  for (const width of [1400, 1000]) {
    await page.setViewportSize({ width, height: 900 });
    await page.screenshot({ path: path.join(artifacts, `mixed-${width}.png`), fullPage: true });
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth),
      false,
    );
    assert.equal(
      await page
        .locator('.content-records > li')
        .evaluateAll((rows) => rows.some((row) => row.scrollWidth > row.clientWidth)),
      false,
    );
  }
  await page.getByRole('button', { name: 'Играть', exact: true }).click();
  await expect
    .poll(
      async () => {
        const state = await invoke('game_state');
        if (state.job?.phase === 'failed') throw new Error(JSON.stringify(state.job.error));
        return state.sessions.some((s) => s.instanceId === id && s.running);
      },
      { timeout: 180_000 },
    )
    .toBe(true);
  const game = (await invoke('game_state')).sessions.find((s) => s.instanceId === id && s.running);
  logPath = game.logPath;
  await expect
    .poll(
      async () => {
        const log = await readFile(logPath, 'utf8').catch(() => '');
        return /OpenAL initialized|Sound engine started|Created:.*atlas/i.test(log);
      },
      { timeout: 90_000 },
    )
    .toBe(true);
  const log = await readFile(logPath, 'utf8');
  assert(/modmenu/i.test(log));
  assert(!/Incompatible mod set|Mod resolution encountered an incompatible mod set/i.test(log));
  await page.screenshot({ path: path.join(artifacts, 'mixed-game.png'), fullPage: true });
  await invoke('stop_game', { id });
  await expect
    .poll(async () =>
      (await invoke('game_state')).sessions.some((s) => s.instanceId === id && s.running),
    )
    .toBe(false);
  assert.deepEqual(await invoke('installed_content', { id }), before);
  assert.deepEqual(errors, []);
  await writeFile(
    path.join(artifacts, 'game-result.json'),
    JSON.stringify(
      {
        passed: true,
        id,
        dataDirectory: testRoot,
        logPath,
        checks: [
          'mixed local root plus verified provider dependencies launch real Fabric 1.21.1 to renderer/audio',
          'mixed-source installed rows and dependency actions fit 1400px and 1000px',
          'clean stop preserves exact mixed provenance receipts',
        ],
        date: new Date().toISOString(),
      },
      null,
      2,
    ),
  );
  console.log('Mixed content game smoke: 3 checks passed.');
} catch (error) {
  await page
    ?.screenshot({ path: path.join(artifacts, 'mixed-game-failure.png'), fullPage: true })
    .catch(() => {});
  await writeFile(
    path.join(artifacts, 'game-result.json'),
    JSON.stringify({ passed: false, id, logPath, error: String(error) }, null, 2),
  );
  throw error;
} finally {
  if (page) await invoke('stop_game', { id }).catch(() => {});
  await browser?.close();
  if (child.exitCode === null) {
    const ended = once(child, 'exit');
    child.kill();
    await ended;
  }
}
