import assert from "node:assert/strict";
import { mkdtemp, realpath, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";

import { VrtCommandError, doctor, explain, verify } from "../src/index.js";

async function fakeVrt(scriptBody) {
  const dir = await mkdtemp(path.join(tmpdir(), "vrt-sdk-"));
  const bin = path.join(dir, "vrt-fake.mjs");
  await writeFile(bin, scriptBody, { mode: 0o755 });
  return { dir, bin };
}

test("verify calls vrt verify --json and parses JSON output", async () => {
  const { dir, bin } = await fakeVrt(`#!/usr/bin/env node
console.log(JSON.stringify({ argv: process.argv.slice(2), cwd: process.cwd(), ok: true }));
`);

  const result = await verify({ root: dir, bin });

  assert.deepEqual(result.argv, ["--root", dir, "verify", "--json"]);
  assert.equal(result.cwd, await realpath(dir));
  assert.equal(result.ok, true);
});

test("verify passes mode, full, and continue flags", async () => {
  const { dir, bin } = await fakeVrt(`#!/usr/bin/env node
console.log(JSON.stringify({ argv: process.argv.slice(2) }));
`);

  const result = await verify({ root: dir, bin, mode: "merge", full: true, continue: true });

  assert.deepEqual(result.argv, [
    "--root",
    dir,
    "verify",
    "--json",
    "--mode",
    "merge",
    "--full",
    "--continue",
  ]);
});

test("doctor and explain call JSON CLI commands", async () => {
  const { dir, bin } = await fakeVrt(`#!/usr/bin/env node
console.log(JSON.stringify({ argv: process.argv.slice(2) }));
`);

  assert.deepEqual((await doctor({ root: dir, bin })).argv, ["--root", dir, "doctor", "--json"]);
  assert.deepEqual((await explain({ root: dir, bin })).argv, ["--root", dir, "explain", "--json"]);
});

test("command failures include exit code and stderr", async () => {
  const { dir, bin } = await fakeVrt(`#!/usr/bin/env node
console.error("typecheck failed");
process.exit(2);
`);

  await assert.rejects(
    () => verify({ root: dir, bin }),
    (error) => {
      assert.ok(error instanceof VrtCommandError);
      assert.equal(error.exitCode, 2);
      assert.match(error.stderr, /typecheck failed/);
      return true;
    },
  );
});
