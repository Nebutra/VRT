# Contributing

VRT is a local verification runtime for AI coding agents. Contributions should preserve the product line: faster feedback is allowed only when the evidence remains auditable.

## Local Setup

Prerequisites:

- Rust stable toolchain
- Node.js 22 or newer
- npm
- Git

Run the full local gate:

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
npm test
npm run docs:check
```

## Development Loop

For changes inside a JS/TS example or an integration surface, use VRT itself before expensive direct checks:

```bash
cargo run -q -p vrt-cli -- verify --json
cargo run -q -p vrt-cli -- explain --json
```

In an installed project, use:

```bash
vrt verify --json
vrt explain --json
```

If RTK or Headroom is active, keep evidence references visible:

```bash
vrt verify --token-profile rtk
vrt verify --token-profile headroom
```

## Evidence Rules

- Skipped checks are not passed.
- Partial evidence is not release readiness.
- Raw logs under `.vrt/evidence/**` must stay retrievable.
- New confidence claims need tests that prove the exact behavior.
- Failed, stale, or mismatched evidence must not be used as a cache source.
- Agent-facing output must preserve residual risk, evidence id, diff hash, profile hash, and raw log references.

## Tests

Keep tests close to the behavior being changed:

- Rust core behavior: `crates/vrt-core/tests/`
- MCP and broker protocol behavior: `crates/vrt-core/tests/vrt_mcp.rs`
- npm wrapper behavior: `packages/vrt/test/`
- SDK behavior: `packages/sdk/test/`
- GitHub Action behavior: `packages/github-action/test/`
- release/docs guardrails: `scripts/*.test.mjs`

When a change touches shared behavior, run the full local gate before opening a pull request.

## Documentation

Mintlify docs live in `docs/*.mdx` and are wired through `docs.json`. If a new command, report field, package surface, or Agent workflow is added, update the relevant Mintlify page and run:

```bash
npm run docs:check
```

## Pull Requests

Before requesting review, include:

- What changed
- Which checks ran
- Which checks were skipped
- Residual risk, if any
- Evidence path or command output for the verification

Do not describe skipped checks as passed checks in a PR summary.
