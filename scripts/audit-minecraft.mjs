import { mkdir, writeFile } from 'node:fs/promises';
const directory = new URL('../.local/minecraft-audit/', import.meta.url);
await mkdir(directory, { recursive: true });
async function json(url) {
  const response = await fetch(url, {
    headers: { 'User-Agent': 'Sporium/0.1.0 (development audit)' },
  });
  if (!response.ok) throw new Error(`${response.status}: ${url}`);
  return response.json();
}
const manifest = await json('https://piston-meta.mojang.com/mc/game/version_manifest_v2.json');
await writeFile(new URL('manifest.json', directory), JSON.stringify(manifest, null, 2));
for (const id of [
  manifest.latest.release,
  manifest.latest.snapshot,
  'b1.7.3',
  'a1.2.6',
  '1.21.4',
]) {
  const entry = manifest.versions.find((item) => item.id === id);
  const value = await json(entry.url);
  await writeFile(new URL(`${id}.json`, directory), JSON.stringify(value, null, 2));
  console.log(
    JSON.stringify({
      id,
      manifest: entry,
      java: value.javaVersion,
      mainClass: value.mainClass,
      arguments: value.arguments ?? value.minecraftArguments,
      assetIndex: value.assetIndex,
      logging: value.logging,
      libraries: value.libraries.length,
      exampleLibraries: value.libraries.filter((item) => item.natives || item.rules).slice(0, 5),
    }),
  );
}
for (const name of ['reqwest', 'zip', 'sha1', 'sha2', 'regex', 'sysinfo']) {
  const value = await json(`https://crates.io/api/v1/crates/${name}`);
  console.log(`${name}: ${value.crate.max_stable_version}`);
}
for (const major of [8, 21, 25]) {
  const value = await json(
    `https://api.adoptium.net/v3/assets/latest/${major}/hotspot?architecture=x64&image_type=jre&os=windows&vendor=eclipse`,
  );
  await writeFile(new URL(`java-${major}.json`, directory), JSON.stringify(value, null, 2));
  console.log(JSON.stringify({ major, binary: value[0]?.binary, version: value[0]?.version }));
}
