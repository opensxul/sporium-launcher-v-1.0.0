import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:net';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile, copyFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium, expect as baseExpect } from '@playwright/test';
const expect = baseExpect.configure({ timeout: 30_000 });
const workspace = path.resolve(import.meta.dirname, '..');
const artifacts = path.join(workspace, '.local/content-adoption-smoke');
const run = path.join(artifacts, `run-${Date.now()}`);
const dataDirectory = path.join(run, 'data');
const fixtures = path.join(run, 'files');
await mkdir(run, { recursive: true });
const fixtureProcess = spawn(
  'powershell.exe',
  [
    '-NoProfile',
    '-ExecutionPolicy',
    'Bypass',
    '-File',
    path.join(workspace, 'scripts/adoption-fixtures.ps1'),
    '-Directory',
    fixtures,
  ],
  { windowsHide: true },
);
assert.equal((await once(fixtureProcess, 'exit'))[0], 0);
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
  for (let attempt = 0; attempt < 120; attempt++) {
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
  page.setDefaultTimeout(30_000);
  page.on('pageerror', (error) => errors.push(error.message));
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.waitForSelector('.app-shell');
}
async function stop() {
  if (!session) return;
  await session.browser?.close();
  if (session.child.exitCode === null) {
    const exited = once(session.child, 'exit');
    session.child.kill();
    await exited;
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
      if (attempt >= 30 || !/LIBRARY_BUSY|INSTANCE_BUSY/.test(error.message)) throw error;
      await delay(100);
    }
  }
}
async function go() {
  await page.evaluate((id) => {
    window.location.hash = `/instance/${id}`;
  }, id);
  await page.getByRole('button', { name: 'Добавить из файла', exact: true }).waitFor();
}
const hash = async (filename) =>
  createHash('sha512')
    .update(await readFile(filename))
    .digest('hex');
const externalRow = (name) => page.locator('.untracked-files > li').filter({ hasText: name });
async function manage(name) {
  await externalRow(name).getByRole('button', { name: 'Управлять файлом', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  return dialog;
}
try {
  await launch();
  id = (
    await invoke('create_instance', {
      request: {
        name: 'Manual content probe',
        minecraftVersion: '1.21.1',
        loader: 'fabric',
        collectionId: null,
      },
    })
  ).affectedId;
  directory = path.join(dataDirectory, 'instances', id);
  await copyFile(path.join(fixtures, 'manual.jar'), path.join(directory, 'mods/manual.jar'));
  await copyFile(path.join(fixtures, 'unknown.jar'), path.join(directory, 'mods/unknown.jar'));
  await copyFile(path.join(fixtures, 'pack.zip'), path.join(directory, 'resourcepacks/pack.zip'));
  await writeFile(path.join(directory, 'mods/notes.txt'), 'preserve unmanaged notes');
  await mkdir(path.join(directory, 'saves/world'), { recursive: true });
  await writeFile(path.join(directory, 'saves/world/level.dat'), 'preserve world');
  const original = await hash(path.join(directory, 'mods/manual.jar'));
  await page.reload();
  await go();
  await expect(externalRow('Manual adoption')).toBeVisible();
  await expect
    .poll(async () =>
      externalRow('Manual adoption')
        .locator('img')
        .evaluateAll((imgs) => imgs.some((img) => img.complete && img.naturalWidth === 1)),
    )
    .toBe(true);
  await expect
    .poll(async () =>
      externalRow('pack.zip')
        .locator('img')
        .evaluateAll((imgs) => imgs.some((img) => img.complete && img.naturalWidth === 1)),
    )
    .toBe(true);
  assert.deepEqual(await invoke('installed_content', { id }), []);
  checks.push(
    'read-only inventory displays embedded local JAR and pack PNG icons without adopting files',
  );
  let dialog = await manage('Manual adoption');
  await dialog.getByRole('button', { name: 'Отмена', exact: true }).click();
  assert.deepEqual(await invoke('installed_content', { id }), []);
  assert.equal(await hash(path.join(directory, 'mods/manual.jar')), original);
  checks.push('cancelled adoption leaves original bytes and receipt unchanged');
  dialog = await manage('unknown.jar');
  await expect(
    dialog.getByRole('button', { name: 'Добавить в управляемый список', exact: true }),
  ).toBeDisabled();
  await dialog.locator('.content-warning input[type=checkbox]').check();
  await dialog.getByRole('button', { name: 'Добавить в управляемый список', exact: true }).click();
  await expect(externalRow('unknown.jar')).toHaveCount(0);
  dialog = await manage('Manual adoption');
  await dialog.getByRole('button', { name: 'Добавить в управляемый список', exact: true }).click();
  await expect(externalRow('Manual adoption')).toHaveCount(0);
  assert.equal(await hash(path.join(directory, 'mods/manual.jar')), original);
  await expect(
    page.locator('.content-records > li').filter({ hasText: 'Manual adoption' }).locator('img'),
  ).toBeVisible();
  checks.push(
    'explicit adoption handles known and unknown metadata, preserves bytes and local provenance',
  );
  dialog = await manage('pack.zip');
  await dialog.locator('.content-warning input[type=checkbox]').check();
  await dialog.getByRole('button', { name: 'Добавить в управляемый список', exact: true }).click();
  await expect(externalRow('pack.zip')).toHaveCount(0);
  checks.push(
    'manually placed resource-pack ZIP becomes managed without downloading or copying it',
  );
  for (const width of [1400, 1000]) {
    await page.setViewportSize({ width, height: 900 });
    await page.screenshot({ path: path.join(artifacts, `managed-${width}.png`), fullPage: true });
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth),
      false,
    );
  }
  await page.setViewportSize({ width: 1400, height: 900 });
  let row = page.locator('.content-records > li').filter({ hasText: 'Manual adoption' });
  await row.getByRole('switch').click();
  await page.getByRole('dialog').getByRole('button', { name: 'Подтвердить', exact: true }).click();
  await expect(row.getByRole('switch')).toHaveAttribute('aria-checked', 'false');
  assert.equal(await hash(path.join(directory, 'mods_disabled/manual.jar')), original);
  await stop();
  await launch();
  await go();
  row = page.locator('.content-records > li').filter({ hasText: 'Manual adoption' });
  await expect(row.getByRole('switch')).toHaveAttribute('aria-checked', 'false');
  await row.getByRole('switch').click();
  await page.getByRole('dialog').getByRole('button', { name: 'Подтвердить', exact: true }).click();
  await expect(row.getByRole('switch')).toHaveAttribute('aria-checked', 'true');
  checks.push('adopted mod toggle and disabled state survive restart with exact original bytes');
  const unknown = (await invoke('installed_content', { id })).find(
    (r) => r.record.file.filename === 'unknown.jar',
  );
  await invoke('change_content', {
    request: {
      instanceId: id,
      action: 'delete',
      files: [
        {
          directory: unknown.record.directory,
          filename: unknown.record.file.filename,
          sha512: unknown.record.file.hashes.sha512,
        },
      ],
    },
  });
  assert.equal(
    await readFile(path.join(directory, 'mods/notes.txt'), 'utf8'),
    'preserve unmanaged notes',
  );
  assert.equal(
    await readFile(path.join(directory, 'saves/world/level.dat'), 'utf8'),
    'preserve world',
  );
  assert.equal(
    (await invoke('content_history', { id })).filter((e) => e.action === 'adopt').length,
    3,
  );
  checks.push('managed deletion records history and preserves unrelated local files and worlds');
  // Read only from a previous isolated fixture; do not alter its instance or receipt.
  const prior = JSON.parse(
    await readFile(path.join(workspace, '.local/content-update-smoke/result.json'), 'utf8'),
  );
  const priorDirectory = path.join(prior.dataDirectory, 'instances', prior.id);
  const receipt = JSON.parse(
    await readFile(path.join(priorDirectory, '.sporium/content.json'), 'utf8'),
  );
  const apple = receipt.files.find((r) => r.title === 'AppleSkin');
  assert(apple);
  await copyFile(
    path.join(priorDirectory, apple.directory, apple.file.filename),
    path.join(directory, 'mods/renamed-appleskin.jar'),
  );
  await page.getByRole('button', { name: 'Обновить список контента', exact: true }).click();
  dialog = await manage('AppleSkin');
  await dialog.getByRole('button', { name: 'Распознать в Modrinth', exact: true }).click();
  await expect(dialog.getByText('Modrinth', { exact: true })).toBeVisible({ timeout: 60_000 });
  await dialog.locator('.local-plan-files').getByText('Подробнее', { exact: true }).click();
  await expect(dialog).toContainText('Заявленные зависимости');
  assert(
    !(await invoke('installed_content', { id })).some(
      (r) => r.record.projectId === apple.projectId,
    ),
  );
  for (const width of [1400, 1000]) {
    await page.setViewportSize({ width, height: 900 });
    await page.screenshot({ path: path.join(artifacts, `match-${width}.png`), fullPage: true });
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth),
      false,
    );
  }
  await page.setViewportSize({ width: 1400, height: 900 });
  if (await dialog.locator('.content-warning input[type=checkbox]').count())
    await dialog.locator('.content-warning input[type=checkbox]').check();
  await dialog.getByRole('button', { name: 'Добавить в управляемый список', exact: true }).click();
  await expect(externalRow('AppleSkin')).toHaveCount(0);
  const matched = (await invoke('installed_content', { id })).find(
    (r) => r.record.projectId === apple.projectId,
  ).record;
  assert.equal(matched.provider, 'modrinth');
  assert.equal(matched.version.id, apple.version.id);
  assert.equal(matched.file.filename, 'renamed-appleskin.jar');
  assert.equal(matched.file.hashes.sha512, apple.file.hashes.sha512);
  assert.equal(
    await hash(path.join(directory, 'mods/renamed-appleskin.jar')),
    apple.file.hashes.sha512,
  );
  const updates = await invoke('content_updates', { id });
  assert.notEqual(updates.find((u) => u.projectId === apple.projectId)?.status, 'local');
  checks.push(
    'live official SHA512 identification validates SHA1/size and preserves renamed filename, provenance and update eligibility',
  );
  await stop();
  await launch();
  const after = await invoke('installed_content', { id });
  assert.equal(
    after.find((r) => r.record.file.filename === 'renamed-appleskin.jar').record.provider,
    'modrinth',
  );
  assert.equal(after.length, 3);
  assert.deepEqual(errors, []);
  checks.push(
    'local and matched Modrinth receipts and adoption history persist after final restart',
  );
  await writeFile(
    path.join(artifacts, 'result.json'),
    JSON.stringify(
      { passed: true, checks, id, dataDirectory, date: new Date().toISOString() },
      null,
      2,
    ),
  );
  console.log(JSON.stringify({ passed: true, checks }, null, 2));
} catch (error) {
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
