import { Buffer } from 'node:buffer';
import { performance } from 'node:perf_hooks';
import assert from 'node:assert/strict';
import { spawn, execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { once } from 'node:events';
import { mkdir, mkdtemp, cp, readFile, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import path from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium, expect } from '@playwright/test';

const root = path.resolve(import.meta.dirname, '..');
const artifacts = path.join(root, '.local/world-native');
await mkdir(artifacts, { recursive: true });
const library = await mkdtemp(path.join(artifacts, 'library-'));
await mkdir(path.join(library, 'launcher'));
const sqlite = new DatabaseSync(path.join(library, 'launcher/sporium.sqlite3'));
for (const name of ['001_settings', '002_instances', '003_local_profiles']) {
  sqlite.exec(await readFile(path.join(root, 'src-tauri/migrations', name + '.sql'), 'utf8'));
}
sqlite.exec('PRAGMA user_version=3');
sqlite.prepare('INSERT INTO settings VALUES(1,7,?)').run(
  JSON.stringify({
    schemaVersion: 1,
    locale: 'ru-RU',
    motion: 'system',
    uiScale: 100,
    localNickname: 'SporiumLocal',
  }),
);
sqlite.close();
const version = process.env.SPORIUM_SMOKE_VERSION || '1.21.1';
if (process.env.SPORIUM_SMOKE_SHARED) {
  try {
    await promisify(execFile)(
      'robocopy.exe',
      [
        process.env.SPORIUM_SMOKE_SHARED,
        path.join(library, 'shared'),
        '/E',
        '/XJ',
        '/R:1',
        '/W:1',
        '/NFL',
        '/NDL',
        '/NJH',
        '/NJS',
        '/NP',
        '/MT:12',
      ],
      { windowsHide: true },
    );
  } catch (error) {
    if (typeof error.code !== 'number' || error.code >= 8) throw error;
  }
}
const executable = path.join(library, 'Sporium.exe');
await cp(
  process.env.SPORIUM_SMOKE_EXE ?? path.join(root, 'src-tauri/target/release/Sporium.exe'),
  executable,
);
const errors = [];
async function listen(server) {
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  return server.address().port;
}
async function launch() {
  const listener = createServer();
  const port = await listen(listener);
  await new Promise((r) => listener.close(r));
  const started = performance.now();
  const child = spawn(executable, ['--library', library], {
    windowsHide: true,
    env: {
      ...process.env,
      WEBVIEW2_USER_DATA_FOLDER: path.join(library, 'webview'),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=' + port,
    },
    stdio: 'ignore',
  });
  try {
    const endpoint = 'http://127.0.0.1:' + port;
    let browser;
    for (let i = 0; i < 140; i++) {
      if (child.exitCode !== null) throw Error('Release process exited with ' + child.exitCode);
      try {
        if ((await fetch(endpoint + '/json/version')).ok) {
          browser = await chromium.connectOverCDP(endpoint);
          break;
        }
      } catch {
        /* WebView2 is still initializing. */
      }
      await delay(200);
    }
    assert(browser, 'WebView2 did not start');
    const context = browser.contexts()[0];
    const page = context.pages()[0] ?? (await context.waitForEvent('page'));
    page.on('pageerror', (e) => errors.push(e.message));
    await page.locator('.forest-hero').waitFor();
    const invoke = (name, args) =>
      page.evaluate(([name, args]) => window.__TAURI_INTERNALS__.invoke(name, args), [name, args]);
    return { child, browser, page, invoke, startMs: Math.round(performance.now() - started) };
  } catch (e) {
    child.kill();
    throw e;
  }
}
async function sampleTree(pid) {
  const script = [
    '$rows=Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId',
    '$ids=@(' + Number(pid) + ')',
    'do {$children=@($rows | Where-Object {$ids -contains $_.ParentProcessId -and $ids -notcontains $_.ProcessId} | Select-Object -ExpandProperty ProcessId);$ids+= $children}while($children.Count)',
    '$sample=Get-Process -Id $ids -ErrorAction SilentlyContinue',
    '@{cpu=($sample|Measure-Object CPU -Sum).Sum;workingSet=($sample|Measure-Object WorkingSet64 -Sum).Sum;count=@($sample).Count}|ConvertTo-Json -Compress',
  ].join(';');
  const { stdout } = await promisify(execFile)(
    'powershell.exe',
    ['-NoProfile', '-NonInteractive', '-Command', script],
    { windowsHide: true },
  );
  return JSON.parse(stdout.trim());
}
async function stop(session) {
  if (!session) return;
  await session.browser.close().catch(() => {});
  if (session.child.exitCode === null) {
    const end = once(session.child, 'exit');
    session.child.kill();
    await end;
  }
}
let session;
let malformed = false;
let gameHandshake = false;
const sockets = new Set();
const statusServer = createServer((socket) => {
  sockets.add(socket);
  socket.on('close', () => sockets.delete(socket));
  socket.on('error', () => {});
  let pending = Buffer.alloc(0),
    answered = false;
  socket.on('data', (data) => {
    pending = Buffer.concat([pending, data]);
    if (!answered && pending.length > pending[0] && pending[pending[0]] === 2) {
      gameHandshake = true;
      socket.end();
      return;
    }
    if (!answered && pending.length >= 2 && pending.length >= pending[0] + 3) {
      if (malformed) {
        socket.end(Buffer.from([2, 0, 127]));
        return;
      }
      const payload = Buffer.from(
        JSON.stringify({ version: { name: version }, players: { online: 2, max: 12 } }),
      );
      assert(payload.length < 120);
      socket.write(Buffer.concat([Buffer.from([payload.length + 2, 0, payload.length]), payload]));
      answered = true;
      pending = Buffer.alloc(0);
    } else if (answered && pending.length >= 10) {
      socket.end(pending.subarray(0, 10));
    }
  });
});
const port = await listen(statusServer);
const report = { version, library, checks: [] };
try {
  session = await launch();
  let { page, invoke } = session;
  report.startMs = session.startMs;
  assert.equal((await invoke('bootstrap')).info.databaseSchema, 4);
  assert.equal((await invoke('bootstrap')).settings.values.localNickname, 'SporiumLocal');
  const distribution = JSON.parse(await readFile(path.join(root, 'config/sporium.json'), 'utf8'));
  assert.equal((await invoke('bootstrap')).settings.values.world.address, distribution.address);
  if (process.env.SPORIUM_SMOKE_LIVE === '1') {
    const live = await invoke('server_status');
    assert(live.online);
    report.liveServer = live;
    await expect(page.locator('.live-state')).toContainText('Онлайн');
  }

  await page.screenshot({ path: path.join(artifacts, 'home.png'), animations: 'disabled' });
  const before = await sampleTree(session.child.pid);
  const idleStart = performance.now();
  await delay(5000);
  const after = await sampleTree(session.child.pid);
  report.idle = {
    sampleMs: Math.round(performance.now() - idleStart),
    cpuSeconds: Number((after.cpu - before.cpu).toFixed(2)),
    workingSetMiB: Math.round(after.workingSet / 1048576),
    processes: after.count,
  };
  const cdp = await page.context().newCDPSession(page);
  for (const [width, height, scale] of [
    [1920, 1080, 1],
    [1280, 800, 1.25],
    [900, 620, 1.5],
  ]) {
    await cdp.send('Emulation.setDeviceMetricsOverride', {
      width,
      height,
      deviceScaleFactor: scale,
      mobile: false,
    });
    assert(
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
      'Horizontal clipping at ' + width,
    );
  }
  await cdp.send('Emulation.clearDeviceMetricsOverride');
  await cdp.detach();
  report.checks.push(
    '1.0.1 database migrates and local nickname is preserved; native home renders',
  );
  for (const route of [
    '/news',
    '/world',
    '/profile',
    '/library',
    '/catalog',
    '/instances',
    '/folders',
    '/downloads',
    '/settings/appearance',
  ]) {
    await page
      .locator('.sidebar a[href="#' + (route.startsWith('/settings') ? '/settings' : route) + '"]')
      .click();
    await expect(page).toHaveURL(new RegExp('#' + route + '$'));
  }
  await page.evaluate(() => (window.location.hash = '/settings/minecraft'));
  await page.getByLabel('Адрес Sporium', { exact: true }).fill('127.0.0.1:' + port);
  await page.getByLabel('Версия Minecraft для Sporium').fill(version);
  await page.getByLabel('Память Java, МиБ', { exact: false }).fill('1024');
  await page.getByRole('button', { name: 'Сохранить изменения' }).click();
  await page.getByText('Настройки сохранены', { exact: true }).waitFor();
  await page.evaluate(() => (window.location.hash = '/'));
  await expect(page.locator('.live-state')).toContainText('Онлайн');
  await expect(page.locator('.server-panel')).toContainText('2 / 12');
  const status = await invoke('server_status');
  assert.equal(status.version, version);
  assert.equal(status.maximum, 12);
  assert.equal(typeof status.latencyMs, 'number');
  report.checks.push(
    'nine desktop routes, settings save, Java status handshake and echoed latency',
  );
  await stop(session);
  session = await launch();
  ({ page, invoke } = session);
  const config = (await invoke('bootstrap')).settings.values.world;
  assert.equal(config.memoryMb, 1024);
  assert.equal(config.address, '127.0.0.1:' + port);
  await expect(page.locator('.live-state')).toContainText('Онлайн');
  report.checks.push('native restart preserves settings');
  malformed = true;
  await page.locator('.server-link').click();
  await expect(page.locator('.live-state')).toContainText('Не удалось проверить');
  malformed = false;
  report.checks.push('Malformed server reply reported as a failed check');
  for (const socket of sockets) socket.destroy();
  await new Promise((r) => statusServer.close(r));
  await page.locator('.server-link').click();
  await expect(page.locator('.live-state')).toContainText('Оффлайн');
  if (process.env.SPORIUM_SMOKE_GAME === '1') {
    statusServer.listen(port, '127.0.0.1');
    await once(statusServer, 'listening');
    // First-run Minecraft intentionally pauses Quick Play at its accessibility
    // screen. Complete that initial choice in this disposable library.
    const { affectedId } = await invoke('create_instance', {
      request: {
        name: 'Sporium Survival',
        minecraftVersion: version,
        loader: 'vanilla',
        collectionId: null,
      },
    });
    await writeFile(
      path.join(library, 'instances', affectedId, 'options.txt'),
      'onboardAccessibility:false\n',
    );
    await page.locator('aside nav a[href="#/"]').click();
    await expect(page).toHaveURL(/#\/$/);
    await page.reload();
    await expect(page.locator('.launch-target a')).toHaveAttribute(
      'href',
      '#/instance/' + affectedId,
    );
    await page.locator('.play-button').click();
    let running, job;
    for (let i = 0; i < 540; i++) {
      const state = await invoke('game_state');
      job = state.job;
      if (job?.phase === 'failed') throw Error('Game failed: ' + JSON.stringify(job.error));
      running = state.sessions.find((s) => s.running);
      if (running) break;
      await delay(1000);
    }
    assert(running, 'Minecraft did not start: ' + JSON.stringify(job));
    let rendered = false;
    for (let i = 0; i < 150; i++) {
      const log = await readFile(running.logPath, 'utf8').catch(() => '');
      const session = (await invoke('game_state')).sessions.find(
        (item) => item.instanceId === running.instanceId,
      );
      if (session && !session.running) {
        throw new Error(
          `Minecraft exited during startup with code ${session.exitCode}. Log: ${running.logPath}`,
        );
      }
      if (log.includes('OpenAL initialized') && /textures\/atlas\/.*atlas/.test(log)) {
        rendered = true;
        break;
      }
      await delay(500);
    }
    assert(rendered, 'Java did not initialize the renderer and audio');
    const installed = JSON.parse(
      await readFile(
        path.join(library, 'instances', running.instanceId, '.sporium/installation.json'),
        'utf8',
      ),
    );
    assert.equal(installed.memoryMb, 1024);
    report.gameVersion = installed.version;
    if (config.autoJoin) {
      await expect.poll(() => gameHandshake, { timeout: 60000 }).toBe(true);
      report.checks.push(
        'Minecraft sent a multiplayer login handshake to the configured local test endpoint',
      );
    }
    await invoke('stop_game', { id: running.instanceId });
    report.checks.push(
      'home installs a Vanilla instance and launches real Minecraft, audio and renderer start, 1024 MiB applied, owned game stops',
    );
  }
  assert.deepEqual(errors, []);
  await writeFile(path.join(artifacts, 'result.json'), JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report));
} finally {
  for (const socket of sockets) socket.destroy();
  if (statusServer.listening) statusServer.close();
  await stop(session);
}
