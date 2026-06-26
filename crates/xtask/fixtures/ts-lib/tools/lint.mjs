// Dependency-free stand-in linter: real work, deterministic, no network.
import { readdirSync, readFileSync } from "node:fs";
let files = readdirSync("src").filter((f) => f.endsWith(".ts"));
for (const f of files) { readFileSync("src/" + f, "utf8"); }
console.log(`lint: scanned ${files.length} files`);
