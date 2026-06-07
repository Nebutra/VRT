import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

test("release workflow builds, aggregates, and packs platform npm binaries", async () => {
  const workflow = await readFile(".github/workflows/release.yml", "utf8");

  assert.match(workflow, /name:\s*Release/);
  assert.match(workflow, /macos-latest/);
  assert.match(workflow, /ubuntu-latest/);
  assert.match(workflow, /windows-latest/);
  assert.match(workflow, /cargo test --workspace/);
  assert.match(workflow, /cargo clippy --workspace --all-targets -- -D warnings/);
  assert.match(workflow, /npm test/);
  assert.match(workflow, /npm run docs:check/);
  assert.match(workflow, /npm --workspace vrt run build:binary/);
  assert.match(workflow, /npm --workspace vrt run stage:binary -- --platform=\$\{\{ matrix\.node_platform \}\} --arch=\$\{\{ matrix\.node_arch \}\}/);
  assert.match(workflow, /actions\/upload-artifact@/);
  assert.match(workflow, /packages\/vrt\/bin\/\$\{\{ matrix\.node_platform \}\}-\$\{\{ matrix\.node_arch \}\}/);
  assert.match(workflow, /\n  assemble:\n/);
  assert.match(workflow, /needs:\s*package/);
  assert.match(workflow, /actions\/download-artifact@/);
  assert.match(workflow, /name:\s*vrt-binary-\$\{\{ matrix\.platform \}\}/);
  assert.match(workflow, /name:\s*vrt-binary-linux-x64/);
  assert.match(workflow, /path:\s*packages\/vrt\/bin\/linux-x64/);
  assert.match(workflow, /name:\s*vrt-binary-darwin-arm64/);
  assert.match(workflow, /path:\s*packages\/vrt\/bin\/darwin-arm64/);
  assert.match(workflow, /name:\s*vrt-binary-win32-x64/);
  assert.match(workflow, /path:\s*packages\/vrt\/bin\/win32-x64/);
  assert.match(workflow, /npm pack --workspace vrt/);
  assert.match(workflow, /name:\s*vrt-npm-package/);
  assert.match(workflow, /publish_to_npm:/);
  assert.match(workflow, /\n  publish:\n/);
  assert.match(workflow, /needs:\s*assemble/);
  assert.match(workflow, /environment:\s*npm/);
  assert.match(workflow, /id-token:\s*write/);
  assert.match(workflow, /actions\/download-artifact@/);
  assert.match(workflow, /name:\s*vrt-npm-package/);
  assert.match(workflow, /npm publish vrt-\*\.tgz --provenance --access public/);

  const packageJob = workflow.split(/\n  assemble:\n/)[0];
  assert.doesNotMatch(
    packageJob,
    /npm pack --workspace vrt/,
    "per-platform jobs must upload binaries, not same-name npm packages",
  );
});
