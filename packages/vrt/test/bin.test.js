import assert from "node:assert/strict";
import { mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import test from "node:test";

const wrapper = new URL("../bin/vrt.js", import.meta.url);

async function runWrapper(args, env) {
  return await new Promise((resolve) => {
    const child = spawn(process.execPath, [wrapper.pathname, ...args], {
      env: { ...process.env, ...env },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    child.on("close", (code) => resolve({ code, stdout, stderr }));
  });
}

test("npm wrapper forwards args to VRT_BIN", async () => {
  const dir = await mkdtemp(path.join(tmpdir(), "vrt-bin-"));
  const fake = path.join(dir, "vrt-fake.mjs");
  await writeFile(
    fake,
    `#!/usr/bin/env node
console.log(JSON.stringify({ argv: process.argv.slice(2) }));
`,
    { mode: 0o755 },
  );

  const result = await runWrapper(["doctor", "--json"], { VRT_BIN: fake });

  assert.equal(result.code, 0);
  assert.deepEqual(JSON.parse(result.stdout).argv, ["doctor", "--json"]);
});

test("npm wrapper exits non-zero when no binary is available", async () => {
  const result = await runWrapper(["doctor"], { VRT_BIN: "/missing/vrt" });

  assert.notEqual(result.code, 0);
  assert.match(result.stderr, /Unable to locate VRT binary/);
});
