import { chmod, copyFile, mkdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

export async function stagePlatformBinary(options = {}) {
  const packageRoot = options.packageRoot ?? defaultPackageRoot();
  const platform = options.platform ?? process.platform;
  const arch = options.arch ?? process.arch;
  const source = options.source ?? defaultSource(platform);
  const destination = stagedBinaryPath({ packageRoot, platform, arch });

  await mkdir(path.dirname(destination), { recursive: true });
  await copyFile(source, destination);
  if (platform !== "win32") {
    await chmod(destination, 0o755);
  }
  return destination;
}

export function stagedBinaryPath({ packageRoot, platform, arch }) {
  return path.join(packageRoot, "bin", `${platform}-${arch}`, executableName(platform));
}

function defaultSource(platform) {
  const root = path.resolve(defaultPackageRoot(), "..", "..");
  return path.join(root, "target", "release", executableName(platform));
}

function executableName(platform) {
  return platform === "win32" ? "vrt.exe" : "vrt";
}

function defaultPackageRoot() {
  return path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const sourceArg = process.argv.find((arg) => arg.startsWith("--source="));
  const platformArg = process.argv.find((arg) => arg.startsWith("--platform="));
  const archArg = process.argv.find((arg) => arg.startsWith("--arch="));
  const staged = await stagePlatformBinary({
    source: sourceArg?.slice("--source=".length),
    platform: platformArg?.slice("--platform=".length),
    arch: archArg?.slice("--arch=".length),
  });
  console.log(`staged ${staged}`);
}
