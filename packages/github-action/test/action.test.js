import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { createActionReport, runAction } from "../src/index.js";

function evidence(overrides = {}) {
  return {
    evidence_id: "ev_test",
    validity: "partial",
    duration_ms: 1234,
    report_path: ".vrt/evidence/ev_test/evidence.json",
    raw_log_dir: ".vrt/evidence/ev_test",
    checks: [
      {
        name: "workspace-typecheck",
        status: "failed",
        summary: "apps/web/page.tsx:4:7 TS2322 Type mismatch",
        raw_log: ".vrt/evidence/ev_test/step_1.raw.log",
      },
    ],
    reused_checks: [
      {
        name: "workspace-lint",
        status: "reused",
        summary: "Reused from evidence ev_previous",
        raw_log: ".vrt/evidence/ev_previous/step_1.raw.log",
      },
    ],
    skipped: [
      {
        capability_id: "workspace-build",
        residual_risk: "Production bundler behavior not verified.",
      },
    ],
    confidence: {
      local: "low",
      merge: "none",
      release: "insufficient",
      residual_risks: ["Production bundler behavior not verified."],
    },
    stale_reasons: ["diff hash changed; previous checks were not reused"],
    ...overrides,
  };
}

test("createActionReport summarizes evidence for PR artifacts", () => {
  const report = createActionReport(evidence());

  assert.equal(report.status, "partial");
  assert.equal(report.outputs.vrt_status, "partial");
  assert.equal(report.outputs.vrt_evidence_id, "ev_test");
  assert.equal(report.outputs.vrt_failed_checks, "1");
  assert.equal(report.outputs.vrt_skipped_checks, "1");
  assert.match(report.summary, /VRT evidence: `ev_test`/);
  assert.match(report.summary, /workspace-typecheck/);
  assert.match(report.summary, /Production bundler behavior not verified/);
  assert.match(report.summary, /diff hash changed/);
});

test("runAction reads evidence, writes GitHub outputs, and fails on partial by default", async () => {
  const dir = await mkdtemp(path.join(tmpdir(), "vrt-action-"));
  const outputFile = path.join(dir, "github-output");
  const summaryFile = path.join(dir, "github-summary");
  const reportFile = path.join(dir, "vrt-report.md");
  const evidenceFile = path.join(dir, "latest.json");
  await writeFile(evidenceFile, JSON.stringify(evidence(), null, 2));

  const result = await runAction({
    evidencePath: evidenceFile,
    outputFile,
    summaryFile,
    reportOutputPath: reportFile,
    failOnPartial: true,
  });

  assert.equal(result.exitCode, 1);
  const outputs = await readFile(outputFile, "utf8");
  assert.match(outputs, /vrt_status=partial/);
  assert.match(outputs, /vrt_failed_checks=1/);
  assert.match(outputs, new RegExp(`vrt_report_path=${reportFile.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}`));
  const summary = await readFile(summaryFile, "utf8");
  assert.match(summary, /## VRT verification report/);
  assert.match(summary, /Raw logs/);
  const report = await readFile(reportFile, "utf8");
  assert.match(report, /## VRT verification report/);
  assert.match(report, /workspace-typecheck/);
  assert.match(report, /Production bundler behavior not verified/);
});

test("runAction allows partial evidence when failOnPartial is false", async () => {
  const dir = await mkdtemp(path.join(tmpdir(), "vrt-action-"));
  const evidenceFile = path.join(dir, "latest.json");
  await writeFile(evidenceFile, JSON.stringify(evidence(), null, 2));

  const result = await runAction({
    evidencePath: evidenceFile,
    failOnPartial: false,
  });

  assert.equal(result.exitCode, 0);
});
