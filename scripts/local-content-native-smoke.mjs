import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdir, readFile, writeFile, access, copyFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium, expect } from '@playwright/test';
const workspace = path.resolve(import.meta.dirname, '..');
const data = path.join(workspace, '.local/modded-smoke');
const artifacts = path.join(workspace, '.local/local-content-smoke');
const fixtures = path.join(artifacts, `files-${Date.now()}`);
await mkdir(artifacts, { recursive: true });
async function powershell(script, args) {
  const child = spawn(
    'powershell.exe',
    [
      '-NoProfile',
      '-ExecutionPolicy',
      'Bypass',
      '-File',
      path.join(workspace, 'scripts', script),
      ...args,
    ],
    { windowsHide: true },
  );
  let output = '';
  child.stdout.on('data', (d) => (output += d));
  child.stderr.on('data', (d) => (output += d));
  const [exit] = await once(child, 'exit');
  assert.equal(exit, 0, output);
}
await powershell('local-content-fixtures.ps1', ['-Directory', fixtures]);
let child, browser, page, id;
const checks = [];
async function start() {
  const net = createServer();
  net.listen(0, '127.0.0.1');
  await once(net, 'listening');
  const port = net.address().port;
  await new Promise((r) => net.close(r));
  child = spawn(path.join(workspace, 'src-tauri/target/debug/sporium.exe'), [], {
    windowsHide: true,
    stdio: 'ignore',
    env: {
      ...process.env,
      SPORIUM_TEST_DATA_DIR: data,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
  });
  for (let i = 0; i < 120; i++) {
    try {
      browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
      break;
    } catch {
      await delay(250);
    }
  }
  assert(browser);
  for (let attempt = 0; attempt < 120; attempt++) {
    page = browser.contexts()[0]?.pages()[0];
    if (page) break;
    await delay(100);
  }
  assert(page);
  page.setDefaultTimeout(30000);
  await page.waitForSelector('.app-shell');
}
async function stop() {
  await browser?.close();
  browser = null;
  if (child?.exitCode === null) {
    const exit = once(child, 'exit');
    child.kill();
    await exit;
  }
}
const invoke = (command, args = {}) =>
  page.evaluate(
    ({ command, args }) =>
      window.__TAURI_INTERNALS__.invoke(command, args).catch((error) => {
        throw new Error(JSON.stringify(error));
      }),
    {
      command,
      args,
    },
  );
async function go() {
  await page.evaluate((id) => {
    window.location.hash = `/instance/${id}`;
  }, id);
  await page.getByRole('button', { name: 'Добавить из файла', exact: true }).waitFor();
}
try {
  await start();
  id = (
    await invoke('create_instance', {
      request: {
        name: `Local imports ${Date.now()}`,
        minecraftVersion: '1.21.1',
        loader: 'fabric',
        collectionId: null,
      },
    })
  ).affectedId;
  assert(id);
  await page.reload();
  await page.waitForSelector('.app-shell');
  // Picker/drop/confirmation UI was checked manually by the user. This suite exercises real IPC.
  const planFiles = (names) =>
    invoke('local_content_plan', {
      id,
      paths: names.map((name) => path.join(fixtures, name)),
    });
  const finish = (plan, acceptUnknown = false, cancel = false) =>
    invoke('finish_local_content', {
      token: plan.plan.token,
      acceptUnknown,
      cancel,
    });
  const picked = await planFiles(['picked.jar']);
  assert.equal(picked.plan.files[0].title, 'Local picked');
  await assert.rejects(access(path.join(data, 'instances', id, 'mods/picked.jar')));
  await finish(picked);
  assert.deepEqual(
    await readFile(path.join(fixtures, 'picked.jar')),
    await readFile(path.join(data, 'instances', id, 'mods/picked.jar')),
  );
  checks.push('native IPC read-only plan, exact-byte install and source preservation');
  const batch = await planFiles(['drop-one.jar', 'drop-two.jar']);
  assert.equal(batch.plan.files.length, 2);
  await finish(batch);
  assert.equal((await invoke('installed_content', { id })).length, 3);
  checks.push('native IPC installs multiple files into the current instance');
  await assert.rejects(
    invoke('local_content_plan', { id, paths: [path.join(fixtures, 'wrong-version.jar')] }),
    (e) => e.message.includes('CONTENT_INCOMPATIBLE'),
  );
  await assert.rejects(
    invoke('local_content_plan', { id, paths: [path.join(fixtures, 'picked.jar')] }),
    (e) => e.message.includes('CONTENT_CONFLICT'),
  );
  checks.push('wrong Minecraft and duplicate IDs rejected');
  const unknown = await planFiles(['unknown.jar']);
  assert(unknown.warnings.includes('unknown_metadata'));
  await assert.rejects(finish(unknown), (e) => e.message.includes('CONTENT_INCOMPATIBLE'));
  await finish(unknown, false, true);
  assert.equal((await invoke('installed_content', { id })).length, 3);
  await finish(await planFiles(['unknown.jar']), true);
  checks.push('unknown metadata requires explicit confirmation; cancelling does not install');
  const file = picked.plan.files[0];
  await invoke('change_content', {
    request: {
      instanceId: id,
      action: 'disable',
      files: [
        {
          directory: file.directory,
          filename: file.file.filename,
          sha512: file.file.hashes.sha512,
        },
      ],
    },
  });
  const receiptPath = path.join(data, 'instances', id, '.sporium/content.json');
  const receipt = await readFile(receiptPath);
  await copyFile(
    path.join(fixtures, 'wrong-version.jar'),
    path.join(data, 'instances', id, 'mods/manual.jar'),
  );
  const external = await invoke('untracked_content', { id });
  assert.equal(external.length, 1);
  assert.equal(external[0].status, 'incompatible');
  assert.deepEqual(await readFile(receiptPath), receipt);
  checks.push('manual file inventory detects incompatible JAR without adopting or changing it');
  await stop();
  await start();
  await go();
  await expect(
    page.getByRole('switch', { name: 'Включить: Local picked', exact: true }),
  ).toBeVisible();
  const rows = await invoke('installed_content', { id });
  assert.equal(rows.length, 4);
  assert(rows.every((r) => r.record.provider === 'local' && r.record.file.url === ''));
  await expect(page.locator('.untracked-files li')).toHaveCount(1);
  await expect(page.locator('.untracked-files')).toContainText('Несовместим со сборкой');
  await page
    .locator('.content-installed')
    .screenshot({ path: path.join(artifacts, 'local-content.png') });
  checks.push('local provenance and disabled state survive native restart');
  await writeFile(
    path.join(artifacts, 'result.json'),
    JSON.stringify(
      { passed: true, checks, instanceId: id, fixtures, date: new Date().toISOString() },
      null,
      2,
    ),
  );
  console.log(`Local content native: ${checks.length} checks passed.`);
} catch (error) {
  await page
    ?.screenshot({ path: path.join(artifacts, 'failure.png'), fullPage: true })
    .catch(() => {});
  throw error;
} finally {
  await stop();
}
