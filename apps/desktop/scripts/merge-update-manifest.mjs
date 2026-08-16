#!/usr/bin/env node
/**
 * Merges the CI-built macOS/Linux updater entries into a release's
 * `latest.json`, then expands it so every install can find *its own* update.
 *
 * The Windows release script (scripts/release-public.ps1 in the dev repo)
 * composes `latest.json` with the Windows keys only — it runs on the machine
 * that built the Windows installers and cannot know the macOS/Linux artifact
 * names or signatures. Those are built later by release-build.yml on this
 * (public) repo, which uploads them as release assets and then runs this
 * script to fold them into the manifest:
 *
 * 1. **Base keys from assets.** Every `*.app.tar.gz` / `*.AppImage` asset
 *    with an uploaded `.sig` becomes a `darwin-*` / `linux-x86_64` entry.
 *    No `.sig`, no entry: a guessed signature would make the update fail
 *    verification *after* downloading.
 *
 * 2. **macOS never matches a universal key.** A universal build is published
 *    as `darwin-universal`, but the plugin asks for `darwin-{arch}` — the
 *    running process's arch. There is no "universal" fallback in the plugin,
 *    so both arch keys are aliased onto the universal artifact.
 *
 * 3. **Per-installer keys.** The plugin looks up `{os}-{arch}-{installer}`
 *    before `{os}-{arch}`, which is how an MSI install is kept on MSI and an
 *    NSIS install on NSIS. Installer-scoped keys are added for every entry.
 *
 * Existing explicit keys are never overwritten, and running the script twice
 * changes nothing.
 *
 * Usage:
 *   node merge-update-manifest.mjs --manifest latest.json [--assets assets.json]
 *                                  [--sig-dir dir] [--out out.json]
 *   node merge-update-manifest.mjs --self-test
 *
 * `--assets` is `gh release view --json assets` output; `--sig-dir` holds the
 * downloaded `*.sig` files.
 */

import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';

/** Maps an updater artifact file name to its bare platform key, or null. */
export function platformOf(name) {
  const n = name.toLowerCase();
  if (n.endsWith('.app.tar.gz')) {
    if (n.includes('universal')) return 'darwin-universal';
    if (n.includes('aarch64') || n.includes('arm64')) return 'darwin-aarch64';
    return 'darwin-x86_64';
  }
  if (n.endsWith('.appimage')) return 'linux-x86_64';
  return null;
}

/** Maps an artifact file name to the updater's installer id, or null. */
export function installerOf(name) {
  const n = name.toLowerCase();
  if (n.endsWith('-setup.exe')) return 'nsis';
  if (n.endsWith('.msi')) return 'msi';
  if (n.endsWith('.app.tar.gz')) return 'app';
  if (n.endsWith('.appimage')) return 'appimage';
  return null;
}

const fileName = (url) => decodeURIComponent(url.split('/').pop() || '');

/**
 * @param {object} manifest parsed latest.json
 * @param {Array<{name: string, url: string}>} assets release assets
 * @param {(name: string) => string|null} readSig reads a `.sig` file's contents
 * @returns {{manifest: object, added: string[]}}
 */
export function merge(manifest, assets = [], readSig = () => null) {
  const platforms = { ...(manifest.platforms || {}) };
  const added = [];
  const put = (key, value) => {
    if (platforms[key]) return; // an explicit entry always wins
    platforms[key] = value;
    added.push(key);
  };

  // 1. Base entries for the CI-built artifacts, signature from the .sig asset.
  for (const asset of assets) {
    const key = platformOf(asset.name);
    if (!key || platforms[key]) continue;
    const signature = readSig(`${asset.name}.sig`);
    if (!signature) continue;
    put(key, { signature: signature.trim(), url: asset.url });
  }

  // 2. A universal macOS artifact serves both architectures.
  const universal = platforms['darwin-universal'];
  if (universal) {
    put('darwin-aarch64', universal);
    put('darwin-x86_64', universal);
  }

  // 3. Installer-scoped keys for every bare entry now present.
  for (const [key, value] of Object.entries({ ...platforms })) {
    if (key.split('-').length > 2) continue; // already installer-scoped
    const installer = installerOf(fileName(value.url));
    if (!installer) continue;
    // The universal entry is not a real target key, so scope the aliases
    // instead — that is what a running app actually looks up.
    if (key === 'darwin-universal') {
      put(`darwin-aarch64-${installer}`, value);
      put(`darwin-x86_64-${installer}`, value);
    } else {
      put(`${key}-${installer}`, value);
    }
  }

  return { manifest: { ...manifest, platforms }, added };
}

/* ---------------- self test ---------------- */

function selfTest() {
  const fail = (msg) => {
    console.error(`FAIL: ${msg}`);
    process.exitCode = 1;
  };
  const eq = (a, b, msg) => {
    if (JSON.stringify(a) !== JSON.stringify(b)) {
      fail(`${msg}\n  got:      ${JSON.stringify(a)}\n  expected: ${JSON.stringify(b)}`);
    }
  };

  const nsis = { signature: 'sig-nsis', url: 'https://x/Conduit_0.2.0_x64-setup.exe' };
  const msi = { signature: 'sig-msi', url: 'https://x/Conduit_0.2.0_x64_en-US.msi' };
  const base = {
    version: '0.2.0',
    platforms: { 'windows-x86_64': nsis, 'windows-x86_64-nsis': nsis, 'windows-x86_64-msi': msi },
  };
  const assets = [
    { name: 'Conduit_0.2.0_universal.app.tar.gz', url: 'https://x/Conduit_0.2.0_universal.app.tar.gz' },
    { name: 'Conduit_0.2.0_universal.dmg', url: 'https://x/Conduit_0.2.0_universal.dmg' },
    { name: 'conduit_0.2.0_amd64.AppImage', url: 'https://x/conduit_0.2.0_amd64.AppImage' },
    { name: 'conduit_0.2.0_amd64.deb', url: 'https://x/conduit_0.2.0_amd64.deb' },
    { name: 'conduit-0.2.0-1.x86_64.rpm', url: 'https://x/conduit-0.2.0-1.x86_64.rpm' },
  ];
  const sigs = {
    'Conduit_0.2.0_universal.app.tar.gz.sig': 'sig-mac\n',
    'conduit_0.2.0_amd64.AppImage.sig': 'sig-linux\n',
  };
  const readSig = (n) => sigs[n] ?? null;

  const { manifest: out } = merge(base, assets, readSig);
  const p = out.platforms;
  const mac = { signature: 'sig-mac', url: assets[0].url };
  const linux = { signature: 'sig-linux', url: assets[2].url };

  // macOS: both real target keys must resolve, since `darwin-universal` never does.
  eq(p['darwin-universal'], mac, 'darwin-universal from asset + sig');
  eq(p['darwin-aarch64'], mac, 'darwin-aarch64 alias');
  eq(p['darwin-x86_64'], mac, 'darwin-x86_64 alias');
  eq(p['darwin-aarch64-app'], mac, 'darwin-aarch64-app alias');
  eq(p['darwin-x86_64-app'], mac, 'darwin-x86_64-app alias');

  // Linux: the AppImage is the updater artifact; deb/rpm belong to apt/dnf.
  eq(p['linux-x86_64'], linux, 'linux-x86_64 from asset + sig');
  eq(p['linux-x86_64-appimage'], linux, 'appimage key');
  if (Object.keys(p).some((k) => /deb|rpm|dmg/.test(k))) fail('deb/rpm/dmg must not become platform keys');

  // Windows entries from the release script survive untouched.
  eq(p['windows-x86_64'], nsis, 'windows nsis key preserved');
  eq(p['windows-x86_64-msi'], msi, 'windows msi key preserved');

  // An artifact whose .sig was not uploaded must be skipped, not guessed at.
  const { manifest: noSig } = merge(base, assets, () => null);
  if (noSig.platforms['darwin-universal']) fail('darwin key added without a signature');
  if (noSig.platforms['linux-x86_64']) fail('linux key added without a signature');

  // Explicit entries win over generated ones.
  const explicit = {
    ...base,
    platforms: { ...base.platforms, 'darwin-universal': { signature: 's', url: 'https://x/other.app.tar.gz' } },
  };
  eq(
    merge(explicit, assets, readSig).manifest.platforms['darwin-universal'].url,
    'https://x/other.app.tar.gz',
    'explicit key preserved',
  );

  // Idempotent: merging twice changes nothing.
  const once = merge(base, assets, readSig).manifest;
  const twice = merge(once, assets, readSig);
  eq(twice.added, [], 'second pass adds nothing');

  // A per-arch (non-universal) macOS build keys straight to its arch.
  const archAssets = [{ name: 'Conduit_0.2.0_aarch64.app.tar.gz', url: 'https://x/Conduit_0.2.0_aarch64.app.tar.gz' }];
  const archOut = merge({ version: '0.2.0', platforms: {} }, archAssets, () => 'sig-arm\n').manifest;
  eq(archOut.platforms['darwin-aarch64'].url, archAssets[0].url, 'per-arch mac artifact keys to its arch');

  if (!process.exitCode) console.log('merge-update-manifest self-test: OK');
}

/* ---------------- cli ---------------- */

function arg(name) {
  const i = process.argv.indexOf(`--${name}`);
  return i === -1 ? null : process.argv[i + 1];
}

if (process.argv.includes('--self-test')) {
  selfTest();
} else {
  const manifestPath = arg('manifest');
  if (!manifestPath) {
    console.error('usage: merge-update-manifest.mjs --manifest latest.json [--assets a.json] [--sig-dir d] [--out o.json]');
    process.exit(2);
  }
  const assetsPath = arg('assets');
  const sigDir = arg('sig-dir');
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  const rawAssets = assetsPath ? JSON.parse(readFileSync(assetsPath, 'utf8')) : [];
  const assets = Array.isArray(rawAssets) ? rawAssets : (rawAssets.assets ?? []);
  const readSig = (name) => {
    if (!sigDir) return null;
    const p = join(sigDir, name);
    return existsSync(p) ? readFileSync(p, 'utf8') : null;
  };
  const { manifest: out, added } = merge(manifest, assets, readSig);
  writeFileSync(arg('out') || manifestPath, `${JSON.stringify(out, null, 2)}\n`);
  console.log(added.length ? `added platform keys: ${added.join(', ')}` : 'no new platform keys');
}
