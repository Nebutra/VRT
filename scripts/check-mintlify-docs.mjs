import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";

const root = process.cwd();
const config = JSON.parse(await readFile(path.join(root, "docs.json"), "utf8"));

assert.equal(config.$schema, "https://mintlify.com/docs.json");
assert.equal(typeof config.name, "string");
assert.equal(typeof config.theme, "string");
assert.ok(config.colors?.primary, "docs.json must define colors.primary");
assert.ok(config.navigation?.groups?.length, "docs.json must define navigation groups");

const pages = config.navigation.groups.flatMap((group) => group.pages ?? []);
assert.ok(pages.length > 0, "navigation must include pages");

for (const page of pages) {
  const file = path.join(root, `${page}.mdx`);
  const source = await readFile(file, "utf8");
  assert.match(source, /^---\n[\s\S]*?\n---/, `${page}.mdx must include frontmatter`);
}

console.log(`Mintlify docs check passed: ${pages.length} navigation pages`);
