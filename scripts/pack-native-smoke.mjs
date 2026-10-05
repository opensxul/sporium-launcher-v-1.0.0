// Sequential real WebView2/IPC, private data only. Synthetic worlds prove byte preservation,
// while a separately downloaded real Modrinth pack proves installation and game startup.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:net';
import { readFile, writeFile, mkdir, cp } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium, expect as baseExpect } from '@playwright/test';
const expect = baseExpect.configure({ timeout: 30000 });
const workspace = path.resolve(import.meta.dirname, '..');
const artifacts = path.join(workspace, '.local/pack-native-smoke');
const run = path.join(artifacts, `run-${Date.now()}`);
const dataDirectory = path.join(run, 'data');
const fixtures = path.join(run, 'files');
await mkdir(run, { recursive: true });
const fixture = spawn(
  'powershell.exe',
  [
    '-NoProfile',
    '-ExecutionPolicy',
    'Bypass',
    '-File',
    path.join(workspace, 'scripts/pack-fixtures.ps1'),
    '-Directory',
    fixtures,
  ],
  { windowsHide: true },
);
assert.equal((await once(fixture, 'exit'))[0], 0);
const prior = await readFile(path.join(artifacts, 'result.json'), 'utf8')
  .then(JSON.parse)
  .catch(() => null);
const old = prior?.checks?.some((check) => check.startsWith('real Modrinth mrpack imports'))
  ? prior
  : JSON.parse(
      await readFile(path.join(workspace, '.local/world-content-smoke/result.json'), 'utf8'),
    );
const oldData = path.resolve(old.dataDirectory);
assert(
  oldData.startsWith(`${path.resolve(artifacts)}${path.sep}`) ||
    (old.passed &&
      oldData.startsWith(`${path.resolve(workspace, '.local/world-content-smoke')}${path.sep}`)),
);
await mkdir(dataDirectory, { recursive: true });
await cp(path.join(oldData, 'shared'), path.join(dataDirectory, 'shared'), { recursive: true });
let session, page, liveId, logPath;
const errors = [],
  checks = [];
async function launch(args = []) {
  const server = createServer();
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const port = server.address().port;
  await new Promise((resolve) => server.close(resolve));
  const child = spawn(path.join(workspace, 'src-tauri/target/debug/sporium.exe'), args, {
    cwd: workspace,
    windowsHide: true,
    stdio: 'ignore',
    env: {
      ...process.env,
      SPORIUM_TEST_DATA_DIR: dataDirectory,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
  });
  session = { child };
  for (let i = 0; i < 120; i++) {
    assert.equal(child.exitCode, null);
    try {
      session.browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
      break;
    } catch {
      await delay(250);
    }
  }
  assert(session.browser);
  page = session.browser.contexts()[0].pages()[0];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.waitForSelector('.app-shell');
}
async function stop() {
  if (!session) return;
  await session.browser?.close();
  if (session.child.exitCode === null) {
    const ended = once(session.child, 'exit');
    session.child.kill();
    await ended;
  }
  session = null;
}
async function invoke(command, args = {}) {
  for (let i = 0; ; i++) {
    try {
      return await page.evaluate(
        async ({ command, args }) => {
          try {
            return await window.__TAURI_INTERNALS__.invoke(command, args);
          } catch (e) {
            throw new Error(JSON.stringify(e), { cause: e });
          }
        },
        { command, args },
      );
    } catch (e) {
      if (i >= 40 || !/LIBRARY_BUSY|INSTANCE_BUSY/.test(e.message)) throw e;
      await delay(100);
    }
  }
}
async function show(preview) {
  await page.evaluate(
    (detail) => window.dispatchEvent(new window.CustomEvent('sporium-pack-preview', { detail })),
    preview,
  );
  await page.getByRole('dialog', { name: 'Импорт сборки', exact: true }).waitFor();
}
async function waitPack() {
  for (let i = 0; i < 1600; i++) {
    const job = await invoke('pack_state');
    if (job?.phase === 'completed') return job;
    if (job && ['failed', 'cancelled'].includes(job.phase)) throw new Error(JSON.stringify(job));
    await delay(250);
  }
  throw new Error('Pack timeout');
}
const hash = async (file) =>
  createHash('sha512')
    .update(await readFile(file))
    .digest('hex');
try {
  // Passing the path models the actual Windows association command, with spaces preserved.
  await launch([path.join(fixtures, 'valid.mrpack')]);
  let dialog = page.getByRole('dialog', { name: 'Импорт сборки', exact: true });
  await dialog.waitFor();
  await expect(dialog.getByLabel('Название новой сборки')).toHaveValue('Native import fixture');
  assert.equal((await invoke('library_snapshot')).instances.length, 0);
  await delay(2200);
  assert.equal(await dialog.count(), 1);
  checks.push('associated-file startup opens exactly one preview and requires confirmation');
  await dialog.getByRole('button', { name: 'Отмена', exact: true }).click();
  assert.equal((await invoke('library_snapshot')).instances.length, 0);
  checks.push('preview cancellation leaves the library empty');
  await page.getByRole('button', { name: 'Импортировать', exact: true }).click();
  await expect(
    dialog.getByRole('button', { name: 'Выбрать .mrpack или .sporium', exact: true }),
  ).toBeEnabled();
  await expect(
    dialog.getByRole('button', { name: 'Из другого лаунчера', exact: true }),
  ).toBeEnabled();
  await page.screenshot({ path: path.join(artifacts, 'import-entry.png') });
  await dialog.getByRole('button', { name: 'Закрыть', exact: true }).click();
  checks.push('real home import entry offers pack picker and external launcher picker');
  for (const file of ['unsafe.mrpack', 'corrupt.mrpack'])
    await assert.rejects(invoke('pack_preview', { path: path.join(fixtures, file) }));
  assert.equal((await invoke('library_snapshot')).instances.length, 0);
  checks.push('unsafe and damaged archives never publish an instance');
  const preview = await invoke('pack_preview', { path: path.join(fixtures, 'valid.mrpack') });
  await show(preview);
  const optional = dialog.getByLabel('mods/optional.jar', { exact: true });
  await expect(optional).not.toBeChecked();
  await optional.check();
  await expect(optional).toBeChecked();
  await optional.uncheck();
  await expect(dialog.getByLabel('mods/server.jar', { exact: true })).toHaveCount(0);
  checks.push('optional client files are opt-in and dedicated server files cannot be selected');
  for (const width of [1400, 1000]) {
    await page.setViewportSize({ width, height: 900 });
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth),
      false,
    );
    await page.screenshot({ path: path.join(artifacts, `preview-${width}.png`) });
  }
  await dialog.getByLabel('Название новой сборки').fill('Native pack');
  await dialog.getByRole('button', { name: 'Создать и импортировать', exact: true }).click();
  const original = (await waitPack()).instanceId;
  const directory = path.join(dataDirectory, 'instances', original);
  assert.equal(await readFile(path.join(directory, 'options.txt'), 'utf8'), 'fixture:client');
  await assert.rejects(readFile(path.join(directory, 'config/server-only.txt')));
  const worldHash = await hash(path.join(directory, 'saves/World/level.dat'));
  checks.push('client overrides win and server overrides are excluded in a new isolated instance');
  const output = path.join(fixtures, 'roundtrip.sporium');
  const report = await invoke('pack_export', {
    request: { id: original, includeWorlds: true, includeLocal: false },
    path: output,
  });
  assert.equal(report.embeddedFiles, 3);
  const exportedHash = await hash(output);
  await assert.rejects(
    invoke('pack_export', {
      request: { id: original, includeWorlds: true, includeLocal: false },
      path: output,
    }),
  );
  assert.equal(await hash(output), exportedHash);
  const copy = await invoke('pack_preview', { path: output });
  await show(copy);
  await dialog.getByLabel('Название новой сборки').fill('Round trip copy');
  await dialog.getByRole('button', { name: 'Создать и импортировать', exact: true }).click();
  const copied = (await waitPack()).instanceId;
  assert.notEqual(copied, original);
  assert.equal(
    await hash(path.join(dataDirectory, 'instances', copied, 'saves/World/level.dat')),
    worldHash,
  );
  assert.equal(await hash(path.join(directory, 'saves/World/level.dat')), worldHash);
  checks.push('Sporium round-trip preserves worlds and refuses overwriting an existing export');
  const prism = path.join(fixtures, 'Prism');
  const originalConfigHash = await hash(path.join(prism, 'mmc-pack.json'));
  const candidates = await invoke('external_scan', { path: prism });
  const external = await invoke('external_preview', { key: candidates[0].key });
  await show(external);
  await dialog.getByRole('button', { name: 'Создать и импортировать', exact: true }).click();
  const imported = (await waitPack()).instanceId;
  assert.equal(await hash(path.join(prism, 'mmc-pack.json')), originalConfigHash);
  assert.equal(
    await readFile(path.join(prism, '.minecraft/accounts.json'), 'utf8'),
    'secret fixture',
  );
  await assert.rejects(readFile(path.join(dataDirectory, 'instances', imported, 'accounts.json')));
  checks.push('external instance copy preserves source metadata and excludes account secrets');
  await stop();
  await launch();
  dialog = page.getByRole('dialog', { name: 'Импорт сборки', exact: true });
  assert.equal((await invoke('library_snapshot')).instances.length, 3);
  assert.equal(await page.getByRole('dialog').count(), 0);
  checks.push('restart preserves imports without replaying a consumed open request');
  // Real provider pack, exact archived 1.21.1 release. Every download remains hash verified.
  const providerPreview = await invoke('provider_pack_preview', {
    projectId: '1KVo5zza',
    versionId: 'N276l2ON',
  });
  assert.equal(providerPreview.minecraft, '1.21.1');
  assert.equal(providerPreview.loader, 'fabric');
  assert(providerPreview.requiredFiles > 0);
  await show(providerPreview);
  await dialog.getByLabel('Название новой сборки').fill('Fabulously Optimized smoke');
  await page.screenshot({ path: path.join(artifacts, 'real-pack-preview.png') });
  await dialog.getByRole('button', { name: 'Создать и импортировать', exact: true }).click();
  liveId = (await waitPack()).instanceId;
  const installed = await invoke('installed_content', { id: liveId });
  assert(installed.length > 20);
  assert(installed.every((item) => item.record.provider === 'modrinth'));
  const provenance = JSON.parse(
    await readFile(path.join(dataDirectory, 'instances', liveId, '.sporium/import.json'), 'utf8'),
  );
  assert(provenance.providerRefs.some((ref) => ref.includes('1KVo5zza:N276l2ON')));
  checks.push('real Modrinth mrpack imports verified downloads and exact provider file provenance');
  const compact = path.join(fixtures, 'live.sporium');
  const liveReport = await invoke('pack_export', {
    request: { id: liveId, includeLocal: false, includeWorlds: false },
    path: compact,
  });
  assert(liveReport.referencedFiles > 20);
  const again = await invoke('pack_preview', { path: compact });
  await invoke('pack_import', { token: again.token, name: 'Provider round trip', optional: [] });
  const againId = (await waitPack()).instanceId;
  assert.equal((await invoke('installed_content', { id: againId })).length, installed.length);
  checks.push('compact provider export restores references and verified receipts');
  await page.evaluate((id) => {
    window.location.hash = `/instance/${id}`;
  }, liveId);
  await page.getByRole('button', { name: 'Играть', exact: true }).waitFor();
  await expect(page.getByRole('button', { name: 'Экспортировать', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'Экспортировать', exact: true }).click();
  const exportDialog = page.getByRole('dialog', { name: 'Экспорт сборки .sporium', exact: true });
  await expect(exportDialog.getByLabel('Включить миры', { exact: true })).not.toBeChecked();
  await exportDialog.getByRole('button', { name: 'Закрыть', exact: true }).click();
  await page.getByRole('button', { name: 'Играть', exact: true }).click();
  await expect
    .poll(
      async () => {
        const game = await invoke('game_state');
        if (game.job?.phase === 'failed') throw new Error(JSON.stringify(game.job));
        return game.sessions.some((s) => s.instanceId === liveId && s.running);
      },
      { timeout: 180000 },
    )
    .toBe(true);
  logPath = (await invoke('game_state')).sessions.find(
    (s) => s.instanceId === liveId && s.running,
  ).logPath;
  await expect
    .poll(
      async () =>
        /OpenAL initialized|Sound engine started|Created:.*atlas/i.test(
          await readFile(logPath, 'utf8').catch(() => ''),
        ),
      { timeout: 120000 },
    )
    .toBe(true);
  const log = await readFile(logPath, 'utf8');
  assert(!/Incompatible mod set|Mod resolution encountered an incompatible mod set/i.test(log));
  await invoke('stop_game', { id: liveId });
  await expect
    .poll(async () =>
      (await invoke('game_state')).sessions.some((s) => s.instanceId === liveId && s.running),
    )
    .toBe(false);
  assert.deepEqual(await invoke('installed_content', { id: liveId }), installed);
  checks.push(
    'imported real Fabric pack launches Minecraft to renderer/audio and clean stop preserves files',
  );
  assert.deepEqual(errors, []);
  await writeFile(
    path.join(artifacts, 'result.json'),
    JSON.stringify(
      { passed: true, checks, dataDirectory, liveId, logPath, date: new Date().toISOString() },
      null,
      2,
    ),
  );
  console.log(`Pack native smoke: ${checks.length} checks passed.`);
} catch (error) {
  await page
    ?.screenshot({ path: path.join(artifacts, 'failure.png'), fullPage: true })
    .catch(() => {});
  await writeFile(
    path.join(artifacts, 'result.json'),
    JSON.stringify(
      { passed: false, checks, dataDirectory, liveId, logPath, error: String(error) },
      null,
      2,
    ),
  );
  throw error;
} finally {
  if (liveId && session) await invoke('stop_game', { id: liveId }).catch(() => {});
  await stop();
}
