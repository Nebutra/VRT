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

Full documentation is maintained with Mintlify. Run:

```bash
npm run docs:dev
```

## Why Agents Need This

Coding agents are fast at edits and slow at verification judgment. A small component change often triggers a full build, long test run, noisy failure, then another full rerun after the agent patches the wrong thing.

VRT inserts a verification planning layer between Git and CI:

```text
Git diff -> Project profile -> Capability graph -> Verification plan -> Runner -> Evidence ledger
```

That gives agents a local feedback loop with lower latency and a better audit trail.

## What You Get In v0

- JS/TS project profiling from `package.json`, lockfiles, `tsconfig`, Next/Vite configs, GitHub Actions workflows, and known tool configs.
- Capability graph from existing scripts such as `typecheck`, `lint`, Biome/format check, env validation, Prisma generate, migration safety, related/full `test`, Playwright smoke/e2e, and `build`.
- Turborepo and Nx adapters that delegate affected task selection to `turbo --affected` and `nx affected`.
- Git diff risk tags for UI, API, shared package, database, auth, billing, env, infra, and CI changes.
- Dev/merge/release verification modes.
- Raw logs under `.vrt/evidence/<evidence-id>/`.
- Human report and agent JSON.
- Failure explanation from the latest evidence.
- `verify --continue` evidence stitching after failures.
- Worktree run lock at `.vrt/run.lock` to avoid competing local verification sessions.
- Same-plan singleflight deduplication so duplicate verification requests reuse matching evidence.
- Exact-match evidence cache for repeated verification requests with the same plan, diff, profile, lockfile, runtime assumptions, and relevant inputs.
- Agent skill installer.
- Bench summary for avoided expensive checks.
- MCP stdio server with structured resources, prompts, tools, and no arbitrary shell tool.
- Local JSONL broker for token-saving agents that need bounded VRT operations without the full MCP envelope.
- Repo-local broker state with queue, lock, runner-pool, and session control-plane commands.
- TypeScript SDK and npm bin wrapper packages.
- GitHub Action package that turns evidence into CI outputs and step summaries.
- SARIF, JUnit, and OpenTelemetry JSON export from latest evidence.
- RTK and Headroom token-saving compatibility profiles.
- Optional git worktree sessions for parallel Agent work.

## 30-Second Tryout

From this repo:

```bash
cargo test --workspace
npm test
npm run docs:check
cargo run -q -p vrt-cli -- --root examples/next-single-app doctor
cargo run -q -p vrt-cli -- --root examples/vite-ts-app doctor
cargo run -q -p vrt-cli -- --root examples/prisma-next-app doctor
cargo run -q -p vrt-cli -- --root examples/next-turbo-pnpm doctor
cargo run -q -p vrt-cli -- --root examples/nx-workspace doctor
```

In a JS/TS project:

```bash
vrt init
vrt doctor
vrt verify --dry-run
vrt verify --dry-run --json
vrt verify
vrt verify --token-profile rtk
vrt verify --token-profile headroom
vrt explain
vrt skill install
vrt token doctor
vrt token manifest --json
vrt token install-rules
vrt session start --worktree ../my-repo-agent-a
vrt session status --json
vrt session list --json
vrt session show <session-id> --json
vrt session close <session-id> --json
vrt session view --json
vrt broker start --json
vrt broker stop --json
vrt queue status --json
vrt queue cancel <job-id> --json
vrt lock list --json
vrt report --format markdown --output .vrt/reports/vrt.md
vrt report --format sarif --output .vrt/reports/vrt.sarif
vrt report --format junit --output .vrt/reports/vrt.junit.xml
vrt report --format otel --output .vrt/reports/vrt.otel.json
vrt false-confidence record --stricter-check "vrt verify --mode release --full" --failure-summary "release check failed on an issue local confidence should have covered"
vrt false-confidence list
printf '{"jsonrpc":"2.0","id":1,"method":"tools/list"}\n' | vrt mcp serve
printf '{"jsonrpc":"2.0","id":2,"method":"resources/list"}\n' | vrt mcp serve
printf '{"jsonrpc":"2.0","id":3,"method":"prompts/list"}\n' | vrt mcp serve
printf '{"id":"1","op":"status"}\n{"id":"2","op":"shutdown"}\n' | vrt broker serve
```

For agents:

```bash
vrt verify --dry-run --json
vrt verify --json
vrt explain --json
vrt mcp serve
```

`vrt verify --dry-run --json` emits the plan without running project commands. `vrt verify --json` emits an agent report with `status`, `failure_kind`, `root_cause_candidates`, `recommended_next_action`, `do_not_run`, confidence, residual risks, raw log references, and the full nested `evidence` record. On failure it exits non-zero only after flushing that JSON to stdout, so agents and SDK wrappers can still parse it.

From Node:

```js
import { plan, verify, explain, tokenDoctor, tokenManifest, tokenInstallRules } from "@vrt/sdk";

const dryRunPlan = await plan({ mode: "merge" });
const evidence = await verify({ mode: "dev" });
const headroomEvidence = await verify({ tokenProfile: "headroom" });
const rtkText = await verify({ tokenProfile: "rtk" });
const explanation = await explain();
const tokenStatus = await tokenDoctor();
const tokenManifestJson = await tokenManifest();
await tokenInstallRules();
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
vrt verify --dry-run
vrt verify --dry-run --json
vrt verify --json
vrt verify --mode merge
vrt verify --mode release
vrt verify --full
vrt verify --continue
vrt verify --broker
vrt verify --no-broker
vrt verify --token-profile rtk
vrt verify --token-profile headroom
vrt explain
vrt explain --json
vrt skill install
vrt bench
vrt bench --concurrency --json
vrt session start --worktree ../my-repo-agent-a
vrt session status --json
vrt session list --json
vrt session view --json
vrt report --format markdown --output .vrt/reports/vrt.md
vrt report --format sarif --output .vrt/reports/vrt.sarif
vrt report --format junit --output .vrt/reports/vrt.junit.xml
vrt report --format otel --output .vrt/reports/vrt.otel.json
vrt false-confidence record --stricter-check "vrt verify --mode release --full" --failure-summary "release check failed on an issue local confidence should have covered"
vrt false-confidence list
vrt mcp serve
vrt broker status --json
vrt broker serve
```

`vrt mcp serve` exposes structured JSON-RPC resources, prompts, and tools over stdio.

Resources:

- `vrt://profile`
- `vrt://latest-evidence`
- `vrt://skill`
- `vrt://token-rules`
- `vrt://token-compatibility`

Prompts:

- `verify_after_change`
- `explain_failure`
- `write_verification_report`

Tools:

- `analyze_change`
- `plan_verification`
- `run_verification`
- `explain_failure`
- `get_evidence`
- `escalate_verification`

It deliberately does not expose `run_any_shell_command`. Verification execution still goes through VRT plans and project-owned capabilities.

`vrt broker serve` exposes the same bounded verification operations over line-oriented JSON for local token-saving agents. Requests can use `op`/`arguments` or `method`/`params`; responses return `{ "id": ..., "ok": true, "result": ... }` or `{ "ok": false, "error": ... }`. Supported operations are `status`, `analyze_change`, `plan_verification`, `run_verification`, `explain_failure`, `get_evidence`, `escalate_verification`, `session_view`, and `shutdown`.

`explain_failure` and `get_evidence` accept an optional `evidence_id`, so token-saving agents can retrieve or explain an older compacted run without relying on `.vrt/latest.json`.

## Packages

```text
crates/vrt-core   Rust runtime library
crates/vrt-cli    Native CLI binary
packages/sdk      ESM SDK for agents and Node tooling
packages/vrt      npm bin wrapper for the native CLI
packages/github-action  GitHub Action for CI evidence summaries
```

The SDK calls the native CLI with JSON output and preserves command failures with exit code, stdout, and stderr. The npm wrapper resolves binaries in this order: `VRT_BIN`, a packaged binary at `bin/<platform>-<arch>/vrt`, locally built Rust binaries under `target/`, then `vrt` on PATH.

`packages/vrt` has a `prepack` flow for native publishing:

```bash
npm --workspace vrt run build:binary
npm --workspace vrt run stage:binary
```

The staging step copies the release binary into `packages/vrt/bin/<platform>-<arch>/vrt` or `vrt.exe` on Windows, which is the same layout the wrapper resolves at runtime.

`.github/workflows/release.yml` runs the quality gate on pull requests and `main`. For tags or manual dispatch it builds Linux, macOS, and Windows native binaries, aggregates them into one `packages/vrt/bin/*` layout, and packs a single `vrt-*.tgz` npm artifact.

The publish job is gated behind the `npm` environment and uses GitHub OIDC (`id-token: write`) with `npm publish --provenance --access public`. Manual dispatch only publishes when `publish_to_npm` is set to `true`; otherwise it only produces artifacts.

For token-saving integrations, `@vrt/sdk` accepts `verify({ tokenProfile: "headroom" })` for structured JSON and `verify({ tokenProfile: "rtk" })` for compact text. It also exposes `tokenDoctor()` and `tokenInstallRules()`.

## RTK And Headroom

VRT is designed to compose with token-saving tools without losing evidence.

- RTK: use `vrt verify --token-profile rtk`, or let RTK proxy it with `rtk vrt verify --token-profile rtk`.
- RTK setup: for Codex command rewriting, install RTK hooks with `rtk init -g --codex`.
- Headroom: use `vrt verify --token-profile headroom` so the visible result is structured JSON with retrievable evidence references.
- Headroom setup: wrap Codex with `headroom wrap codex`, run proxy mode with `headroom proxy --port 8787`, or install Headroom MCP with `headroom mcp install`.
- Manifest: use `vrt token manifest --json` or MCP resource `vrt://token-compatibility` to expose the compatibility contract to token-saving agents.
- Rules: run `vrt token install-rules` to write `.vrt/token-saving/RTK_HEADROOM.md`, Cursor/Windsurf/Codex token-saving rule files, and links in `AGENTS.md`, `CLAUDE.md`, and `GEMINI.md`.
- Doctor: run `vrt token doctor --json` to check whether `rtk` and `headroom` are visible on PATH and see recommended commands.

The compact profiles intentionally preserve `evidence=`, `report=`, `raw=`, `raw_log`, and `.vrt/evidence` references. RTK can compact the visible command output, and Headroom can compress structured summaries, but the full VRT evidence ledger remains available locally. If Headroom compression hides required detail, retrieve it through `headroom_retrieve` or VRT raw log paths instead of rerunning broad checks.

## What VRT Is

VRT is a verification planning and evidence layer between Git and CI. It reuses your existing tools instead of replacing them: TypeScript, ESLint, Biome, Vitest, Jest, Playwright, Prisma, Turborepo, Nx, and your package manager scripts. Monorepo behavior is documented in the Mintlify [Monorepos](docs/monorepos.mdx) page.

Unsupported or non-JS roots degrade to an `unknown` profile with explicit weak spots rather than crashing or claiming confidence. Capability weak spots include missing typecheck and test scripts; release-facing weak spots include missing environment validation, missing CI, and missing Prisma migration safety checks when database tooling is detected.

The repository includes example fixtures for Next.js, Vite, Prisma, Turborepo, and Nx under `examples/`. They are covered by the Rust test suite so profiler behavior and documentation stay aligned.

Contributions are covered in [Contributing](CONTRIBUTING.md). The short version: run the full local gate, preserve raw evidence, and never describe skipped checks as passed.

## What VRT Is Not

VRT is not a CI replacement, build-system replacement, test runner, typechecker, or log-compression trick. Acceleration comes from planning and sequencing verification, not from pretending skipped checks passed.

## Evidence Files

Every `vrt verify` writes:

- `.vrt/profile.json`
- `.vrt/config.toml`
- `.vrt/run.lock/lock.json` while verification is active
- `.vrt/latest.json`
- `.vrt/evidence/<evidence-id>/evidence.json`
- `.vrt/evidence/<evidence-id>/*.raw.log`
- `.vrt/cache/evidence/<cache-key>.json` for exact-match valid evidence reuse

Reports include checks run, check safety levels, checks skipped, confidence, residual risks, diff hash, session id, lockfile hash, config hash, toolchain version, relevant inputs hash, environment assumptions, dirty worktree state, changed file paths, raw log paths, broker job id, queue wait, lock wait, singleflight metadata, resource lock observations, and runner-pool class.

Profile JSON, evidence JSON, false-confidence JSONL rows, and `.vrt/config.toml` carry `schema_version = 1` only as a parser compatibility boundary. It is not a VRT release number, feature milestone, or changelog mechanism.

`.vrt/config.toml` is a policy overlay, not a command rule pile. Today VRT uses `[policy].default_mode` when `vrt verify`, MCP, or the JSONL broker are called without an explicit mode, uses `[policy.strict].areas` to escalate matching risk tags into broader local plans, uses `[policy.relaxed].areas` to keep low-risk docs/marketing/style changes in faster loops, and respects `[release]` policy for build proof and external CI disclosure. Existing config is preserved by future `vrt init` and `vrt verify` runs, so user policy is not overwritten.

If a stricter follow-up check fails for a reason an earlier confidence level should have covered, record it with `vrt false-confidence record`. Cases are appended to `.vrt/false-confidence.jsonl` with the original evidence id, previous confidence, diff hash, profile hash, stricter check, and failure summary. `vrt bench --json` includes `false_confidence_cases` and `false_confidence_rate`.

`vrt verify --continue` links new evidence to `.vrt/latest.json`. It reuses prior passed checks only when the previous evidence is partial and the base commit, profile hash, diff hash, config hash, toolchain version, environment assumptions, and relevant inputs hash still match. If any scope input changed after a patch, VRT does not reuse old checks and records stale reasons instead.

If another verification is already running in the same worktree, VRT checks the active lock. The same `plan_id` can singleflight-join and reuse matching `.vrt/latest.json` evidence when the plan id, base commit, diff hash, and profile hash match. The follower writes its own evidence record with `singleflight.role = "follower"`, `shared_from_evidence_id`, and scoped `reused_checks`, so each session gets an auditable reference without rerunning the same command. Different plans are refused while the lock is active. If a process crashed, remove `.vrt/run.lock` only after confirming no VRT run is active.

For repeated local requests after a valid run, VRT can reuse an exact-match cached evidence entry instead of rerunning commands. A cache hit requires the same `plan_id`, base commit, diff hash, profile hash, lockfile hash, config hash, toolchain version, relevant inputs hash, runtime env assumptions, selected steps, and skipped checks. The new evidence record gets its own evidence id and session id, sets `continued_from` to the source evidence, and puts prior passed checks in `reused_checks`. Failed, partial, stale, or mismatched evidence is never used as a cache source.

`vrt bench --json` reports `cache_hits`, `cache_hit_rate`, `evidence_reuse_rate`, `reused_checks`, `reruns_avoided`, `early_failures`, `ci_failures_shifted_left`, `stale_evidence_detected`, `log_lines_compressed`, `agent_tokens_saved_estimate`, `queue_wait_time_ms`, `lock_wait_time_ms`, `singleflight_hits`, `duplicate_commands_avoided`, `resource_conflicts_avoided`, `runner_pool_utilization`, `session_count`, `shared_evidence_count`, `estimated_saved_time_ms`, and a `saved_by` breakdown for skipped expensive checks and exact evidence reuse. These are conservative local estimates; skipped checks remain residual risk.

`vrt verify --broker` submits the run through the repo-local broker control plane and writes `.vrt/broker/jobs/<job-id>.json`; the evidence includes `broker_job_id`. If broker state is running, `vrt verify` uses that path automatically unless `--no-broker` is passed. `vrt session start --worktree <path>` creates an optional git worktree session for parallel Agent work. VRT writes session metadata under `.vrt/session.json` and `.vrt/sessions/<session-id>.json` in both the original repository and the new worktree. Agents should `cd` into the worktree and export the shown `VRT_SESSION_ID` before running verification. `vrt session view --json` aggregates all recorded sessions with latest evidence, active lock state, confidence, and false-confidence counts.

`vrt queue status --json` summarizes the repo-local job ledger under `.vrt/broker/jobs/*.json`, including queued, running, and cancelled jobs. `vrt queue cancel <job-id> --json` only marks jobs that are still queued; running or completed verification is not retroactively cancelled.

`vrt report --format markdown|sarif|junit|otel --output <path>` exports the latest evidence without rerunning checks. Markdown is a PR-ready proof artifact with confidence, residual risks, and raw-log references. SARIF contains failed checks as code-scanning results. JUnit contains every run or reused check as a testcase. OpenTelemetry JSON contains a `vrt.verify` root span plus check and skipped-check child spans with evidence, confidence, raw-log, and residual-risk attributes.

`packages/github-action` reads `.vrt/latest.json` and writes GitHub Actions outputs plus a Markdown step summary. When `report-output-path` is set, it also writes a Markdown report file for `actions/upload-artifact`, PR comment bots, or release handoff. It does not rerun verification; it turns existing evidence into PR/CI-facing proof.

## Agent Rule

After `vrt skill install`, agents should call `vrt verify --json` before expensive direct build/test/lint/typecheck commands, inspect its failure guidance before patching, use `vrt explain --json` when they need to reread the latest failed evidence, and preserve residual risks in user-facing reports.

`vrt skill install` writes rules to `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, `.cursor/rules/vrt.md`, `.windsurf/rules/vrt.md`, `.codex/skills/vrt/SKILL.md`, and `.vrt/skill/vrt.md`.

When RTK or Headroom is active, agents should prefer `vrt verify --token-profile rtk` or `vrt verify --token-profile headroom` for human-visible output, then retrieve raw logs from `.vrt/evidence/**` only when the compact summary is insufficient.

## v0.1 Direction

- More language-specific root-cause extractors.
- Release provenance hardening and package signing.

## License

MIT
