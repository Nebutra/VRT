# VRT v0 Technical Scope

VRT v0 implements the local Rust-native runtime path:

1. Project profiler
2. Capability graph
3. Git diff interpreter
4. Verification planner
5. Runner
6. Evidence ledger
7. Human report
8. Agent JSON
9. Skill installer
10. MCP stdio server
11. TypeScript SDK
12. npm CLI wrapper
13. `verify --continue` evidence stitching
14. SARIF and JUnit report export

The implementation is intentionally conservative. It shells out to project-owned commands and records what happened. It does not introduce a new typechecker, test runner, build system, or CI replacement.

## Confidence Rules

Skipped checks are represented as residual risk. A successful local run can raise local confidence, but release confidence remains insufficient unless release-mode verification and external CI/deployment checks exist.

Continuation is conservative. VRT links continued evidence to the previous evidence id, but it only reuses passed checks when previous evidence is partial and base commit, profile hash, and diff hash are unchanged. Changed diffs produce stale reasons and force the current plan to run again.

## Report Exports

`vrt report --format sarif --output <path>` exports failed checks from the latest evidence as SARIF 2.1.0 results. `vrt report --format junit --output <path>` exports run and reused checks as JUnit testcases. Reports are derived from evidence only; exporting does not rerun verification.

## MCP Boundary

`vrt mcp serve` implements a small JSON-RPC stdio server for the core Agent workflow:

- `initialize`
- `tools/list`
- `tools/call`

Exposed tools:

- `analyze_change`
- `plan_verification`
- `run_verification`
- `explain_failure`
- `get_evidence`
- `escalate_verification`

The server does not expose arbitrary shell execution. Unknown tools return a JSON-RPC protocol error. Tool execution failures return tool results with `isError: true` so agents can inspect and self-correct.

## TypeScript Surface

v0 includes two package surfaces:

- `packages/sdk`: ESM SDK exposing `verify`, `doctor`, `explain`, `bench`, `runJson`, and `resolveVrtBinary`.
- `packages/vrt`: npm `vrt` bin wrapper that forwards to `VRT_BIN` or a locally built Rust binary.

The SDK is intentionally thin. It does not reimplement planning logic in TypeScript; it calls the native CLI and parses JSON. Command failures preserve exit code, stdout, stderr, and the exact command for agent diagnostics.
