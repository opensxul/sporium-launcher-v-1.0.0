import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdir, readFile, writeFile, access, unlink } from 'node:fs/promises';
import { createServer } from 'node:net';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium, expect as baseExpect } from '@playwright/test';
const expect = baseExpect.configure({ timeout: 45_000 });

// A NEW UUID instance in the dedicated modded-probe root reuses only its immutable caches.
// Normal launcher data and the other fixture instances are never changed.
const workspace = path.resolve(import.meta.dirname, '..');
const dataDirectory = path.join(workspace, '.local/modded-smoke');
const artifacts = path.join(workspace, '.local/modrinth-native-smoke');
await access(path.join(dataDirectory, 'launcher/sporium.sqlite3'));
await mkdir(artifacts, { recursive: true });
let session;
const errors = [];
const checks = [];
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
  const page = browser.contexts()[0].pages()[0];
  page.setDefaultTimeout(45_000);
  page.on('pageerror', (error) => errors.push(error.message));
  await page.waitForSelector('.app-shell');
  session = { child, browser, page };
  return page;
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
let page;
let id;
try {
  page = await launch();
  const invoke = (command, args = {}) =>
    page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), {
      command,
      args,
    });
  const go = async (route) => {
    await page.evaluate((route) => {
      window.location.hash = route;
    }, route);
  };
  const before = await invoke('library_snapshot');
  const instanceName = `Modrinth from mod ${Date.now()}`;
  await go('/catalog');
  await page.getByRole('heading', { name: 'Каталог Modrinth', exact: true }).waitFor();
  await expect(page.locator('.catalog-result').first()).toBeVisible({ timeout: 90_000 });
  await page.getByLabel('Поиск на Modrinth', { exact: true }).fill('Mod Menu');
  await page.getByRole('button', { name: 'Найти', exact: true }).click();
  await expect(page.locator('.catalog-result h2').first()).toHaveText('Mod Menu');
  const card = page
    .locator('.catalog-result')
    .filter({ has: page.getByRole('heading', { name: 'Mod Menu', exact: true }) });
  await expect(card).toBeVisible({ timeout: 90_000 });
  await expect
    .poll(() => card.locator('img').evaluate((img) => img.complete && img.naturalWidth > 0))
    .toBe(true);
  await page.screenshot({ path: path.join(artifacts, 'global-catalog.png'), fullPage: true });
  await card.getByRole('button', { name: 'Подробнее / установить', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(
    dialog.getByRole('combobox', { name: 'Установить в сборку', exact: true }),
  ).toBeVisible();
  await dialog
    .getByRole('combobox', { name: 'Установить в сборку', exact: true })
    .selectOption('__new__');
  await expect(
    dialog.getByRole('combobox', { name: 'Совместимая версия', exact: true }),
  ).toBeVisible();
  await dialog.getByRole('textbox', { name: 'Название сборки', exact: true }).fill(instanceName);
  await dialog
    .getByRole('combobox', { name: 'Совместимая версия', exact: true })
    .selectOption('6lgOkclV');
  await dialog
    .getByRole('combobox', { name: 'Версия Minecraft', exact: true })
    .selectOption('1.21.1');
  await expect(dialog.getByRole('combobox', { name: 'Загрузчик', exact: true })).toHaveValue(
    'fabric',
  );
  await dialog.getByRole('button', { name: 'Проверить зависимости', exact: true }).click();
  await expect(dialog.getByRole('heading', { name: 'Будет установлено', exact: true })).toBeVisible(
    { timeout: 90_000 },
  );
  await expect(dialog.locator('.content-plan li')).toHaveCount(3);
  assert.deepEqual(
    await invoke('library_snapshot'),
    before,
    'Planning must not create an empty instance',
  );
  await expect(dialog.locator('.content-create-summary')).toContainText('1.21.1');
  await page.screenshot({ path: path.join(artifacts, 'dependency-plan.png'), fullPage: true });
  checks.push(
    'real CDN catalog icon; create-from-mod version/loader selection and read-only three-file dependency plan',
  );
  await dialog.getByRole('button', { name: 'Создать сборку и установить', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  id = (await invoke('content_state')).instanceId;
  const directory = path.join(dataDirectory, 'instances', id);
  await writeFile(path.join(directory, 'mods/local-preserve.txt'), 'unmanaged bytes');
  await writeFile(path.join(directory, 'saves/world-preserve.dat'), 'world bytes');
  await expect
    .poll(
      async () => {
        const job = await invoke('content_state');
        if (job?.phase === 'failed') throw new Error(JSON.stringify(job.error));
        return job?.phase;
      },
      { timeout: 300_000 },
    )
    .toBe('completed');
  const created = (await invoke('library_snapshot')).instances.find((i) => i.id === id);
  assert.equal(created.name, instanceName);
  assert.equal(created.minecraftVersion, '1.21.1');
  assert.equal(created.loader, 'fabric');
  assert.equal(created.status, 'installed');
  assert(created.loaderVersion);
  assert(created.iconRef.startsWith('https://cdn.modrinth.com/'));
  assert.equal((await invoke('game_state')).sessions.filter((s) => s.running).length, 0);
  await access(path.join(directory, '.sporium/installation.json'));
  checks.push(
    'confirmation creates exactly one instance with project icon and automatically prepares Minecraft, Java and Fabric without launching',
  );
  let installed = await invoke('installed_content', { id });
  assert.equal(installed.length, 3);
  assert(installed.every((r) => r.status === 'installed'));
  for (const { record } of installed) {
    const bytes = await readFile(path.join(directory, record.directory, record.file.filename));
    assert.equal(createHash('sha512').update(bytes).digest('hex'), record.file.hashes.sha512);
    assert.equal(record.provider, 'modrinth');
  }
  assert.equal(
    await readFile(path.join(directory, 'mods/local-preserve.txt'), 'utf8'),
    'unmanaged bytes',
  );
  assert.equal(
    await readFile(path.join(directory, 'saves/world-preserve.dat'), 'utf8'),
    'world bytes',
  );
  checks.push(
    'verified official downloads, SHA-512 receipts and required dependencies; unmanaged files and world preserved',
  );
  await go(`/instance/${id}`);
  await expect(page.locator('.content-records li')).toHaveCount(3);
  await expect
    .poll(() =>
      page
        .locator('.content-records img')
        .evaluateAll(
          (images) =>
            images.length === 3 && images.every((img) => img.complete && img.naturalWidth > 0),
        ),
    )
    .toBe(true);
  await expect
    .poll(() =>
      page
        .locator('.instance-title-icon img')
        .evaluate((img) => img.complete && img.naturalWidth > 0),
    )
    .toBe(true);
  await page.screenshot({ path: path.join(artifacts, 'installed-content.png'), fullPage: true });
  await page.getByRole('button', { name: 'Добавить контент', exact: true }).click();
  const browserDialog = page.getByRole('dialog', {
    name: 'Добавить контент из Modrinth',
    exact: true,
  });
  await expect(browserDialog.locator('.catalog-instance-context')).toContainText(
    'Minecraft 1.21.1 · Fabric',
  );
  assert.equal(new URL(page.url()).hash, `#/instance/${id}`);
  await expect
    .poll(async () => page.locator('.catalog-result').count(), { timeout: 90_000 })
    .toBeGreaterThan(0);
  await browserDialog
    .getByRole('searchbox', { name: 'Поиск на Modrinth', exact: true })
    .fill('Mod Menu');
  await browserDialog.getByRole('button', { name: 'Найти', exact: true }).click();
  const installedCard = browserDialog
    .locator('.catalog-result')
    .filter({ has: page.getByRole('heading', { name: 'Mod Menu', exact: true }) });
  await expect(
    installedCard.getByRole('button', { name: 'Установлено', exact: true }),
  ).toBeDisabled();
  await browserDialog
    .getByRole('checkbox', { name: 'Скрыть уже установленное', exact: true })
    .check();
  await expect(installedCard).toHaveCount(0);
  await browserDialog
    .getByRole('checkbox', { name: 'Скрыть уже установленное', exact: true })
    .uncheck();
  await expect(installedCard).toBeVisible();
  await page.screenshot({ path: path.join(artifacts, 'instance-catalog.png'), fullPage: false });
  await browserDialog.getByRole('button', { name: 'Закрыть', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Добавить контент', exact: true })).toBeFocused();
  checks.push(
    'instance catalog opens without route change; locked context, installed badge, hide/show filter and focus restoration',
  );
  const filtered = await invoke('content_search', {
    query: {
      query: 'Mod Menu',
      kind: 'mod',
      minecraft: '1.12.2',
      loader: 'forge',
      category: '',
      environment: 'server',
      sort: 'relevance',
      offset: 0,
      instanceId: id,
    },
  });
  assert(
    filtered.hits.some((h) => h.id === 'mOgUt4GM'),
    'Backend enforces instance context instead of forged query filters',
  );
  const parent = installed.find((r) => r.record.projectId === 'mOgUt4GM').record;
  const repeated = await invoke('content_plan', {
    request: { instanceId: id, projectId: parent.projectId, versionId: parent.version.id },
  });
  assert.equal(repeated.alreadyInstalled, 3);
  const target = path.join(directory, parent.directory, parent.file.filename);
  const original = await readFile(target);
  await writeFile(target, 'user-modified');
  try {
    const code = await page.evaluate(
      async (request) => {
        try {
          await window.__TAURI_INTERNALS__.invoke('content_plan', { request });
          return null;
        } catch (error) {
          return error.code;
        }
      },
      { instanceId: id, projectId: parent.projectId, versionId: parent.version.id },
    );
    assert.equal(code, 'CONTENT_CONFLICT');
    assert.equal(await readFile(target, 'utf8'), 'user-modified');
  } finally {
    await writeFile(target, original);
  }
  checks.push(
    'context locked by backend, repeat install reuses receipts, modified managed file protected',
  );
  // An unknown file is preserved even if a provider offers exactly the same destination name.
  const unknownPlan = await invoke('content_details', { projectId: 'sodium', instanceId: id });
  const unknownVersion = unknownPlan.versions.find((v) => v.version_type === 'release');
  assert(unknownVersion);
  const unknownFile = unknownVersion.files.find((f) => f.primary) ?? unknownVersion.files[0];
  const unknownPath = path.join(directory, 'mods', unknownFile.filename);
  await writeFile(unknownPath, 'local sodium sentinel');
  try {
    const code = await page.evaluate(
      async (request) => {
        try {
          await window.__TAURI_INTERNALS__.invoke('content_plan', { request });
          return null;
        } catch (error) {
          return error.code;
        }
      },
      { instanceId: id, projectId: unknownPlan.project.id, versionId: unknownVersion.id },
    );
    assert.equal(code, 'CONTENT_CONFLICT');
    assert.equal(await readFile(unknownPath, 'utf8'), 'local sodium sentinel');
  } finally {
    await unlink(unknownPath);
  }
  checks.push('unknown local filename collision never overwritten or adopted');
  await stop();
  page = await launch();
  installed = await invoke('installed_content', { id });
  assert.equal(installed.length, 3);
  assert(installed.every((r) => r.status === 'installed'));
  await go(`/instance/${id}`);
  await expect(page.locator('.content-records li')).toHaveCount(3);
  await page.getByRole('button', { name: 'Играть', exact: true }).click();
  await expect
    .poll(
      async () => {
        const view = await invoke('game_state');
        if (view.job?.phase === 'failed') throw new Error(JSON.stringify(view.job.error));
        return view.sessions.some((s) => s.instanceId === id && s.running);
      },
      { timeout: 180_000 },
    )
    .toBe(true);
  const game = (await invoke('game_state')).sessions.find((s) => s.instanceId === id && s.running);
  await expect
    .poll(
      async () => {
        const log = await readFile(game.logPath, 'utf8').catch(() => '');
        return /OpenAL initialized|Sound engine started|Created:.*atlas/i.test(log);
      },
      { timeout: 90_000 },
    )
    .toBe(true);
  await page.screenshot({ path: path.join(artifacts, 'modded-launch.png'), fullPage: true });
  await invoke('stop_game', { id });
  checks.push(
    'receipts survive native restart; Fabric 1.21.1 with Mod Menu and dependencies initializes renderer/audio',
  );
  for (const [slug, kind, directoryName] of [
    ['appleskin', 'mod', 'mods'],
    ['faithful-32x', 'resourcepack', 'resourcepacks'],
    ['complementary-reimagined', 'shader', 'shaderpacks'],
  ]) {
    const details = await invoke('content_details', { projectId: slug, instanceId: id });
    assert.equal(details.project.project_type, kind);
    const selected = details.versions.find((v) => v.version_type === 'release');
    assert(selected);
    await page.getByRole('button', { name: 'Добавить контент', exact: true }).click();
    const catalog = page.getByRole('dialog', { name: 'Добавить контент из Modrinth', exact: true });
    await catalog
      .getByRole('button', {
        name: kind === 'mod' ? 'Моды' : kind === 'shader' ? 'Шейдеры' : 'Ресурспаки',
        exact: true,
      })
      .click();
    await expect(catalog.getByRole('button', { name: 'Найти', exact: true })).toBeEnabled({
      timeout: 90_000,
    });
    await catalog
      .getByRole('searchbox', { name: 'Поиск на Modrinth', exact: true })
      .fill(details.project.title);
    await catalog.getByRole('button', { name: 'Найти', exact: true }).click();
    const row = catalog
      .locator('.catalog-result')
      .filter({ has: page.getByRole('heading', { name: details.project.title, exact: true }) });
    await expect(row).toBeVisible({ timeout: 90_000 });
    await row.getByRole('button', { name: 'Установить', exact: true }).click();
    let projectDialog = page.getByRole('dialog', { name: details.project.title, exact: true });
    await expect(projectDialog).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(projectDialog).toHaveCount(0);
    await expect(catalog).toBeVisible();
    await expect(
      catalog.getByRole('searchbox', { name: 'Поиск на Modrinth', exact: true }),
    ).toHaveValue(details.project.title);
    await row.getByRole('button', { name: 'Установить', exact: true }).click();
    projectDialog = page.getByRole('dialog', { name: details.project.title, exact: true });
    await projectDialog
      .getByRole('combobox', { name: 'Совместимая версия', exact: true })
      .selectOption(selected.id);
    await projectDialog.getByRole('button', { name: 'Проверить зависимости', exact: true }).click();
    await expect(projectDialog.locator('.content-plan')).toContainText(directoryName);
    await projectDialog.getByRole('button', { name: 'Установить выбранное', exact: true }).click();
    await expect(projectDialog).toHaveCount(0);
    await expect(catalog).toBeVisible();
    assert.equal(new URL(page.url()).hash, '#/instance/' + id);
    await expect
      .poll(async () => (await invoke('content_state'))?.phase, { timeout: 180_000 })
      .toBe('completed');
    await expect(row.getByRole('button', { name: 'Установлено', exact: true })).toBeDisabled();
    await expect(
      catalog.getByRole('searchbox', { name: 'Поиск на Modrinth', exact: true }),
    ).toHaveValue(details.project.title);
    await page.screenshot({
      path: path.join(artifacts, 'inline-' + kind + '.png'),
      fullPage: false,
    });
    await catalog.getByRole('button', { name: 'Закрыть', exact: true }).click();
    const results = await invoke('content_search', {
      query: {
        query: details.project.title,
        kind,
        minecraft: '',
        loader: '',
        category: '',
        environment: '',
        sort: 'relevance',
        offset: 0,
        instanceId: id,
      },
    });
    assert(results.hits.some((h) => h.id === details.project.id));
  }
  installed = await invoke('installed_content', { id });
  assert.equal(installed.length, 6);
  assert(installed.every((r) => r.status === 'installed'));
  checks.push(
    'resource pack and shader use compatible version files and separate destinations; contextual search accepts legacy environment metadata',
  );
  await page
    .getByRole('searchbox', { name: 'Поиск в контенте сборки', exact: true })
    .fill('Mod Menu');
  await expect(page.locator('.content-records li')).toHaveCount(1);
  await page.getByRole('searchbox', { name: 'Поиск в контенте сборки', exact: true }).fill('');
  await page
    .locator('.content-installed')
    .getByRole('button', { name: 'Ресурспаки', exact: true })
    .click();
  await expect(page.locator('.content-records li')).toHaveCount(1);
  await page
    .locator('.content-installed')
    .getByRole('button', { name: 'Все', exact: true })
    .click();
  await expect(page.locator('.content-records li')).toHaveCount(6);
  checks.push(
    'inline resource-pack/shader installation preserves catalog search and route; nested Escape closes only project dialog; installed list refreshes and filters',
  );
  const selectedNames = ['AppleSkin', 'Mod Menu'];
  for (const name of selectedNames)
    await page.getByRole('checkbox', { name: `Выбрать: ${name}`, exact: true }).check();
  await page.getByRole('button', { name: 'Отключить', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Отмена', exact: true }).click();
  assert((await invoke('installed_content', { id })).every((r) => r.status === 'installed'));
  await page.getByRole('button', { name: 'Отключить', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Подтвердить', exact: true }).click();
  await expect(
    page.getByRole('switch', { name: 'Включить: AppleSkin', exact: true }),
  ).toHaveAttribute('aria-checked', 'false');
  installed = await invoke('installed_content', { id });
  assert.equal(installed.filter((r) => r.status === 'disabled').length, 2);
  for (const row of installed.filter((r) => r.status === 'disabled')) {
    await access(
      path.join(dataDirectory, 'instances', id, 'mods_disabled', row.record.file.filename),
    );
    await assert.rejects(
      access(path.join(dataDirectory, 'instances', id, 'mods', row.record.file.filename)),
    );
    assert.equal(row.record.provider, 'modrinth');
  }
  await page.locator('.content-history summary').click();
  await page.screenshot({ path: path.join(artifacts, 'content-management.png'), fullPage: true });
  checks.push(
    'bulk disable confirmation/cancel, real file relocation, no active duplicates, preserved provenance and visible history',
  );
  await stop();
  page = await launch();
  await go(`/instance/${id}`);
  await expect(
    page.getByRole('switch', { name: 'Включить: AppleSkin', exact: true }),
  ).toBeVisible();
  installed = await invoke('installed_content', { id });
  assert.equal(installed.filter((r) => r.status === 'disabled').length, 2);
  await page.getByRole('checkbox', { name: 'Выбрать видимые', exact: true }).check();
  await page.getByRole('button', { name: 'Включить', exact: true }).click();
  await expect(page.getByRole('dialog').locator('li')).toHaveCount(2);
  await page.getByRole('dialog').getByRole('button', { name: 'Подтвердить', exact: true }).click();
  await expect(
    page.getByRole('switch', { name: 'Отключить: AppleSkin', exact: true }),
  ).toHaveAttribute('aria-checked', 'true');
  assert((await invoke('installed_content', { id })).every((r) => r.status === 'installed'));
  checks.push('restart keeps disabled mods and bulk re-enable restores active content');
  await page.getByRole('button', { name: 'Удалить: AppleSkin', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Подтвердить', exact: true }).click();
  await expect(page.locator('.content-records li')).toHaveCount(5);
  installed = await invoke('installed_content', { id });
  assert(!installed.some((r) => r.record.title === 'AppleSkin'));
  const history = await invoke('content_history', { id });
  assert.deepEqual(
    history.slice(-3).map((event) => event.action),
    ['disable', 'enable', 'delete'],
  );
  checks.push(
    'confirmed single deletion changes only selected content and persists ordered change history',
  );
  assert.deepEqual(errors, []);
  await writeFile(
    path.join(artifacts, 'result.json'),
    JSON.stringify(
      {
        passed: true,
        checks,
        instanceId: id,
        dataDirectory,
        files: installed.map((r) => ({
          project: r.record.projectId,
          version: r.record.version.id,
          file: r.record.file.filename,
        })),
        logPath: game.logPath,
      },
      null,
      2,
    ),
  );
  console.log(`Modrinth native smoke passed: ${checks.length} checks.`);
} catch (error) {
  await page
    ?.screenshot({ path: path.join(artifacts, 'failure.png'), fullPage: true })
    .catch(() => {});
  if (page)
    await writeFile(
      path.join(artifacts, 'failure-ui.txt'),
      await page
        .locator('body')
        .innerText()
        .catch(() => ''),
    );
  throw error;
} finally {
  await stop();
}
