# VRT Growth Positioning

## Sharp Wedge

Coding agents are fast at editing but bad at choosing verification order. VRT owns the local moment after an agent changes code and before CI runs.

## Landing Hook

Stop making coding agents wait for the wrong checks.

## Core Conversion Path

1. Developer sees the pain statement.
2. Developer runs `vrt doctor` against a familiar JS/TS project.
3. Developer runs `vrt verify --json`.
4. Agent patches failures and can call `vrt verify --continue` to link evidence without reusing stale checks.
5. Agent consumes the JSON and reports residual risk.
6. Maintainer asks future contributors to attach VRT evidence before CI.

## Audience

- AI-native solo builders working in Next.js, TypeScript, pnpm, Turborepo, and similar stacks.
- Open source maintainers who want contributors to prove local readiness before CI.
- Small teams running multiple agents or sessions against the same repo.

## Product-Led Growth Surfaces

- Evidence reports: portable proof that can be pasted into PRs or agent final messages.
- Agent skill installer: turns project verification behavior into repository policy.
- JSON CLI output and MCP stdio tools: lets agents integrate without waiting for an editor extension.
- TypeScript SDK and npm wrapper: lowers integration friction for Node-based agents and JS/TS projects.
- SARIF/JUnit exports: lets local evidence flow into code scanning, CI summaries, and PR artifacts.
- Bench summary: quantifies avoided expensive checks while preserving residual risk.

## Trust Boundary

Do not market VRT as a tool that makes builds or tests faster. VRT reduces total verification loop cost by planning order, avoiding unnecessary checks, stopping early on root causes, and preserving evidence.

The durable message is: skipped is not passed.
