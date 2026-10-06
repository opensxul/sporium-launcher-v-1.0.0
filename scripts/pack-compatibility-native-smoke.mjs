// Live Modrinth regression for Fresh & Smooth. All data stays in a fresh .local directory.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:net';
import { mkdir, readFile, writeFile, access } from 'node:fs/promises';
import { setTimeout as delay } from 'node:timers/promises';
import path from 'node:path';
import { chromium, expect } from '@playwright/test';

const root = path.resolve(import.meta.dirname, '..');
const directory = path.join(root, '.local/pack-compatibility', `run-${Date.now()}`);
const data = path.join(directory, 'data');
await mkdir(data, { recursive: true });
const server = createServer();
server.listen(0, '127.0.0.1');
await once(server, 'listening');
const port = server.address().port;
await new Promise((resolve) => server.close(resolve));
const child = spawn(path.join(root, 'src-tauri/target/debug/Sporium.exe'), [], {
  cwd: root,
  windowsHide: true,
  stdio: 'ignore',
  env: {
    ...process.env,
    SPORIUM_TEST_DATA_DIR: data,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
  },
});
let browser, page;
const errors = [];
const invoke = (command, args = {}) =>
  page.evaluate(
    async ({ command, args }) => {
      try {
        return await window.__TAURI_INTERNALS__.invoke(command, args);
      } catch (error) {
        throw new Error(JSON.stringify(error), { cause: error });
      }
    },
    { command, args },
  );
try {
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
  await page.waitForSelector('.app-shell');
  const bootstrap = await invoke('bootstrap');
  assert.equal(
    path.resolve(bootstrap.info.dataDirectory).toLowerCase(),
    path.resolve(data).toLowerCase(),
  );
  if (process.argv.includes('--expect-unsupported')) {
    await assert.rejects(
      invoke('provider_pack_preview', { projectId: 'ICQPO4fX', versionId: '7WgtmLp8' }),
      /CONTENT_UNSUPPORTED/,
    );
    console.log('Original Fresh & Smooth import failure reproduced through native IPC.');
  } else {
    const preview = await invoke('provider_pack_preview', {
      projectId: 'ICQPO4fX',
      versionId: '7WgtmLp8',
    });
    assert.equal(preview.minecraft, '26.2');
    assert.equal(preview.loader, 'fabric');
    assert.equal(preview.loaderVersion, '0.19.5');
    assert(preview.warnings.includes('private_overrides_skipped:1'));
    assert.equal(preview.requiredFiles, 172);
    await page.evaluate(
      (detail) => window.dispatchEvent(new window.CustomEvent('sporium-pack-preview', { detail })),
      preview,
    );
    const dialog = page.getByRole('dialog', { name: 'Импорт сборки', exact: true });
    await expect(dialog).toBeVisible();
    await dialog.getByText('Подробности импорта', { exact: true }).click();
    await expect(
      dialog.getByText('Пропущены приватные служебные файлы модов: 1', { exact: true }),
    ).toBeVisible();
    await page.screenshot({ path: path.join(directory, 'preview.png'), fullPage: true });
    await dialog.getByRole('button', { name: 'Создать и импортировать', exact: true }).click();
    let completed;
    for (let attempt = 0; attempt < 2400; attempt++) {
      const job = await invoke('pack_state');
      if (job?.phase === 'completed') {
        completed = job;
        break;
      }
      assert(!job || !['failed', 'cancelled'].includes(job.phase), JSON.stringify(job));
      await delay(250);
    }
    assert(completed, 'Pack import timed out');
    const instance = path.join(data, 'instances', completed.instanceId);
    for (const file of [
      'data/fabricDefaultResourcePacks.dat',
      'data/fabric_default_resource_packs.json',
      'automodpack/automodpack-client.json',
      'automodpack/automodpack-server.json',
    ]) {
      assert((await readFile(path.join(instance, file))).length > 0, file);
    }
    await assert.rejects(access(path.join(instance, 'automodpack/.private')));
    await page.reload();
    await page.waitForSelector('.app-shell');
    const snapshot = await invoke('library_snapshot');
    const saved = snapshot.instances.find((item) => item.id === completed.instanceId);
    assert.equal(saved.minecraftVersion, '26.2');
    assert.equal(saved.loaderVersion, '0.19.5');
    assert.deepEqual(errors, []);
    const result = {
      passed: true,
      dataDirectory: data,
      instanceId: completed.instanceId,
      requiredFiles: preview.requiredFiles,
      checks: [
        'live provider preview',
        'localized skipped-private notice',
        'full pack import through UI',
        'Fabric and AutoModpack files retained',
        'private state omitted',
        'saved instance after reload',
      ],
    };
    await writeFile(path.join(directory, 'result.json'), JSON.stringify(result, null, 2));
    await writeFile(
      path.join(root, '.local/pack-compatibility/result.json'),
      JSON.stringify(result, null, 2),
    );
    console.log(`Fresh & Smooth import passed: ${directory}`);
  }
} catch (error) {
  await page
    ?.screenshot({ path: path.join(directory, 'failure.png'), fullPage: true })
    .catch(() => {});
  throw error;
} finally {
  await browser?.close();
  if (child.exitCode === null) {
    const exited = once(child, 'exit');
    child.kill();
    await exited;
  }
}
