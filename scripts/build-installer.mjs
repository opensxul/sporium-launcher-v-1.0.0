import { spawnSync } from 'node:child_process';
import { mkdir, copyFile, readFile, writeFile, access } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { homedir } from 'node:os';
import path from 'node:path';

const root = path.resolve(import.meta.dirname, '..');
const config = JSON.parse(await readFile(path.join(root, 'src-tauri/tauri.conf.json'), 'utf8'));
const version = config.version;
const key =
  process.env.TAURI_SIGNING_PRIVATE_KEY || path.join(homedir(), '.sporium-signing/updater.key');
if (!process.env.TAURI_SIGNING_PRIVATE_KEY) await access(key);
const result = spawnSync(
  process.execPath,
  ['node_modules/@tauri-apps/cli/tauri.js', 'build', '--bundles', 'nsis'],
  {
    cwd: root,
    stdio: 'inherit',
    env: {
      ...process.env,
      TAURI_SIGNING_PRIVATE_KEY: key,
      TAURI_SIGNING_PRIVATE_KEY_PASSWORD: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? '',
    },
  },
);
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
const installer = path.join(
  root,
  `src-tauri/target/release/bundle/nsis/Sporium_${version}_x64-setup.exe`,
);
const signature = (await readFile(`${installer}.sig`, 'utf8')).trim();
const output = path.join(root, 'releases');
await mkdir(output, { recursive: true });
const target = path.join(output, 'SporiumLauncher.exe');
await copyFile(installer, target);
await writeFile(`${target}.sig`, `${signature}\n`);
const bytes = await readFile(target);
const digest = createHash('sha256').update(bytes).digest('hex');
await writeFile(path.join(output, 'SHA256SUMS.txt'), `${digest}  SporiumLauncher.exe\n`);
const notes = await readFile(path.join(root, `docs/releases/${version}.md`), 'utf8');
await writeFile(
  path.join(output, 'latest.json'),
  JSON.stringify(
    {
      version,
      notes,
      pub_date: new Date().toISOString(),
      size: bytes.length,
      platforms: {
        'windows-x86_64': {
          signature,
          url: `https://github.com/opensxul/sporium-launcher-v-1.0.0/releases/download/v${version}/SporiumLauncher.exe`,
        },
      },
    },
    null,
    2,
  ) + '\n',
);
console.log(`Installer: ${target}\nSHA256: ${digest}`);
