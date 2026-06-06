# VRT

![VRT hero](assets/vrt-hero.png)

**Stop making coding agents wait for the wrong checks.**

VRT is an agent-native local verification runtime. It looks at the current Git diff, understands the JS/TS project, plans the smallest serious verification path, runs project-owned checks, and writes an auditable evidence ledger before CI.

The promise is not "skip tests." The promise is:

- Run high-signal checks first.
- Avoid unnecessary full builds during small local loops.
- Preserve raw logs and explain failures.
- Report skipped checks as residual risk, never as passed.
- Give agents JSON they can act on without reading thousands of log lines.

```bash
cargo install --path crates/vrt-cli
vrt init
vrt verify --json
```

## Why Agents Need This

Coding agents are fast at edits and slow at verification judgment. A small component change often triggers a full build, long test run, noisy failure, then another full rerun after the agent patches the wrong thing.

VRT inserts a verification planning layer between Git and CI:

```text
Git diff -> Project profile -> Capability graph -> Verification plan -> Runner -> Evidence ledger
```

That gives agents a local feedback loop with lower latency and a better audit trail.

## What You Get In v0

- JS/TS project profiling from `package.json`, lockfiles, `tsconfig`, Next/Vite configs, and known tool configs.
- Capability graph from existing scripts such as `typecheck`, `lint`, `test`, and `build`.
- Git diff risk tags for UI, API, shared package, database, auth, billing, env, infra, and CI changes.
- Dev/merge/release verification modes.
- Raw logs under `.vrt/evidence/<evidence-id>/`.
- Human report and agent JSON.
- Failure explanation from the latest evidence.
- `verify --continue` evidence stitching after failures.
- Agent skill installer.
- Bench summary for avoided expensive checks.
- MCP stdio server with structured tools and no arbitrary shell tool.
- TypeScript SDK and npm bin wrapper packages.
- SARIF and JUnit export from latest evidence.

## 30-Second Tryout

From this repo:

```bash
cargo test --workspace
npm test
cargo run -q -p vrt-cli -- --root examples/next-single-app doctor
```

In a JS/TS project:

```bash
vrt init
vrt doctor
vrt verify
vrt explain
vrt skill install
vrt report --format sarif --output .vrt/reports/vrt.sarif
vrt report --format junit --output .vrt/reports/vrt.junit.xml
printf '{"jsonrpc":"2.0","id":1,"method":"tools/list"}\n' | vrt mcp serve
```

For agents:

```bash
vrt verify --json
vrt explain --json
vrt mcp serve
```

From Node:

```js
import { verify, explain } from "@vrt/sdk";

const evidence = await verify({ mode: "dev" });
const explanation = await explain();
```

## Example Output Shape

```text
Verified in 18.4s

Ran:
- workspace-typecheck: passed
- workspace-test: passed

Skipped:
- workspace-build
  Residual risk: Production bundler behavior not verified.

Confidence:
- local: high
- merge: low
- release: insufficient
```

The skipped build is not reported as passed. That is the product line.

## Growth Loop

VRT is designed to make every local verification run produce reusable proof:

1. Agent changes code.
2. VRT chooses the next useful check.
3. Failure output is compressed into root-cause candidates.
4. Evidence is stored with session id, diff hash, profile hash, raw logs, and confidence.
5. The final agent report tells the human what was proven and what was not.

The viral surface is the evidence report: maintainers can ask contributors and agents to attach VRT output before CI.

## Commands

```bash
vrt init
vrt doctor
vrt verify
vrt verify --json
vrt verify --mode merge
vrt verify --mode release
vrt verify --full
vrt verify --continue
vrt explain
vrt explain --json
vrt skill install
vrt bench
vrt report --format sarif --output .vrt/reports/vrt.sarif
vrt report --format junit --output .vrt/reports/vrt.junit.xml
vrt mcp serve
```

`vrt mcp serve` exposes structured JSON-RPC tools over stdio:

- `analyze_change`
- `plan_verification`
- `run_verification`
- `explain_failure`
- `get_evidence`
- `escalate_verification`

It deliberately does not expose `run_any_shell_command`. Verification execution still goes through VRT plans and project-owned capabilities.

## Packages

```text
crates/vrt-core   Rust runtime library
crates/vrt-cli    Native CLI binary
packages/sdk      ESM SDK for agents and Node tooling
packages/vrt      npm bin wrapper for the native CLI
```

The SDK calls the native CLI with JSON output and preserves command failures with exit code, stdout, and stderr. The npm wrapper forwards to `VRT_BIN` when set, otherwise it looks for a locally built Rust binary.

## What VRT Is

VRT is a verification planning and evidence layer between Git and CI. It reuses your existing tools instead of replacing them: TypeScript, ESLint, Biome, Vitest, Jest, Playwright, Prisma, Turborepo, Nx, and your package manager scripts.

## What VRT Is Not

VRT is not a CI replacement, build-system replacement, test runner, typechecker, or log-compression trick. Acceleration comes from planning and sequencing verification, not from pretending skipped checks passed.

## Evidence Files

Every `vrt verify` writes:

- `.vrt/profile.json`
- `.vrt/config.toml`
- `.vrt/latest.json`
- `.vrt/evidence/<evidence-id>/evidence.json`
- `.vrt/evidence/<evidence-id>/*.raw.log`

Reports include checks run, checks skipped, confidence, residual risks, diff hash, session id, and raw log paths.

`vrt verify --continue` links new evidence to `.vrt/latest.json`. It reuses prior passed checks only when the previous evidence is partial and the base commit, profile hash, and diff hash still match. If the diff changed after a patch, VRT does not reuse old checks and records stale reasons instead.

`vrt report --format sarif|junit --output <path>` exports the latest evidence without rerunning checks. SARIF contains failed checks as code-scanning results. JUnit contains every run or reused check as a testcase.

## Agent Rule

After `vrt skill install`, agents should call `vrt verify --json` before expensive direct build/test/lint/typecheck commands, use `vrt explain --json` after failures, and preserve residual risks in user-facing reports.

## v0.1 Direction

- Better Nx/Turbo affected integration.
- More precise root-cause extraction.
- MCP resources and prompts.
- Binary download/install scripts for published npm packages.

## License

MIT
