import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:net';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile, copyFile, rename, unlink } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium, expect as baseExpect } from '@playwright/test';
const expect = baseExpect.configure({ timeout: 30_000 });
const workspace = path.resolve(import.meta.dirname, '..');
const artifacts = path.join(workspace, '.local/content-diagnostics-smoke');
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
    path.join(workspace, 'scripts/diagnostics-fixtures.ps1'),
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
  await page.getByRole('button', { name: 'Проверить зависимости', exact: true }).waitFor();
}
async function diagnostics() {
  await page.getByRole('button', { name: 'Проверить зависимости', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.locator('.dependency-report')).toBeVisible();
  return dialog;
}
const close = (dialog) =>
  dialog.locator('.modal-actions').getByRole('button', { name: 'Закрыть', exact: true }).click();
const report = () => invoke('content_diagnostics', { id });
const check = (report, file, dep) =>
  report.mods.find((m) => m.filename === file).checks.find((c) => c.dependency.id === dep);
const hash = async (filename) =>
  createHash('sha512')
    .update(await readFile(filename))
    .digest('hex');
const copy = (from, to = `mods/${from}`) =>
  copyFile(path.join(fixtures, from), path.join(directory, to));
try {
  await launch();
  id = (
    await invoke('create_instance', {
      request: {
        name: 'Dependency diagnostics probe',
        minecraftVersion: '1.21.1',
        loader: 'fabric',
        collectionId: null,
      },
    })
  ).affectedId;
  directory = path.join(dataDirectory, 'instances', id);
  for (const file of ['owner.jar', 'wrong_lib.jar', 'bad_lib.jar', 'soft_lib.jar'])
    await copy(file);
  await copy('disabled_lib.jar', 'mods_disabled/disabled_lib.jar');
  await mkdir(path.join(directory, 'saves/world'), { recursive: true });
  await writeFile(path.join(directory, 'saves/world/level.dat'), 'preserve diagnostic world');
  const original = await hash(path.join(directory, 'mods/owner.jar'));
  await page.reload();
  await go();
  const first = await report();
  assert(first.complete);
  assert.equal(first.errors, 4);
  for (const [dep, status] of [
    ['fabric-api', 'missing'],
    ['disabled_lib', 'disabled'],
    ['wrong_lib', 'version_mismatch'],
    ['bad_lib', 'conflict'],
    ['soft_lib', 'conflict'],
    ['optional_lib', 'optional'],
  ])
    assert.equal(check(first, 'owner.jar', dep).status, status);
  assert.equal(check(first, 'owner.jar', 'optional_lib').severity, 'info');
  assert.deepEqual(await invoke('installed_content', { id }), []);
  assert.deepEqual(await invoke('content_history', { id }), []);
  checks.push(
    'offline scan distinguishes missing, disabled, wrong versions, hard/soft conflicts and optional dependencies without changing files or receipts',
  );

  let dialog = await diagnostics();
  for (const status of ['missing', 'disabled', 'version_mismatch', 'conflict'])
    await expect(
      dialog.locator(`.diagnostic-checks > li[data-status="${status}"]`).first(),
    ).toBeVisible();
  await expect(dialog.locator('[data-status="optional"]')).toHaveCount(0);
  await dialog
    .getByRole('checkbox', { name: 'Только проблемы и предупреждения', exact: true })
    .uncheck();
  await expect(dialog.locator('[data-status="optional"]')).toHaveCount(1);
  for (const width of [1400, 1000]) {
    await page.setViewportSize({ width, height: 900 });
    await page.screenshot({
      path: path.join(artifacts, `diagnostics-${width}.png`),
      fullPage: true,
    });
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth),
      false,
    );
    assert.equal(await dialog.evaluate((el) => el.scrollWidth > el.clientWidth), false);
  }
  await page.setViewportSize({ width: 1400, height: 900 });
  await dialog
    .locator('[data-status="missing"]')
    .filter({ hasText: 'fabric-api' })
    .getByRole('button', { name: 'Найти в Modrinth', exact: true })
    .click();
  dialog = page.getByRole('dialog');
  await expect(dialog.locator('.catalog-search input')).toHaveValue('fabric-api');
  await expect(dialog).toContainText('1.21.1');
  await expect(dialog).toContainText('Fabric');
  await dialog
    .locator('.modal-header')
    .getByRole('button', { name: 'Закрыть', exact: true })
    .click();
  assert.deepEqual(await invoke('installed_content', { id }), []);
  checks.push(
    'native dialog filters findings, shows optional requirements on demand and opens contextual Modrinth search without installing or guessing project identity',
  );

  await page
    .locator('.untracked-files > li')
    .filter({ hasText: 'Dependency probe' })
    .getByRole('button', { name: 'Управлять файлом', exact: true })
    .click();
  dialog = page.getByRole('dialog');
  await expect(dialog.locator('.dependency-report [data-status="missing"]')).toHaveCount(1);
  await expect(
    dialog.getByRole('button', { name: 'Добавить в управляемый список', exact: true }),
  ).toBeDisabled();
  await dialog.locator('.content-warning input[type=checkbox]').check();
  await dialog.getByRole('button', { name: 'Добавить в управляемый список', exact: true }).click();
  await expect(
    page.locator('.content-records > li').filter({ hasText: 'Dependency probe' }),
  ).toBeVisible();
  assert.equal(await hash(path.join(directory, 'mods/owner.jar')), original);
  const receipt = await readFile(path.join(directory, '.sporium/content.json'), 'utf8');
  const history = await invoke('content_history', { id });
  checks.push(
    'adoption preview includes exact dependency findings and requires explicit warning acceptance while preserving original bytes',
  );

  dialog = await diagnostics();
  await copy('api.jar');
  await copy('wrong_lib_fixed.jar', 'mods/wrong_lib.jar');
  await rename(
    path.join(directory, 'mods_disabled/disabled_lib.jar'),
    path.join(directory, 'mods/disabled_lib.jar'),
  );
  await rename(
    path.join(directory, 'mods/bad_lib.jar'),
    path.join(directory, 'mods_disabled/bad_lib.jar'),
  );
  await rename(
    path.join(directory, 'mods/soft_lib.jar'),
    path.join(directory, 'mods_disabled/soft_lib.jar'),
  );
  await dialog
    .locator('.modal-actions')
    .getByRole('button', { name: 'Обновить список контента', exact: true })
    .click();
  await expect(dialog.locator('.dependency-report')).toBeVisible();
  await expect(dialog.locator('.diagnostic-error')).toHaveCount(0);
  const fixed = await report();
  assert.equal(fixed.errors, 0);
  for (const dep of ['fabric-api', 'disabled_lib', 'wrong_lib'])
    assert.equal(check(fixed, 'owner.jar', dep).status, 'satisfied');
  assert.equal(await readFile(path.join(directory, '.sporium/content.json'), 'utf8'), receipt);
  assert.deepEqual(await invoke('content_history', { id }), history);
  await close(dialog);
  checks.push(
    'refresh re-inspects actual manually changed files and disabled state without rewriting managed provenance or history',
  );

  await copy('nested.jar');
  await unlink(path.join(directory, 'mods/api.jar'));
  let uncertain = await report();
  assert(!uncertain.complete);
  assert.equal(check(uncertain, 'owner.jar', 'fabric-api').status, 'unknown');
  await writeFile(path.join(directory, 'mods/nested.jar'), 'corrupt archive');
  uncertain = await report();
  assert(!uncertain.complete);
  assert.equal(check(uncertain, 'owner.jar', 'fabric-api').status, 'unknown');
  assert.equal(uncertain.mods.find((m) => m.filename === 'nested.jar').status, 'unreadable');
  dialog = await diagnostics();
  await expect(dialog).toContainText('Часть метаданных или вложенных JAR не прочитана');
  await expect(
    dialog.locator('[data-status="unknown"]').filter({ hasText: 'fabric-api' }),
  ).toBeVisible();
  await close(dialog);
  await unlink(path.join(directory, 'mods/nested.jar'));
  await copy('api.jar');
  checks.push(
    'nested and unreadable active JARs make completeness explicit and unresolved providers unknown instead of a fabricated missing verdict',
  );

  let plan = await invoke('local_content_plan', {
    id,
    paths: [path.join(fixtures, 'staged.jar'), path.join(fixtures, 'staged_dependency.jar')],
  });
  assert.equal(check(plan.diagnostics, 'staged.jar', 'staged_dependency').status, 'satisfied');
  assert(!plan.warnings.includes('dependency_issues'));
  await copy('nested.jar');
  await assert.rejects(
    invoke('finish_local_content', { token: plan.plan.token, acceptUnknown: true, cancel: false }),
    /RECORD_CONFLICT/,
  );
  await invoke('finish_local_content', {
    token: plan.plan.token,
    acceptUnknown: false,
    cancel: true,
  });
  await unlink(path.join(directory, 'mods/nested.jar'));
  plan = await invoke('local_content_plan', {
    id,
    paths: [path.join(fixtures, 'staged.jar'), path.join(fixtures, 'staged_dependency.jar')],
  });
  await invoke('finish_local_content', {
    token: plan.plan.token,
    acceptUnknown: false,
    cancel: false,
  });
  const adoption = await invoke('content_adoption_plan', {
    request: { instanceId: id, directory: 'mods', filename: 'api.jar', recognize: false },
  });
  await unlink(path.join(directory, 'mods/staged_dependency.jar'));
  await assert.rejects(
    invoke('finish_content_adoption', {
      token: adoption.plan.token,
      acceptUnknown: true,
      cancel: false,
    }),
    /RECORD_CONFLICT/,
  );
  await invoke('finish_content_adoption', {
    token: adoption.plan.token,
    acceptUnknown: false,
    cancel: true,
  });
  await copy('staged_dependency.jar');
  checks.push(
    'staged imports resolve their batch together and local/adoption confirmation refuses a graph changed since preview',
  );

  await stop();
  await launch();
  await go();
  const after = await report();
  assert(after.complete);
  assert.equal(after.errors, 0);
  assert.equal(check(after, 'staged.jar', 'staged_dependency').status, 'satisfied');
  assert.equal((await invoke('installed_content', { id })).length, 3);
  assert.equal(await hash(path.join(directory, 'mods/owner.jar')), original);
  assert.equal(
    await readFile(path.join(directory, 'saves/world/level.dat'), 'utf8'),
    'preserve diagnostic world',
  );
  assert.deepEqual(errors, []);
  checks.push(
    'restart rescans unchanged files, preserves local management metadata and worlds, and reports satisfied requirements',
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
