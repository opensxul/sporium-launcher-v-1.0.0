import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:net';
import { mkdir, mkdtemp, readFile, writeFile, cp } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium, expect as baseExpect } from '@playwright/test';
const expect = baseExpect.configure({ timeout: 90000 });
const workspace = path.resolve(import.meta.dirname, '..');
const artifacts = path.join(workspace, '.local/project-native-smoke');
await mkdir(artifacts, { recursive: true });
const dataDirectory = await mkdtemp(path.join(artifacts, 'data-'));
let child, browser, page, id;
const checks = [],
  errors = [];
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
  page = browser.contexts()[0].pages()[0];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.waitForSelector('.app-shell');
}
async function stop() {
  await browser?.close();
  browser = null;
  if (child?.exitCode === null) {
    const exited = once(child, 'exit');
    child.kill();
    await exited;
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
      if (attempt >= 200 || !/LIBRARY_BUSY|INSTANCE_BUSY/.test(error.message)) throw error;
      await delay(250);
    }
  }
}
const hash = (bytes, type) => createHash(type).update(bytes).digest('hex');
async function policy(content = 'off', project = 'off') {
  await invoke('save_automatic_policy', {
    value: { schemaVersion: 1, content, project, instances: [] },
  });
}
async function applyDialog() {
  const dialog = page.getByRole('dialog');
  const consent = dialog.getByRole('checkbox', { name: /Разрешаю перечисленные/ });
  if (await consent.count()) await consent.check();
  await dialog.getByRole('button', { name: 'Применить изменения', exact: true }).click();
  await expect(dialog).toHaveCount(0);
}
try {
  await launch();
  await policy();
  id = (
    await invoke('create_instance', {
      request: {
        name: 'Project native probe',
        minecraftVersion: '1.21.1',
        loader: 'vanilla',
        collectionId: null,
      },
    })
  ).affectedId;
  const directory = path.join(dataDirectory, 'instances', id);
  await writeFile(path.join(directory, 'config/managed.toml'), 'initial');
  await writeFile(path.join(directory, 'options.txt'), 'user:before');
  await mkdir(path.join(directory, 'saves/World'), { recursive: true });
  await writeFile(path.join(directory, 'saves/World/level.dat'), 'world unchanged');
  await writeFile(path.join(directory, 'screenshots/user.png'), 'screenshot unchanged');
  await stop();
  await launch();
  await page.evaluate((id) => {
    window.location.hash = `/instance/${id}?tab=settings`;
  }, id);
  await page.getByRole('button', { name: 'Создать проект', exact: true }).click();
  let dialog = page.getByRole('dialog');
  await dialog.getByLabel('Название проекта', { exact: true }).fill('Creator native');
  await dialog.getByLabel('Версия проекта', { exact: true }).fill('1.0');
  await dialog.locator('summary').click();
  await dialog
    .getByLabel('Правило файла: config/managed.toml', { exact: true })
    .selectOption('REQUIRED_LOCKED');
  await dialog.getByRole('button', { name: 'Посмотреть изменения', exact: true }).click();
  await expect(dialog.getByRole('heading', { name: 'Изменения сборки' })).toBeVisible();
  assert.equal((await invoke('project_view', { id })).manifest, null);
  checks.push('Creator Studio preview does not mutate live files or manifest');
  for (const width of [1400, 1000]) {
    await page.setViewportSize({ width, height: 900 });
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth),
      false,
    );
    await page.screenshot({ path: path.join(artifacts, `project-plan-${width}.png`) });
  }
  await applyDialog();
  assert.equal((await invoke('project_view', { id })).manifest.version, '1.0');
  assert.equal(
    (await invoke('library_snapshot')).instances.find((item) => item.id === id).instanceType,
    'creator_studio',
  );
  checks.push('Native confirmation commits project and library type');
  await writeFile(path.join(directory, 'config/managed.toml'), 'manual change');
  await page.getByRole('button', { name: 'Восстановить файлы проекта', exact: true }).click();
  await page.getByRole('dialog').locator('summary').click();
  await expect(page.getByRole('dialog').getByText(/Файл изменён вручную/)).toBeVisible();
  await applyDialog();
  assert.equal(await readFile(path.join(directory, 'config/managed.toml'), 'utf8'), 'initial');
  const points = await invoke('content_restore_points', { id });
  assert(points.some((point) => point.projectPoint && point.settingsAvailable));
  await writeFile(path.join(directory, 'options.txt'), 'user:after');
  await page.getByRole('tab', { name: 'История', exact: true }).click();
  await page.getByRole('button', { name: 'Точки восстановления', exact: true }).click();
  dialog = page.getByRole('dialog');
  await dialog.locator('input[type=radio]').first().check();
  await dialog.getByRole('checkbox', { name: /Также восстановить сохранённые настройки/ }).check();
  await dialog.getByRole('button', { name: 'Восстановить', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  assert.equal(
    await readFile(path.join(directory, 'config/managed.toml'), 'utf8'),
    'manual change',
  );
  assert.equal(await readFile(path.join(directory, 'options.txt'), 'utf8'), 'user:before');
  let plan = await invoke('project_repair_plan', { id });
  await invoke('project_apply', { token: plan.token, acceptChanges: true });
  checks.push('Repair and configuration rollback preserve snapshot bytes and user worlds');
  const source = path.join(dataDirectory, 'launcher/native-project.json');
  const manifest = (await invoke('project_view', { id })).manifest;
  const replacement = Buffer.from('managed second version');
  const file = manifest.files.find((file) => file.path === 'config/managed.toml');
  Object.assign(file, {
    sha256: hash(replacement, 'sha256'),
    sha512: hash(replacement, 'sha512'),
    size: replacement.length,
  });
  manifest.version = '2.0';
  await writeFile(
    path.join(dataDirectory, 'launcher/project-sources', `${file.sha512}.bin`),
    replacement,
  );
  await writeFile(source, JSON.stringify(manifest));
  await writeFile(
    path.join(directory, '.sporium/project-source.json'),
    JSON.stringify({ path: source }),
  );
  await policy('off', 'check');
  const checked = await invoke('automatic_check', { id });
  assert.equal(checked.project.status, 'available');
  assert.equal((await invoke('project_view', { id })).manifest.version, '1.0');
  await policy('off', 'install');
  const updated = await invoke('automatic_check', { id });
  assert.equal(updated.status, 'project_installed');
  assert.equal((await invoke('project_view', { id })).manifest.version, '2.0');
  assert.equal(
    await readFile(path.join(directory, 'config/managed.toml'), 'utf8'),
    replacement.toString(),
  );
  checks.push(
    'Check-only leaves bytes untouched and explicitly enabled compatible auto-update commits',
  );
  await writeFile(source, 'unavailable source');
  assert.equal((await invoke('project_check', { id })).status, 'unavailable');
  assert.equal(
    await readFile(path.join(directory, 'saves/World/level.dat'), 'utf8'),
    'world unchanged',
  );
  assert.equal(
    await readFile(path.join(directory, 'screenshots/user.png'), 'utf8'),
    'screenshot unchanged',
  );
  await policy();
  await stop();
  await launch();
  assert.equal((await invoke('project_view', { id })).manifest.version, '2.0');
  assert.equal((await invoke('automatic_policy')).project, 'off');
  checks.push('Unavailable source and restart preserve project, update policy and user files');
  await page.evaluate((id) => {
    window.location.hash = `/instance/${id}?tab=settings`;
  }, id);
  await page.locator('.project-auto > summary').click();
  await expect(
    page.locator('.project-panel').getByText('Creator native', { exact: true }),
  ).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Автоматические обновления' })).toBeVisible();
  await page.getByLabel('Отдельные моды и другой контент', { exact: true }).selectOption('install');
  const savePolicy = page.getByRole('button', {
    name: 'Сохранить правила обновления',
    exact: true,
  });
  await expect(savePolicy).toBeDisabled();
  await page.getByRole('checkbox', { name: /Разрешаю автоматическую установку/ }).check();
  await savePolicy.click();
  await expect(page.getByRole('status').filter({ hasText: 'Правила сохранены' })).toBeVisible();
  assert.equal(
    (await invoke('automatic_policy')).instances.find((item) => item.id === id).content,
    'install',
  );
  checks.push('Automatic installation requires visible consent and persists an instance override');
  for (const width of [1400, 1000]) {
    await page.setViewportSize({ width, height: 900 });
    await page.locator('.project-panel').scrollIntoViewIfNeeded();
    await page.screenshot({ path: path.join(artifacts, `project-${width}.png`) });
  }
  checks.push('Instance project and automatic policy UI render at desktop and narrow widths');
  await policy();
  // Import an official pack into this isolated library using existing verified download caches.
  console.log(`${checks.length} native project checks passed; importing official pack`);
  const previous = JSON.parse(
    await readFile(path.join(workspace, '.local/pack-native-smoke/result.json'), 'utf8'),
  );
  assert(
    previous.passed &&
      path.resolve(previous.dataDirectory).startsWith(path.join(workspace, '.local') + path.sep),
  );
  await cp(
    path.join(previous.dataDirectory, 'shared/cache'),
    path.join(dataDirectory, 'shared/cache'),
    { recursive: true, force: false, errorOnExist: false },
  );
  const preview = await invoke('provider_pack_preview', {
    projectId: '1KVo5zza',
    versionId: 'N276l2ON',
  });
  await invoke('pack_import', {
    token: preview.token,
    name: 'Managed official pack',
    optional: [],
  });
  let packJob;
  for (let attempt = 0; attempt < 600; attempt++) {
    packJob = await invoke('pack_state');
    if (['completed', 'failed', 'cancelled'].includes(packJob?.phase)) break;
    await delay(500);
  }
  assert.equal(packJob.phase, 'completed', JSON.stringify(packJob));
  const packId = packJob.instanceId;
  console.log(`Official pack imported: ${packId}; preparing managed repair`);
  const packDir = path.join(dataDirectory, 'instances', packId);
  const before = await readFile(path.join(packDir, '.sporium/content.json'));
  const packPlan = await invoke('project_repair_plan', { id: packId });
  console.log(`Managed repair prepared: ${packPlan.changes.length} file changes`);
  await invoke('project_apply', { token: packPlan.token, acceptChanges: true });
  const packView = await invoke('project_view', { id: packId });
  assert.equal(packView.manifest.packSource.projectId, '1KVo5zza');
  assert(packView.manifest.files.length >= 50);
  const after = JSON.parse(await readFile(path.join(packDir, '.sporium/content.json'), 'utf8'));
  assert.equal(after.files.length, JSON.parse(before).files.length);
  assert(
    ['current', 'available', 'incompatible', 'unavailable'].includes(
      (await invoke('project_check', { id: packId })).status,
    ),
  );
  checks.push('Real Modrinth pack import, managed repair, provenance and whole-pack update check');
  assert.deepEqual(errors, []);
  await writeFile(
    path.join(artifacts, 'result.json'),
    JSON.stringify(
      { passed: true, checks, dataDirectory, id, packId, date: new Date().toISOString() },
      null,
      2,
    ),
  );
  console.log(JSON.stringify({ passed: true, checks, dataDirectory, id, packId }, null, 2));
} catch (error) {
  await page?.screenshot({ path: path.join(artifacts, 'failure.png') }).catch(() => {});
  await writeFile(
    path.join(artifacts, 'result.json'),
    JSON.stringify(
      {
        passed: false,
        checks,
        error: String(error),
        dataDirectory,
        id,
        date: new Date().toISOString(),
      },
      null,
      2,
    ),
  );
  throw error;
} finally {
  await stop();
}
