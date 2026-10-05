import { readFile, writeFile } from 'node:fs/promises';
const versions = JSON.parse(await readFile('.local/loader-audit/skinmod.json', 'utf8'));
const version = versions.find((v) => v.id === 'OLaesh5y');
if (!version || version.project_id !== 'idMHQ4n2') throw new Error('Unexpected skin project');
const file = version.files.find((f) => f.primary);
await writeFile(
  'src-tauri/skinmod.json',
  JSON.stringify(
    {
      project: 'https://github.com/xfl03/MCCustomSkinLoader',
      version: version.version_number,
      minecraft: version.game_versions,
      url: file.url,
      sha1: file.hashes.sha1,
      size: file.size,
    },
    null,
    2,
  ) + '\n',
);
