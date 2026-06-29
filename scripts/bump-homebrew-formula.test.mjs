import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { test } from "node:test";

import { rewriteFormula } from "./bump-homebrew-formula.mjs";

const SHA = "a".repeat(64);

function sampleFormula() {
  return [
    "class Vrt < Formula",
    '  url "https://github.com/Nebutra/VRT/archive/refs/tags/v0.1.0.tar.gz"',
    '  sha256 "0000000000000000000000000000000000000000000000000000000000000000"',
    "end",
    "",
  ].join("\n");
}

test("rewrites url and sha256 for a new tag", () => {
  const out = rewriteFormula(sampleFormula(), { version: "v1.2.3", sha256: SHA });
  assert.match(out, /archive\/refs\/tags\/v1\.2\.3\.tar\.gz/);
  assert.match(out, new RegExp(`sha256 "${SHA}"`));
  assert.doesNotMatch(out, /v0\.1\.0\.tar\.gz/);
});

test("rejects a non-semver tag", () => {
  assert.throws(() => rewriteFormula(sampleFormula(), { version: "1.2.3", sha256: SHA }), /invalid version/);
});

test("rejects a malformed sha256", () => {
  assert.throws(() => rewriteFormula(sampleFormula(), { version: "v1.2.3", sha256: "xyz" }), /invalid sha256/);
});

test("fails loudly if the formula has no url/sha lines", () => {
  assert.throws(() => rewriteFormula("class Vrt < Formula\nend\n", { version: "v1.2.3", sha256: SHA }), /url line not found/);
});

test("the real committed formula is rewritable (shape stays valid)", async () => {
  const formula = await readFile(path.resolve(process.cwd(), "Formula/vrt.rb"), "utf8");
  const out = rewriteFormula(formula, { version: "v9.9.9", sha256: SHA });
  assert.match(out, /archive\/refs\/tags\/v9\.9\.9\.tar\.gz/);
  assert.match(out, new RegExp(`sha256 "${SHA}"`));
});
