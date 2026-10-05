import { mkdir, writeFile } from 'node:fs/promises';
import { Buffer } from 'node:buffer';
const out = new URL('../.local/loader-audit/', import.meta.url);
await mkdir(out, { recursive: true });
const endpoints = {
  fabric: 'https://meta.fabricmc.net/v2/versions/loader/1.21.1',
  forge: 'https://files.minecraftforge.net/net/minecraftforge/forge/maven-metadata.json',
  neo: 'https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml',
  skinmod: 'https://api.modrinth.com/v2/project/customskinloader/version',
};
for (const [name, url] of Object.entries(endpoints)) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${name}: HTTP ${response.status}`);
  const content = await response.text();
  await writeFile(new URL(`${name}.${name === 'neo' ? 'xml' : 'json'}`, out), content);
  if (name === 'fabric')
    console.log(
      name,
      JSON.parse(content)
        .slice(0, 3)
        .map((x) => x.loader.version),
    );
  if (name === 'forge')
    console.log(
      name,
      Object.fromEntries(
        ['1.12.2', '1.20.1', '1.21.1'].map((v) => [v, JSON.parse(content)[v]?.slice(-3)]),
      ),
    );
  if (name === 'neo')
    console.log(
      name,
      [...content.matchAll(/<version>(21\.1\.[^<]+)<\/version>/g)].slice(-3).map((x) => x[1]),
    );
  if (name === 'skinmod')
    console.log(
      name,
      JSON.parse(content)
        .slice(0, 4)
        .map((x) => ({
          id: x.id,
          name: x.name,
          loaders: x.loaders,
          game_versions: x.game_versions.slice(-4),
          files: x.files.map((f) => ({
            filename: f.filename,
            url: f.url,
            size: f.size,
            hashes: f.hashes,
          })),
        })),
    );
}
for (const [name, url] of Object.entries({
  'forge-legacy':
    'https://maven.minecraftforge.net/net/minecraftforge/forge/1.12.2-14.23.5.2864/forge-1.12.2-14.23.5.2864-installer.jar',
  'forge-modern':
    'https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.4.23/forge-1.20.1-47.4.23-installer.jar',
  'neo-installer':
    'https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.252/neoforge-21.1.252-installer.jar',
  'fabric-profile': 'https://meta.fabricmc.net/v2/versions/loader/1.21.1/0.19.5/profile/json',
})) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${name}: ${response.status}`);
  const bytes = Buffer.from(await response.arrayBuffer());
  await writeFile(new URL(`${name}.${name === 'fabric-profile' ? 'json' : 'jar'}`, out), bytes);
  console.log(name, bytes.length);
}
