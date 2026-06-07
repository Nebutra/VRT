import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import path from "node:path";

const root = process.cwd();

const docsConfig = JSON.parse(await readFile(path.join(root, "docs.json"), "utf8"));
const tokens = JSON.parse(await readFile(path.join(root, "assets/brand/brand-tokens.json"), "utf8"));
const brandDoc = await readFile(path.join(root, "docs/brand.mdx"), "utf8");

const pages = docsConfig.navigation.groups.flatMap((group) => group.pages ?? []);
assert.ok(pages.includes("docs/brand"), "Mintlify navigation must include docs/brand");

assert.equal(
  docsConfig.colors.primary,
  tokens.colors.cobalt,
  "docs.json primary color should match brand cobalt",
);

for (const match of brandDoc.matchAll(/`(\/assets\/[^`]+)`/g)) {
  const assetPath = match[1].replace(/^\//, "");
  await access(path.join(root, assetPath));
}

for (const logoPath of [docsConfig.logo?.light, docsConfig.logo?.dark, docsConfig.favicon]) {
  assert.equal(typeof logoPath, "string", "docs.json logo and favicon paths must be strings");
  await access(path.join(root, logoPath.replace(/^\//, "")));
}
