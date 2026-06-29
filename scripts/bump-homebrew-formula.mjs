// Update Formula/vrt.rb for a new tagged release: rewrite the source-tarball
// url and its sha256. Run from the Release workflow on tag push.
//
//   node scripts/bump-homebrew-formula.mjs --version v0.1.0
//   node scripts/bump-homebrew-formula.mjs --version v0.1.0 --sha256 <hex>
//
// With no --sha256, the GitHub source tarball is downloaded and hashed.

import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const TAG_RE = /^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;
const SHA_RE = /^[0-9a-f]{64}$/;

/** Pure rewrite — exported for tests. Throws on malformed inputs. */
export function rewriteFormula(text, { version, sha256 }) {
  if (!TAG_RE.test(version)) {
    throw new Error(`invalid version tag: ${version} (expected vMAJOR.MINOR.PATCH)`);
  }
  if (!SHA_RE.test(sha256)) {
    throw new Error(`invalid sha256: ${sha256} (expected 64 lowercase hex chars)`);
  }
  const urlRe =
    /url "https:\/\/github\.com\/Nebutra\/VRT\/archive\/refs\/tags\/[^"]*\.tar\.gz"/;
  const shaRe = /sha256 "[0-9a-f]{64}"/;
  if (!urlRe.test(text)) throw new Error("formula url line not found");
  if (!shaRe.test(text)) throw new Error("formula sha256 line not found");
  return text
    .replace(
      urlRe,
      `url "https://github.com/Nebutra/VRT/archive/refs/tags/${version}.tar.gz"`,
    )
    .replace(shaRe, `sha256 "${sha256}"`);
}

async function fetchTarballSha256(version) {
  const url = `https://github.com/Nebutra/VRT/archive/refs/tags/${version}.tar.gz`;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`fetch ${url} failed: ${res.status}`);
  const buf = Buffer.from(await res.arrayBuffer());
  return createHash("sha256").update(buf).digest("hex");
}

function arg(name) {
  const hit = process.argv.find((a) => a.startsWith(`--${name}=`));
  if (hit) return hit.slice(name.length + 3);
  const idx = process.argv.indexOf(`--${name}`);
  return idx >= 0 ? process.argv[idx + 1] : undefined;
}

const isMain =
  process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (isMain) {
  const version = arg("version");
  if (!version) throw new Error("missing --version vX.Y.Z");
  const sha256 = arg("sha256") ?? (await fetchTarballSha256(version));
  const formulaPath = arg("formula") ?? path.resolve(process.cwd(), "Formula/vrt.rb");
  const text = await readFile(formulaPath, "utf8");
  await writeFile(formulaPath, rewriteFormula(text, { version, sha256 }));
  console.log(`bumped ${formulaPath} -> ${version} sha256=${sha256}`);
}
