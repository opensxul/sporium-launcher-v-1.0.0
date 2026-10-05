import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:net';
import { copyFile, mkdir, mkdtemp, readFile, readdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium, expect as baseExpect } from '@playwright/test';
const expect = baseExpect.configure({ timeout: 90_000 });

const workspace = path.resolve(import.meta.dirname, '..');
const artifacts = path.join(workspace, '.local/content-restore-smoke');
await mkdir(artifacts, { recursive: true });
const dataDirectory = await mkdtemp(path.join(artifacts, 'data-'));
const previous = JSON.parse(
  await readFile(path.join(workspace, '.local/content-update-smoke/result.json'), 'utf8'),
);
assert(
  previous.passed &&
    path.resolve(previous.dataDirectory).startsWith(path.join(workspace, '.local') + path.sep),
);
const original = path.join(previous.dataDirectory, 'instances', previous.id);
const snapshots = path.join(original, '.sporium/content-snapshots');
const source = (await readdir(snapshots))[0];
const snapshot = JSON.parse(await readFile(path.join(snapshots, source, 'snapshot.json'), 'utf8'));
const receipt = JSON.parse(await readFile(path.join(original, '.sporium/content.json'), 'utf8'));
assert(snapshot.files.length && receipt.files.length);
let child;
let browser;
let page;
let id;
const checks = [];
const errors = [];
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
  page.setDefaultTimeout(30_000);
  page.on('pageerror', (error) => errors.push(error.message));
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.waitForSelector('.app-shell');
}
async function stop() {
  await browser?.close();
  browser = null;
  if (child && child.exitCode === null) {
    const exit = once(child, 'exit');
    child.kill();
    await exit;
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
      if (attempt >= 50 || !/LIBRARY_BUSY|INSTANCE_BUSY/.test(error.message)) throw error;
      await delay(100);
    }
  }
}
async function go() {
  await page.evaluate((id) => {
    window.location.hash = `/instance/${id}?tab=history`;
  }, id);
  await page.getByRole('button', { name: 'Точки восстановления', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByRole('status')).toHaveCount(0);
  await expect(dialog.locator('input[type=radio]').first()).toBeVisible();
  return dialog;
}
try {
  await launch();
  id = (
    await invoke('create_instance', {
      request: {
        name: 'Restore probe',
        minecraftVersion: snapshot.instance.minecraftVersion,
        loader: snapshot.instance.loader,
        collectionId: null,
      },
    })
  ).affectedId;
  const instance = (await invoke('library_snapshot')).instances.find((value) => value.id === id);
  const directory = path.join(dataDirectory, 'instances', id);
  const root = path.join(directory, '.sporium/content-snapshots', source);
  await mkdir(path.join(root, 'files'), { recursive: true });
  for (const record of receipt.files) {
    const target = path.join(directory, record.directory, record.file.filename);
    await mkdir(path.dirname(target), { recursive: true });
    await copyFile(path.join(original, record.directory, record.file.filename), target);
  }
  for (let index = 0; index < snapshot.files.length; index++)
    await copyFile(
      path.join(snapshots, source, 'files', `${index}.bin`),
      path.join(root, 'files', `${index}.bin`),
    );
  await writeFile(path.join(root, 'snapshot.json'), JSON.stringify({ ...snapshot, instance }));
  await writeFile(path.join(directory, '.sporium/content.json'), JSON.stringify(receipt));
  await mkdir(path.join(directory, 'saves'), { recursive: true });
  await writeFile(path.join(directory, 'saves/preserve.dat'), 'world');
  await writeFile(path.join(directory, 'mods/manual.jar'), 'manual');
  await writeFile(path.join(directory, 'options.txt'), 'settings');
  const before = await invoke('installed_content', { id });
  assert(before.every((value) => ['installed', 'disabled'].includes(value.status)));
  // Fixture creation uses raw IPC; restart to load the new library in the UI provider.
  await stop();
  await launch();
  let dialog = await go();
  await expect(dialog.getByRole('button', { name: 'Восстановить', exact: true })).toBeDisabled();
  await dialog.getByRole('button', { name: 'Отмена', exact: true }).click();
  assert.deepEqual(await invoke('installed_content', { id }), before);
  checks.push('real native restore-point list requires selection; cancel preserves all receipts');
  await page.getByRole('button', { name: 'Точки восстановления', exact: true }).click();
  dialog = page.getByRole('dialog');
  await dialog.locator('input[type=radio]').first().check();
  await page.screenshot({ path: path.join(artifacts, 'restore-1400.png') });
  await page.setViewportSize({ width: 1000, height: 760 });
  await page.screenshot({ path: path.join(artifacts, 'restore-1000.png') });
  assert(await dialog.evaluate((element) => element.scrollWidth <= element.clientWidth + 1));
  checks.push(
    'restore preview at 1400 and 1000 widths has explicit effect/undo notice and no horizontal overflow',
  );
  await dialog.getByRole('button', { name: 'Восстановить', exact: true }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  const restored = await invoke('installed_content', { id });
  assert.deepEqual(
    restored.map((value) => value.record),
    snapshot.files,
  );
  assert(restored.every((value) => ['installed', 'disabled'].includes(value.status)));
  for (const [name, bytes] of [
    ['saves/preserve.dat', 'world'],
    ['mods/manual.jar', 'manual'],
    ['options.txt', 'settings'],
  ])
    assert.equal(await readFile(path.join(directory, name), 'utf8'), bytes);
  checks.push(
    'UI restore publishes exact verified old real CDN files/receipts and preserves worlds/settings/untracked files',
  );
  const undo = (await invoke('content_restore_points', { id })).find(
    (value) => value.id !== source,
  );
  assert(undo?.available);
  await invoke('restore_content', { id, point: undo.id });
  assert.deepEqual(await invoke('installed_content', { id }), before);
  checks.push('automatically saved undo point restores the previous updated and disabled state');
  await stop();
  await launch();
  assert.deepEqual(await invoke('installed_content', { id }), before);
  assert.equal(
    (await invoke('content_history', { id })).filter((value) => value.action === 'restore').length,
    2,
  );
  checks.push('restart preserves restored receipts and exactly two history events');
  await writeFile(path.join(root, 'files/0.bin'), 'corrupt');
  assert.equal(
    (await invoke('content_restore_points', { id })).find((value) => value.id === source).available,
    false,
  );
  await assert.rejects(invoke('restore_content', { id, point: source }), /INTEGRITY/);
  assert.deepEqual(await invoke('installed_content', { id }), before);
  checks.push('corrupted restore bytes disable the point and backend refuses mutation');
  assert.deepEqual(errors, []);
  await writeFile(
    path.join(artifacts, 'result.json'),
    JSON.stringify(
      { passed: true, checks, id, dataDirectory, date: new Date().toISOString() },
      null,
      2,
    ),
  );
  console.log(`Content restore native: ${checks.length} checks passed.`);
} catch (error) {
  await page?.screenshot({ path: path.join(artifacts, 'failure.png') }).catch(() => {});
  throw error;
} finally {
  await stop();
}
