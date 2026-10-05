import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:net';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium, expect as baseExpect } from '@playwright/test';
const expect = baseExpect.configure({ timeout: 90_000 });
const workspace = path.resolve(import.meta.dirname, '..');
const dataDirectory = path.join(workspace, '.local/modded-smoke');
const artifacts = path.join(workspace, '.local/content-update-smoke');
await mkdir(artifacts, { recursive: true });
let session;
let page;
let id;
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
  session = { child };
  for (let attempt = 0; attempt < 120; attempt++) {
    assert.equal(child.exitCode, null);
    try {
      browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
      break;
    } catch {
      await delay(250);
    }
  }
  assert(browser, 'WebView2 unavailable');
  page = browser.contexts()[0].pages()[0];
  session.browser = browser;
  page.setDefaultTimeout(90_000);
  await page.setViewportSize({ width: 1400, height: 900 });
  page.on('pageerror', (e) => errors.push(e.message));
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
          } catch (e) {
            throw new Error(JSON.stringify(e), { cause: e });
          }
        },
        { command, args },
      );
    } catch (e) {
      if (attempt >= 30 || !/LIBRARY_BUSY|INSTANCE_BUSY/.test(e.message)) throw e;
      await delay(100);
    }
  }
}
async function go() {
  await page.evaluate((id) => {
    window.location.hash = `/instance/${id}`;
  }, id);
}
async function completed() {
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
}
async function open(initialTitle) {
  await page
    .getByRole('button', {
      name: initialTitle ? `Проверить обновления: ${initialTitle}` : 'Проверить обновления',
      exact: true,
    })
    .click();
  await expect(page.getByRole('dialog').locator('.content-updates li').first()).toBeVisible();
  await expect(page.getByRole('dialog').getByRole('status')).toHaveCount(0);
  return page.getByRole('dialog');
}
try {
  await launch();
  id = (
    await invoke('create_instance', {
      request: {
        name: `Update probe ${Date.now()}`,
        minecraftVersion: '1.21.1',
        loader: 'fabric',
        collectionId: null,
      },
    })
  ).affectedId;
  const directory = path.join(dataDirectory, 'instances', id);
  const projects = ['appleskin', 'sodium', 'modmenu'];
  const titles = [];
  for (const projectId of projects) {
    const details = await invoke('content_details', { projectId, instanceId: id });
    const versions = details.versions
      .filter((v) => v.version_type === 'release' && v.status === 'listed')
      .sort((a, b) => b.date_published.localeCompare(a.date_published));
    assert(versions.length >= 2, `${projectId} needs two real compatible releases`);
    const previous = versions.find((v) => v.date_published < versions[0].date_published);
    assert(previous, 'A genuinely older dated release is required');
    const plan = await invoke('content_plan', {
      request: { instanceId: id, projectId: details.project.id, versionId: previous.id },
    });
    await invoke('content_install', { token: plan.token });
    await completed();
    titles.push(details.project.title);
  }
  await writeFile(path.join(directory, 'mods/manual.txt'), 'manual sentinel');
  await writeFile(path.join(directory, 'saves/world.dat'), 'world sentinel');
  await mkdir(path.join(directory, 'config'), { recursive: true });
  await writeFile(path.join(directory, 'config/probe.json'), '{"preserve":true}');
  await go();
  const checked = await invoke('content_updates', { id });
  for (const title of titles) {
    const u = checked.find((u) => u.title === title);
    assert.equal(u.status, 'available');
    assert(u.candidate.game_versions.includes('1.21.1'));
    assert(u.candidate.loaders.includes('fabric'));
  }
  checks.push(
    'live official update discovery selects newer stable exact Minecraft/Fabric versions',
  );

  let dialog = await open();
  let apple = dialog
    .locator('.content-updates li')
    .filter({ has: page.getByText(titles[0], { exact: true }) });
  await apple.getByRole('button', { name: 'Закрепить версию', exact: true }).click();
  await expect(apple.getByText('Версия закреплена', { exact: true })).toBeVisible();
  await expect(apple.getByRole('checkbox')).toBeDisabled();
  await dialog.getByRole('button', { name: 'Отмена', exact: true }).click();
  await stop();
  await launch();
  await go();
  dialog = await open();
  apple = dialog
    .locator('.content-updates li')
    .filter({ has: page.getByText(titles[0], { exact: true }) });
  await expect(apple.getByText('Версия закреплена', { exact: true })).toBeVisible();
  await apple.getByRole('button', { name: 'Снять закрепление', exact: true }).click();
  await apple.getByRole('button', { name: 'Пропустить эту версию', exact: true }).click();
  await expect(apple.getByText('Обновление пропущено', { exact: true })).toBeVisible();
  await dialog.getByRole('button', { name: 'Отмена', exact: true }).click();
  assert.equal(
    (await invoke('content_updates', { id })).find((u) => u.title === titles[0]).status,
    'ignored',
  );
  checks.push(
    'pin/unpin and specific-version ignore are persisted and exclude automatic selection after restart',
  );

  await page.getByRole('switch', { name: `Отключить: ${titles[1]}`, exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Подтвердить', exact: true }).click();
  await expect(
    page.getByRole('switch', { name: `Включить: ${titles[1]}`, exact: true }),
  ).toBeVisible();
  const beforeSingle = await invoke('installed_content', { id });
  dialog = await open(titles[1]);
  assert.equal(
    await dialog
      .getByRole('checkbox')
      .filter({ visible: true })
      .evaluateAll((items) => items.filter((i) => i.checked).length),
    1,
  );
  await dialog.getByRole('button', { name: 'Посмотреть изменения', exact: true }).click();
  await expect(dialog.locator('.update-plan')).toContainText(titles[1]);
  await expect(dialog.locator('.update-plan')).toContainText('Отключён');
  await dialog.getByRole('button', { name: 'Назад к списку', exact: true }).click();
  await dialog.getByRole('button', { name: 'Отмена', exact: true }).click();
  assert.deepEqual(await invoke('installed_content', { id }), beforeSingle);
  checks.push(
    'single selection and cancelled preview leave tracked state and file bytes unchanged',
  );
  dialog = await open(titles[1]);
  await dialog.getByRole('button', { name: 'Посмотреть изменения', exact: true }).click();
  await dialog.getByRole('button', { name: 'Обновить', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await completed();
  const afterSingle = await invoke('installed_content', { id });
  const newSodium = afterSingle.find((r) => r.record.title === titles[1]);
  assert.equal(newSodium.status, 'disabled');
  assert.notEqual(
    newSodium.record.version.id,
    beforeSingle.find((r) => r.record.title === titles[1]).record.version.id,
  );
  checks.push('real single update replaces the selected project and preserves disabled state');

  dialog = await open();
  apple = dialog
    .locator('.content-updates li')
    .filter({ has: page.getByText(titles[0], { exact: true }) });
  await apple.getByRole('button', { name: 'Сбросить пропуск', exact: true }).click();
  await dialog.getByRole('button', { name: 'Отмена', exact: true }).click();
  for (const title of [titles[0], titles[2]])
    await page.getByRole('checkbox', { name: `Выбрать: ${title}`, exact: true }).check();
  await page.getByRole('button', { name: 'Обновить выбранные', exact: true }).click();
  dialog = page.getByRole('dialog');
  await expect(dialog.getByRole('status')).toHaveCount(0);
  await expect(
    dialog.getByRole('checkbox', { name: `Выбрать: ${titles[0]}`, exact: true }),
  ).toBeChecked();
  await expect(
    dialog.getByRole('checkbox', { name: `Выбрать: ${titles[2]}`, exact: true }),
  ).toBeChecked();
  const beforeBulk = await invoke('installed_content', { id });
  await dialog.getByRole('button', { name: 'Посмотреть изменения', exact: true }).click();
  await expect(dialog.locator('.update-plan')).toBeVisible();
  await dialog.evaluate((element) => {
    element.scrollTop = 0;
  });
  await page.screenshot({ path: path.join(artifacts, 'update-plan-1400.png'), fullPage: false });
  await dialog.getByRole('button', { name: 'Обновить', exact: true }).click();
  await completed();
  const afterBulk = await invoke('installed_content', { id });
  for (const title of [titles[0], titles[2]])
    assert.notEqual(
      afterBulk.find((r) => r.record.title === title).record.version.id,
      beforeBulk.find((r) => r.record.title === title).record.version.id,
    );
  assert.equal(
    afterBulk.find((r) => r.record.title === titles[1]).record.version.id,
    newSodium.record.version.id,
  );
  for (const row of afterBulk) {
    const bytes = await readFile(
      path.join(directory, row.record.directory, row.record.file.filename),
    );
    assert.equal(createHash('sha512').update(bytes).digest('hex'), row.record.file.hashes.sha512);
  }
  assert.equal(await readFile(path.join(directory, 'mods/manual.txt'), 'utf8'), 'manual sentinel');
  assert.equal(await readFile(path.join(directory, 'saves/world.dat'), 'utf8'), 'world sentinel');
  const history = await invoke('content_history', { id });
  assert.equal(history.filter((e) => e.action === 'update').length, 2);
  checks.push(
    'selected compatible batch updates real CDN files/dependencies, preserves unselected/local/world data and records old → new history',
  );
  const { readdir } = await import('node:fs/promises');
  const snapshots = await readdir(path.join(directory, '.sporium/content-snapshots'));
  assert.equal(snapshots.length, 2);
  for (const token of snapshots) {
    const root = path.join(directory, '.sporium/content-snapshots', token);
    const manifest = JSON.parse(await readFile(path.join(root, 'snapshot.json'), 'utf8'));
    for (const [index, record] of manifest.files.entries()) {
      const bytes = await readFile(path.join(root, `files/${index}.bin`));
      assert.equal(createHash('sha512').update(bytes).digest('hex'), record.file.hashes.sha512);
    }
    assert.equal(
      await readFile(path.join(root, 'settings/config/probe.json'), 'utf8'),
      '{"preserve":true}',
    );
  }
  checks.push(
    'verified pre-update snapshots retain original managed bytes, provenance and config without copying worlds',
  );
  await stop();
  await launch();
  await go();
  assert.deepEqual(await invoke('installed_content', { id }), afterBulk);
  assert.equal(
    (await invoke('content_history', { id })).filter((e) => e.action === 'update').length,
    2,
  );
  dialog = await open();
  await dialog
    .getByRole('button', { name: 'Выбрать все доступные обновления', exact: true })
    .click();
  await expect(
    dialog.getByRole('button', { name: 'Посмотреть изменения', exact: true }),
  ).toBeDisabled();
  await page.setViewportSize({ width: 1000, height: 760 });
  await page.screenshot({ path: path.join(artifacts, 'updates-1000.png'), fullPage: false });
  checks.push(
    'restart preserves updated/disabled state and history; current/incompatible content is excluded from update-all',
  );
  await dialog.getByRole('button', { name: 'Отмена', exact: true }).click();
  await page.getByRole('button', { name: 'Играть', exact: true }).click();
  let game;
  await expect
    .poll(
      async () => {
        game = (await invoke('game_state')).sessions.find((s) => s.instanceId === id && s.running);
        const alerts = await page.getByRole('alert').allTextContents();
        assert(!alerts.some((text) => text.includes('Сборка занята')), alerts.join('\n'));
        return Boolean(game);
      },
      { timeout: 180_000 },
    )
    .toBe(true);
  await expect
    .poll(
      async () => {
        const log = await readFile(game.logPath, 'utf8');
        return log.includes('OpenAL initialized') && log.includes('textures/atlas/gui.png-atlas');
      },
      { timeout: 90_000 },
    )
    .toBe(true);
  await invoke('stop_game', { id });
  await expect
    .poll(async () =>
      (await invoke('game_state')).sessions.some((s) => s.instanceId === id && s.running),
    )
    .toBe(false);
  checks.push('updated real Fabric instance reaches renderer/audio and stops cleanly');
  assert.deepEqual(errors, []);
  await writeFile(
    path.join(artifacts, 'result.json'),
    JSON.stringify(
      { passed: true, checks, id, dataDirectory, date: new Date().toISOString() },
      null,
      2,
    ),
  );
  console.log(`Content update native: ${checks.length} checks passed.`);
} catch (e) {
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
  throw e;
} finally {
  await stop();
}
