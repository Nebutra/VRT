#!/usr/bin/env node
import { spawn } from "node:child_process";
import { access } from "node:fs/promises";
import { constants } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const bin = await resolveBinary();
if (!bin) {
  console.error(
    "Unable to locate VRT binary. Set VRT_BIN or build the Rust CLI with `cargo build -p vrt-cli`.",
  );
  process.exit(1);
}

const child = spawn(bin, process.argv.slice(2), {
  stdio: "inherit",
  env: process.env,
});

child.on("error", (error) => {
  console.error(`Unable to start VRT binary: ${error.message}`);
  process.exit(1);
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});

async function resolveBinary() {
  if (process.env.VRT_BIN) {
    return (await executable(process.env.VRT_BIN)) ? process.env.VRT_BIN : null;
  }
  const candidates = [
    repoPath("target", "release", "vrt"),
    repoPath("target", "debug", "vrt"),
    "vrt",
  ].filter(Boolean);
  for (const candidate of candidates) {
    if (candidate === "vrt") {
      return candidate;
    }
    try {
      if (await executable(candidate)) {
        return candidate;
      }
    } catch {
      // Keep searching.
    }
  }
  return null;
}

async function executable(candidate) {
  try {
    await access(candidate, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function repoPath(...segments) {
  const here = path.dirname(fileURLToPath(import.meta.url));
  return path.resolve(here, "..", "..", "..", ...segments);
}
