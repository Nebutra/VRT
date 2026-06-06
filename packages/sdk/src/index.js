import { spawn } from "node:child_process";
import { access } from "node:fs/promises";
import { constants } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export class VrtCommandError extends Error {
  constructor(message, { exitCode, stdout, stderr, command }) {
    super(message);
    this.name = "VrtCommandError";
    this.exitCode = exitCode;
    this.stdout = stdout;
    this.stderr = stderr;
    this.command = command;
  }
}

export async function verify(options = {}) {
  const args = ["verify", "--json"];
  if (options.mode) {
    args.push("--mode", options.mode);
  }
  if (options.full) {
    args.push("--full");
  }
  if (options.continue) {
    args.push("--continue");
  }
  return runJson(args, options);
}

export async function doctor(options = {}) {
  return runJson(["doctor", "--json"], options);
}

export async function explain(options = {}) {
  return runJson(["explain", "--json"], options);
}

export async function bench(options = {}) {
  return runJson(["bench", "--json"], options);
}

export async function runJson(args, options = {}) {
  const root = path.resolve(options.root ?? process.cwd());
  const bin = await resolveVrtBinary(options.bin);
  const commandArgs = ["--root", root, ...args];
  const result = await runCommand(bin, commandArgs, {
    cwd: root,
    env: options.env,
  });
  try {
    return JSON.parse(result.stdout);
  } catch (error) {
    throw new VrtCommandError(`VRT returned invalid JSON: ${error.message}`, {
      ...result,
      command: [bin, ...commandArgs],
    });
  }
}

export async function runCommand(command, args, options = {}) {
  return await new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: options.cwd,
      env: { ...process.env, ...(options.env ?? {}) },
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
    child.on("error", (error) => {
      reject(
        new VrtCommandError(`Failed to start VRT: ${error.message}`, {
          exitCode: null,
          stdout,
          stderr,
          command: [command, ...args],
        }),
      );
    });
    child.on("close", (exitCode) => {
      const result = {
        exitCode,
        stdout,
        stderr,
        command: [command, ...args],
      };
      if (exitCode === 0) {
        resolve(result);
      } else {
        reject(new VrtCommandError(`VRT command failed with exit code ${exitCode}`, result));
      }
    });
  });
}

export async function resolveVrtBinary(explicitBin) {
  const candidates = [
    explicitBin,
    process.env.VRT_BIN,
    repoBinary("target", "release", "vrt"),
    repoBinary("target", "debug", "vrt"),
    "vrt",
  ].filter(Boolean);

  for (const candidate of candidates) {
    if (candidate === "vrt") {
      return candidate;
    }
    const resolved = path.resolve(candidate);
    try {
      await access(resolved, constants.X_OK);
      return resolved;
    } catch {
      // Try the next candidate.
    }
  }
  return "vrt";
}

function repoBinary(...segments) {
  const here = path.dirname(fileURLToPath(import.meta.url));
  return path.resolve(here, "..", "..", "..", ...segments);
}
