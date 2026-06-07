import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";

const root = process.cwd();

async function readJson(file) {
  return JSON.parse(await readFile(path.join(root, file), "utf8"));
}

const license = await readFile(path.join(root, "LICENSE"), "utf8");
assert.match(license, /^MIT License\n/);
assert.match(license, /Copyright \(c\) 2026 Nebutra/);

const contributing = await readFile(path.join(root, "CONTRIBUTING.md"), "utf8");
assert.match(contributing, /^# Contributing\n/);
assert.match(contributing, /vrt verify --json/);
assert.match(contributing, /skipped checks are not passed/i);

const readme = await readFile(path.join(root, "README.md"), "utf8");
assert.match(readme, /\[Contributing\]\(CONTRIBUTING\.md\)/);
assert.match(readme, /MIT/);

for (const file of [
  "packages/vrt/package.json",
  "packages/sdk/package.json",
  "packages/github-action/package.json",
]) {
  const pkg = await readJson(file);
  assert.equal(pkg.license, "MIT", `${file} must be MIT licensed`);
  assert.equal(
    pkg.repository?.url,
    "git+https://github.com/nebutra/vrt.git",
    `${file} must point at the VRT repository`,
  );
  assert.equal(
    pkg.repository?.type,
    "git",
    `${file} must use a git repository descriptor`,
  );
  assert.equal(typeof pkg.description, "string", `${file} must describe the package`);
  assert.ok(pkg.description.length > 20, `${file} description is too terse`);
}

const wrapper = await readJson("packages/vrt/package.json");
assert.equal(wrapper.bin?.vrt, "./bin/vrt.js");
assert.ok(wrapper.files?.includes("bin"));
assert.ok(wrapper.files?.includes("src"));

const sdk = await readJson("packages/sdk/package.json");
assert.equal(sdk.types, "./src/index.d.ts");

const action = await readJson("packages/github-action/package.json");
assert.equal(action.main, "./src/index.js");
