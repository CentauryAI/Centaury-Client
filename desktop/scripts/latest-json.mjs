#!/usr/bin/env node
// C5: emit the Tauri v2 updater manifest (static JSON, v2.tauri.app/plugin/updater)
// from the `.sig` files `tauri build` writes when `bundle.createUpdaterArtifacts`
// is true and TAURI_SIGNING_PRIVATE_KEY is set. Node built-ins only.
//
// Usage:
//   node scripts/latest-json.mjs --base-url <url> [--version X.Y.Z] [--notes TEXT]
//                                [--out FILE] [dir ...]
//
//   --base-url  where the artifacts are downloadable (asset name is appended)
//   --version   advertised version (default: src-tauri/tauri.conf.json "version")
//   --notes     release notes (default: "Centaury <version>")
//   --out       write here instead of stdout
//   dir ...     searched recursively for *.sig (default: the cargo bundle dirs,
//               ../target and src-tauri/target, relative to desktop/)
//
// Updater bundle per platform (createUpdaterArtifacts: true):
//   linux   *.AppImage.sig      (deb/rpm installs don't self-update)
//   darwin  *.app.tar.gz.sig
//   windows *-setup.exe.sig (NSIS) preferred over *.msi.sig
// Platform key = `<os>-<arch>`: arch comes from a path segment containing the
// key (CI downloads each runner's artifacts into desktop-<key>/), else from the
// file name (aarch64|arm64 → aarch64, x86_64|x64|amd64 → x86_64).

import { readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const DESKTOP = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const WANTED = ["linux-x86_64", "darwin-aarch64", "darwin-x86_64", "windows-x86_64"];
const ARCHES = ["x86_64", "aarch64", "i686", "armv7"];

function die(msg) {
  console.error(`latest-json: ${msg}`);
  process.exit(1);
}

const { values: opt, positionals } = parseArgs({
  options: {
    "base-url": { type: "string" },
    version: { type: "string" },
    notes: { type: "string" },
    out: { type: "string" },
  },
  allowPositionals: true,
});

const base = opt["base-url"]?.replace(/\/+$/, "");
if (!base) die("--base-url is required");

const version = (opt.version ?? JSON.parse(readFileSync(join(DESKTOP, "src-tauri/tauri.conf.json"), "utf8")).version)
  .trim()
  .replace(/^v/, "");
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$/.test(version)) die(`not a semver version: ${version}`);

const roots = positionals.length
  ? positionals.map((d) => resolve(d))
  : [resolve(DESKTOP, "../target"), resolve(DESKTOP, "src-tauri/target")];

function* walk(dir) {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return; // missing default root is fine
  }
  for (const e of entries) {
    const p = join(dir, e.name);
    if (e.isDirectory()) yield* walk(p);
    else if (e.isFile() && e.name.endsWith(".sig")) yield p;
  }
}

/** `.sig` path → { os, rank } for updater bundles, null for everything else. */
function classify(name) {
  if (name.endsWith(".AppImage.sig")) return { os: "linux", rank: 1 };
  if (name.endsWith(".app.tar.gz.sig")) return { os: "darwin", rank: 1 };
  if (/-setup\.exe\.sig$/.test(name)) return { os: "windows", rank: 2 };
  if (name.endsWith(".msi.sig")) return { os: "windows", rank: 1 };
  return null;
}

function archOf(os, sigPath, root) {
  const rel = sigPath.slice(root.length).split(sep);
  for (const seg of rel.slice(0, -1)) {
    const a = ARCHES.find((x) => seg.includes(`${os}-${x}`));
    if (a) return a;
  }
  const n = basename(sigPath).toLowerCase();
  if (/(aarch64|arm64)/.test(n)) return "aarch64";
  if (/(x86_64|x64|amd64)/.test(n)) return "x86_64";
  if (/(i686|x86)/.test(n)) return "i686";
  if (/armv7|armhf/.test(n)) return "armv7";
  return null;
}

const picked = {}; // key → { rank, sig }
for (const root of roots) {
  if (positionals.length && !statSync(root, { throwIfNoEntry: false })?.isDirectory()) die(`not a directory: ${root}`);
  for (const sig of walk(root)) {
    const c = classify(basename(sig));
    if (!c) continue;
    const arch = archOf(c.os, sig, root);
    if (!arch) die(`can't tell the architecture of ${sig} — put it under a dir named like ${c.os}-x86_64`);
    const key = `${c.os}-${arch}`;
    const prev = picked[key];
    if (prev && prev.rank > c.rank) continue;
    if (prev && prev.rank === c.rank && prev.sig !== sig) die(`two ${key} updater bundles: ${prev.sig} and ${sig}`);
    picked[key] = { rank: c.rank, sig };
  }
}

const keys = Object.keys(picked).sort();
if (!keys.length) die(`no updater .sig found under ${roots.join(", ")} — build with createUpdaterArtifacts + TAURI_SIGNING_PRIVATE_KEY`);
for (const k of WANTED) if (!picked[k]) console.error(`latest-json: warning: no ${k} bundle, platform omitted`);

const platforms = {};
for (const k of keys) {
  const { sig } = picked[k];
  const signature = readFileSync(sig, "utf8").trim();
  if (!signature) die(`empty signature: ${sig}`);
  const asset = basename(sig).slice(0, -".sig".length);
  platforms[k] = { signature, url: `${base}/${encodeURIComponent(asset)}` };
}

const manifest = {
  version,
  notes: opt.notes ?? `Centaury ${version}`,
  pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"), // RFC 3339
  platforms,
};
const json = `${JSON.stringify(manifest, null, 2)}\n`;
if (opt.out) {
  writeFileSync(opt.out, json);
  console.error(`latest-json: wrote ${opt.out} (${keys.join(", ")})`);
} else {
  process.stdout.write(json);
}
