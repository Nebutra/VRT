import { appendFile, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

export function createActionReport(evidence) {
  const checks = Array.isArray(evidence.checks) ? evidence.checks : [];
  const reusedChecks = Array.isArray(evidence.reused_checks) ? evidence.reused_checks : [];
  const skipped = Array.isArray(evidence.skipped) ? evidence.skipped : [];
  const failedChecks = checks.filter((check) => check.status === "failed");
  const status = evidence.validity ?? "unknown";
  const outputs = {
    vrt_status: status,
    vrt_evidence_id: evidence.evidence_id ?? "",
    vrt_failed_checks: String(failedChecks.length),
    vrt_skipped_checks: String(skipped.length),
  };

  return {
    status,
    outputs,
    summary: renderSummary({
      evidence,
      checks,
      reusedChecks,
      skipped,
      failedChecks,
    }),
  };
}

export async function runAction(options = {}) {
  const env = options.env ?? process.env;
  const evidencePath =
    options.evidencePath ?? env.INPUT_EVIDENCE_PATH ?? env["INPUT_EVIDENCE-PATH"] ?? ".vrt/latest.json";
  const failOnPartial =
    options.failOnPartial ?? parseBoolean(env.INPUT_FAIL_ON_PARTIAL ?? env["INPUT_FAIL-ON-PARTIAL"], true);
  const outputFile = options.outputFile ?? env.GITHUB_OUTPUT;
  const summaryFile = options.summaryFile ?? env.GITHUB_STEP_SUMMARY;
  const reportOutputPath =
    options.reportOutputPath ??
    env.INPUT_REPORT_OUTPUT_PATH ??
    env["INPUT_REPORT-OUTPUT-PATH"] ??
    "";

  const evidence = JSON.parse(await readFile(evidencePath, "utf8"));
  const report = createActionReport(evidence);
  if (reportOutputPath) {
    await writeMarkdownReport(reportOutputPath, report.summary);
    report.outputs.vrt_report_path = reportOutputPath;
  }
  if (outputFile) {
    await appendFile(outputFile, renderOutputs(report.outputs));
  }
  if (summaryFile) {
    await appendFile(summaryFile, `${report.summary}\n`);
  }

  return {
    ...report,
    exitCode: failOnPartial && report.status !== "valid" ? 1 : 0,
  };
}

async function writeMarkdownReport(reportOutputPath, summary) {
  const parent = path.dirname(reportOutputPath);
  if (parent && parent !== ".") {
    await mkdir(parent, { recursive: true });
  }
  await writeFile(reportOutputPath, `${summary}\n`);
}

function renderSummary({ evidence, checks, reusedChecks, skipped, failedChecks }) {
  const lines = [
    "## VRT verification report",
    "",
    `VRT evidence: \`${evidence.evidence_id ?? "unknown"}\``,
    "",
    "| Field | Value |",
    "| --- | --- |",
    `| Status | \`${evidence.validity ?? "unknown"}\` |`,
    `| Local confidence | \`${evidence.confidence?.local ?? "unknown"}\` |`,
    `| Merge confidence | \`${evidence.confidence?.merge ?? "unknown"}\` |`,
    `| Release confidence | \`${evidence.confidence?.release ?? "unknown"}\` |`,
    `| Checks run | ${checks.length} |`,
    `| Checks reused | ${reusedChecks.length} |`,
    `| Checks skipped | ${skipped.length} |`,
    "",
  ];

  if (failedChecks.length > 0) {
    lines.push("### Failed checks", "");
    for (const check of failedChecks) {
      lines.push(`- \`${check.name}\`: ${check.summary ?? "No summary"}`);
      if (check.raw_log) {
        lines.push(`  - Raw log: \`${check.raw_log}\``);
      }
    }
    lines.push("");
  }

  if (skipped.length > 0) {
    lines.push("### Skipped checks are residual risk", "");
    for (const item of skipped) {
      lines.push(`- \`${item.capability_id}\`: ${item.residual_risk ?? "Residual risk not specified."}`);
    }
    lines.push("");
  }

  if (Array.isArray(evidence.stale_reasons) && evidence.stale_reasons.length > 0) {
    lines.push("### Stale evidence notes", "");
    for (const reason of evidence.stale_reasons) {
      lines.push(`- ${reason}`);
    }
    lines.push("");
  }

  lines.push(`Raw logs: \`${evidence.raw_log_dir ?? ".vrt/evidence"}\``);
  return lines.join("\n");
}

function renderOutputs(outputs) {
  return Object.entries(outputs)
    .map(([key, value]) => `${key}=${String(value).replace(/\n/g, " ")}`)
    .join("\n")
    .concat("\n");
}

function parseBoolean(value, fallback) {
  if (value == null || value === "") {
    return fallback;
  }
  return ["1", "true", "yes", "on"].includes(String(value).toLowerCase());
}

const isDirectRun = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isDirectRun) {
  const result = await runAction();
  process.exitCode = result.exitCode;
}
