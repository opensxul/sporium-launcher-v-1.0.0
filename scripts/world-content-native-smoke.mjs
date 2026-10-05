// Real native WebView2 + IPC. Local archives use real planner/confirmation IPC;
// the user-confirmed OS picker/drop boundary is not automated again.
// Synthetic level.dat files validate import/preservation, not Minecraft world playability.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:net';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile, copyFile, readdir, cp } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium, expect as baseExpect } from '@playwright/test';
const expect = baseExpect.configure({ timeout: 60_000 });
const workspace = path.resolve(import.meta.dirname, '..');
const artifacts = path.join(workspace, '.local/world-content-smoke');
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
    path.join(workspace, 'scripts/world-content-fixtures.ps1'),
    '-Directory',
    fixtures,
  ],
  { windowsHide: true },
);
assert.equal((await once(fixture, 'exit'))[0], 0);
let session, page, id, directory;
const errors = [],
  checks = [];
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
  page.setDefaultTimeout(60_000);
  page.on('pageerror', (error) => errors.push(error.message));
  await page.setViewportSize({ width: 1400, height: 900 });
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
      if (attempt >= 40 || !/LIBRARY_BUSY|INSTANCE_BUSY/.test(error.message)) throw error;
      await delay(100);
    }
  }
}
async function go(target = id) {
  await page.evaluate((value) => {
    window.location.hash = `/instance/${value}`;
  }, target);
  await page.getByRole('button', { name: 'Добавить карту или датапак', exact: true }).waitFor();
}
const hash = async (file) =>
  createHash('sha512')
    .update(await readFile(file))
    .digest('hex');
async function worldDialog(kind, title, world = '') {
  await page.getByRole('button', { name: 'Добавить карту или датапак', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('Тип контента', { exact: true }).selectOption(kind);
  await dialog.getByLabel('Название', { exact: true }).fill(title);
  if (kind === 'datapack') {
    await expect(dialog.getByRole('button', { name: 'Выбрать ZIP', exact: true })).toBeDisabled();
    await dialog.getByLabel('Мир назначения', { exact: true }).selectOption(world);
  }
  await expect(dialog.getByRole('button', { name: 'Выбрать ZIP', exact: true })).toBeEnabled();
  await dialog.getByRole('button', { name: 'Отмена', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  const plan = await invoke('world_archive_plan', {
    request: {
      instanceId: id,
      source: path.join(fixtures, kind === 'map' ? 'map.zip' : 'datapack.zip'),
      kind,
      title,
      world: world || null,
    },
  });
  await assert.rejects(
    invoke('finish_world_archive', { token: plan.token, acceptUnknown: false, cancel: false }),
    /CONTENT_INCOMPATIBLE/,
  );
  return plan;
}
async function acceptWorld(plan) {
  await invoke('finish_world_archive', { token: plan.token, acceptUnknown: true, cancel: false });
  await page.reload();
  await go();
}
async function waitInstall(target) {
  for (let i = 0; i < 3600; i++) {
    const job = await invoke('content_state');
    if (job?.instanceId === target && job.phase === 'completed') return;
    if (job && ['failed', 'cancelled'].includes(job.phase)) throw new Error(JSON.stringify(job));
    await delay(250);
  }
  throw new Error('Content job timeout');
}
try {
  const previous = await readFile(path.join(artifacts, 'result.json'), 'utf8')
    .then(JSON.parse)
    .catch(() => null);
  if (
    previous?.passed &&
    path.resolve(previous.dataDirectory).startsWith(`${path.resolve(artifacts)}${path.sep}`)
  )
    await cp(path.join(previous.dataDirectory, 'shared'), path.join(dataDirectory, 'shared'), {
      recursive: true,
    });
  await launch();
  id = (
    await invoke('create_instance', {
      request: {
        name: 'Phase 10 native probe',
        minecraftVersion: '1.21.1',
        loader: 'fabric',
        collectionId: null,
      },
    })
  ).affectedId;
  directory = path.join(dataDirectory, 'instances', id);
  for (const world of ['one', 'two']) {
    await mkdir(path.join(directory, 'saves', world), { recursive: true });
    await writeFile(path.join(directory, 'saves', world, 'level.dat'), `preserve ${world}`);
  }
  await page.reload();
  await go();
  const originalOne = await hash(path.join(directory, 'saves/one/level.dat')),
    originalTwo = await hash(path.join(directory, 'saves/two/level.dat'));
  let dialog = await worldDialog('map', 'Imported native map');
  await invoke('finish_world_archive', { token: dialog.token, acceptUnknown: false, cancel: true });
  assert.equal((await invoke('content_worlds', { id })).length, 2);
  assert.deepEqual(await invoke('content_history', { id }), []);
  checks.push(
    'map ZIP review and cancellation leave worlds and history unchanged; compatibility acceptance is explicit',
  );
  dialog = await worldDialog('map', 'Imported native map');
  await acceptWorld(dialog);
  const imported = (await invoke('content_worlds', { id })).find((w) => w.imported);
  assert(imported);
  assert.equal(imported.title, 'Imported native map');
  assert.equal(imported.imported.source, 'map.zip');
  assert.equal(
    await readFile(path.join(directory, 'saves', imported.id, 'region/r.0.0.mca'), 'utf8'),
    'preserved region fixture',
  );
  await expect(page.locator('.world-list')).toContainText('Imported native map');
  checks.push(
    'wrapper map publishes a new isolated world with exact file bytes, title, source and history',
  );
  dialog = await worldDialog('datapack', 'Native data pack', 'one');
  assert.equal(dialog.world, 'one');
  await acceptWorld(dialog);
  let rows = await invoke('installed_content', { id });
  assert.equal(rows.length, 1);
  assert.equal(rows[0].record.directory, 'saves/one/datapacks');
  assert.equal(rows[0].record.provider, 'local');
  assert.equal(
    await hash(path.join(directory, 'saves/one/datapacks/datapack.zip')),
    await hash(path.join(fixtures, 'datapack.zip')),
  );
  await expect(page.locator('.content-installed')).toContainText('Мир назначения: one');
  checks.push(
    'local datapack requires a chosen real world and explicit warnings acceptance, preserves the ZIP and managed local provenance',
  );
  for (const width of [1400, 1000]) {
    await page.setViewportSize({ width, height: 900 });
    await page.screenshot({ path: path.join(artifacts, `worlds-${width}.png`), fullPage: true });
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth),
      false,
    );
  }
  checks.push('world and datapack views fit 1400px and 1000px without horizontal overflow');
  await stop();
  await launch();
  await go();
  assert.equal((await invoke('content_worlds', { id })).length, 3);
  assert.equal((await invoke('installed_content', { id })).length, 1);
  assert(
    (await invoke('content_history', { id })).some((event) => event.action === 'import_world'),
  );
  checks.push('world imports, datapack receipt and history persist across native restart');
  await invoke('change_content', {
    request: {
      instanceId: id,
      action: 'delete',
      files: rows.map(({ record }) => ({
        directory: record.directory,
        filename: record.file.filename,
        sha512: record.file.hashes.sha512,
      })),
    },
  });
  assert.equal(await hash(path.join(directory, 'saves/one/level.dat')), originalOne);
  assert.equal(await hash(path.join(directory, 'saves/two/level.dat')), originalTwo);
  assert.equal((await readdir(path.join(directory, 'saves/two'))).length, 1);
  checks.push(
    'datapack deletion removes only its ZIP, preserving chosen world, other world and original input',
  );

  // Live provider path: context locks game version but uses datapack platform rather than Fabric.
  await page.reload();
  await go();
  await page.getByRole('button', { name: 'Добавить контент', exact: true }).click();
  const catalog = page.getByRole('dialog').first();
  await catalog.getByRole('button', { name: 'Датапаки', exact: true }).click();
  await catalog.getByRole('searchbox').fill('Terralith');
  await catalog.getByRole('button', { name: 'Найти', exact: true }).click();
  const hit = catalog
    .locator('.catalog-result')
    .filter({ has: page.getByRole('heading', { name: 'Terralith', exact: true }) })
    .first();
  await expect(hit).toBeVisible();
  await hit.getByRole('button', { name: 'Установить', exact: true }).click();
  dialog = page.getByRole('dialog').last();
  await dialog.getByLabel('Мир назначения', { exact: true }).waitFor();
  await expect(
    dialog.getByRole('button', { name: 'Проверить зависимости', exact: true }),
  ).toBeDisabled();
  await dialog.getByLabel('Мир назначения', { exact: true }).selectOption('two');
  await dialog.getByRole('button', { name: 'Проверить зависимости', exact: true }).click();
  await expect(dialog.locator('.content-plan')).toContainText('Мир назначения: two');
  await dialog.getByRole('button', { name: 'Установить выбранное', exact: true }).click();
  await waitInstall(id);
  rows = await invoke('installed_content', { id });
  const remoteData = rows.find((r) => r.record.kind === 'datapack');
  assert(remoteData);
  assert.equal(remoteData.record.provider, 'modrinth');
  assert.equal(remoteData.record.directory, 'saves/two/datapacks');
  assert(remoteData.record.version.loaders.includes('datapack'));
  assert(remoteData.record.version.game_versions.includes('1.21.1'));
  assert.equal(
    await hash(path.join(directory, remoteData.record.directory, remoteData.record.file.filename)),
    remoteData.record.file.hashes.sha512,
  );
  checks.push(
    'live contextual Modrinth datapack search filters concrete datapack versions, requires explicit world, and installs verified official bytes there',
  );
  await hit.getByRole('button', { name: 'Мир назначения', exact: true }).click();
  dialog = page.getByRole('dialog').last();
  await dialog.getByLabel('Мир назначения', { exact: true }).selectOption('one');
  await dialog.getByRole('button', { name: 'Проверить зависимости', exact: true }).click();
  await expect(dialog.locator('.content-plan')).toContainText('Мир назначения: one');
  await dialog.getByRole('button', { name: 'Установить выбранное', exact: true }).click();
  await waitInstall(id);
  assert.deepEqual(
    (await invoke('installed_content', { id }))
      .filter((r) => r.record.kind === 'datapack')
      .map((r) => r.record.directory)
      .sort(),
    ['saves/one/datapacks', 'saves/two/datapacks'],
  );
  checks.push(
    'an installed datapack remains selectable for a second world without changing the first world copy',
  );
  // Load a known real Mod Menu artifact, renamed. Previous native data is only read.
  const prior = JSON.parse(
    await readFile(path.join(workspace, '.local/modrinth-native-smoke/result.json'), 'utf8'),
  );
  const receipt = JSON.parse(
    await readFile(
      path.join(prior.dataDirectory, 'instances', prior.instanceId, '.sporium/content.json'),
      'utf8',
    ),
  );
  const parent = receipt.files.find((r) => r.projectId === 'mOgUt4GM');
  assert(parent);
  const localSource = path.join(fixtures, 'renamed-modmenu.jar');
  await copyFile(
    path.join(
      prior.dataDirectory,
      'instances',
      prior.instanceId,
      parent.directory,
      parent.file.filename,
    ),
    localSource,
  );
  const parentHash = await hash(localSource);
  await page.reload();
  await go();
  let importedLocal = await invoke('local_content_plan', { id, paths: [localSource] });
  importedLocal = await invoke('local_content_dependencies', { token: importedLocal.plan.token });
  assert.equal(importedLocal.plan.files.length, 3);
  assert(importedLocal.plan.files.every((r) => r.provider === 'modrinth'));
  await invoke('finish_local_content', {
    token: importedLocal.plan.token,
    acceptUnknown: false,
    cancel: true,
  });
  assert((await invoke('installed_content', { id })).every((r) => r.record.kind !== 'mod'));
  assert.equal(await hash(localSource), parentHash);
  checks.push(
    'live SHA512 matching finds required Mod Menu dependencies in preview and cancellation publishes no mods',
  );
  importedLocal = await invoke('local_content_plan', { id, paths: [localSource] });
  importedLocal = await invoke('local_content_dependencies', { token: importedLocal.plan.token });
  assert.equal(importedLocal.plan.files.length, 3);
  await invoke('finish_local_content', {
    token: importedLocal.plan.token,
    acceptUnknown: true,
    cancel: false,
  });
  rows = await invoke('installed_content', { id });
  const mods = rows.filter((r) => r.record.kind === 'mod');
  assert.equal(mods.length, 3);
  assert(mods.every((r) => r.record.provider === 'modrinth'));
  assert.equal(await hash(path.join(directory, 'mods/renamed-modmenu.jar')), parentHash);
  checks.push(
    'confirmed matched local import publishes original renamed JAR plus required compatible provider dependencies as one managed batch',
  );
  const second = (
    await invoke('create_instance', {
      request: {
        name: 'Local provenance dependency probe',
        minecraftVersion: '1.21.1',
        loader: 'fabric',
        collectionId: null,
      },
    })
  ).affectedId;
  const localPlan = await invoke('local_content_plan', { id: second, paths: [localSource] });
  await invoke('finish_local_content', {
    token: localPlan.plan.token,
    acceptUnknown: true,
    cancel: false,
  });
  await page.reload();
  await go(second);
  await page.getByRole('button', { name: 'Установить зависимости', exact: true }).click();
  dialog = page.getByRole('dialog');
  await expect(dialog.locator('.local-plan-files > li')).toHaveCount(2);
  await dialog.getByRole('button', { name: 'Установить всё', exact: true }).click();
  await waitInstall(second);
  const secondRows = await invoke('installed_content', { id: second });
  assert.equal(secondRows.length, 3);
  assert.equal(
    secondRows.find((r) => r.record.file.filename === 'renamed-modmenu.jar').record.provider,
    'local',
  );
  checks.push(
    'managed local dependency action verifies provider identity and installs remote dependencies while keeping root local provenance',
  );
  await stop();
  await launch();
  assert.equal((await invoke('installed_content', { id })).length, 5);
  assert.equal((await invoke('installed_content', { id: second })).length, 3);
  assert.equal(await hash(path.join(directory, 'saves/one/level.dat')), originalOne);
  assert.equal(await hash(path.join(directory, 'saves/two/level.dat')), originalTwo);
  assert.equal(await hash(localSource), parentHash);
  assert.deepEqual(errors, []);
  checks.push(
    'mixed provider/local content and per-world datapacks survive restart; all original source/world bytes are preserved',
  );
  await writeFile(
    path.join(artifacts, 'result.json'),
    JSON.stringify(
      { passed: true, checks, id, second, dataDirectory, date: new Date().toISOString() },
      null,
      2,
    ),
  );
  console.log(JSON.stringify({ passed: true, checks }, null, 2));
} catch (error) {
  if (page)
    await writeFile(
      path.join(artifacts, 'failure-ui.txt'),
      await page
        .locator('body')
        .innerText()
        .catch(() => ''),
    );
  await page
    ?.screenshot({ path: path.join(artifacts, 'failure.png'), fullPage: true })
    .catch(() => {});
  await writeFile(
    path.join(artifacts, 'result.json'),
    JSON.stringify({ passed: false, checks, id, dataDirectory, error: String(error) }, null, 2),
  );
  throw error;
} finally {
  await stop();
}
