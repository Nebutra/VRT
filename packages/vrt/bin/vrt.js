#!/usr/bin/env node
import { spawn } from "node:child_process";
import { resolveBinary, unsupportedBinaryMessage } from "../src/resolve-binary.js";

const bin = await resolveBinary();
if (!bin) {
  console.error(unsupportedBinaryMessage());
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
