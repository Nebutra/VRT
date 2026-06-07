import { constants } from "node:fs";
import { access } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

export async function resolveBinary(options = {}) {
  const packageRoot = options.packageRoot ?? defaultPackageRoot();
  const env = options.env ?? process.env;
  const platform = options.platform ?? process.platform;
  const arch = options.arch ?? process.arch;
  const pathBinary = options.pathBinary ?? true;

  if (env.VRT_BIN) {
    return (await executable(env.VRT_BIN)) ? env.VRT_BIN : null;
  }

  const candidates = [
    packagedBinary(packageRoot, platform, arch),
    repoBinary(packageRoot, "target", "release", executableName(platform)),
    repoBinary(packageRoot, "target", "debug", executableName(platform)),
    pathBinary ? "vrt" : null,
  ].filter(Boolean);

  for (const candidate of candidates) {
    if (candidate === "vrt") {
      return candidate;
    }
    if (await executable(candidate)) {
      return candidate;
    }
  }

  return null;
}

export function unsupportedBinaryMessage(options = {}) {
  const platform = options.platform ?? process.platform;
  const arch = options.arch ?? process.arch;
  return `Unable to locate VRT binary for ${platform}-${arch}. Set VRT_BIN to an executable VRT binary, install a package that includes bin/${platform}-${arch}/${executableName(platform)}, or build the Rust CLI with \`cargo build -p vrt-cli\`.`;
}

export function packagedBinary(packageRoot, platform, arch) {
  return path.join(packageRoot, "bin", `${platform}-${arch}`, executableName(platform));
}

function repoBinary(packageRoot, ...segments) {
  return path.resolve(packageRoot, "..", "..", ...segments);
}

async function executable(candidate) {
  try {
    await access(candidate, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function executableName(platform) {
  return platform === "win32" ? "vrt.exe" : "vrt";
}

function defaultPackageRoot() {
  const here = path.dirname(fileURLToPath(import.meta.url));
  return path.resolve(here, "..");
}
