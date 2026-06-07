import assert from "node:assert/strict";
import { mkdir, mkdtemp, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import test from "node:test";
import { resolveBinary, unsupportedBinaryMessage } from "../src/resolve-binary.js";
import { stagePlatformBinary, stagedBinaryPath } from "../scripts/stage-platform-binary.mjs";

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

test("resolveBinary prefers packaged platform binary before developer fallbacks", async () => {
  const dir = await mkdtemp(path.join(tmpdir(), "vrt-pkg-"));
  const packagedDir = path.join(dir, "bin", "darwin-arm64");
  await mkdir(packagedDir, { recursive: true });
  const packaged = path.join(packagedDir, "vrt");
  await writeFile(packaged, "#!/bin/sh\n", { mode: 0o755 });

  const resolved = await resolveBinary({
    packageRoot: dir,
    env: {},
    platform: "darwin",
    arch: "arm64",
    pathBinary: false,
  });

  assert.equal(resolved, packaged);
});

test("unsupportedBinaryMessage includes exact platform key and recovery options", () => {
  const message = unsupportedBinaryMessage({
    platform: "linux",
    arch: "s390x",
  });

  assert.match(message, /linux-s390x/);
  assert.match(message, /VRT_BIN/);
  assert.match(message, /cargo build -p vrt-cli/);
});

test("stagePlatformBinary copies native binary into packaged platform layout", async () => {
  const dir = await mkdtemp(path.join(tmpdir(), "vrt-stage-"));
  const source = path.join(dir, "target", "release", "vrt");
  await mkdir(path.dirname(source), { recursive: true });
  await writeFile(source, "#!/bin/sh\necho staged\n", { mode: 0o755 });

  const staged = await stagePlatformBinary({
    source,
    packageRoot: dir,
    platform: "linux",
    arch: "x64",
  });

  assert.equal(staged, path.join(dir, "bin", "linux-x64", "vrt"));
  assert.equal(await resolveBinary({
    packageRoot: dir,
    env: {},
    platform: "linux",
    arch: "x64",
    pathBinary: false,
  }), staged);
  assert.ok((await stat(staged)).mode & 0o111);
});

test("stagedBinaryPath uses exe suffix on Windows", () => {
  assert.equal(
    stagedBinaryPath({
      packageRoot: "/pkg",
      platform: "win32",
      arch: "x64",
    }),
    path.join("/pkg", "bin", "win32-x64", "vrt.exe"),
  );
});
