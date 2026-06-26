//! Emit the `.vrt-proof/` package (Canvas §11): machine-readable metrics, a
//! human-readable summary carrying the §18 verdict, per-scenario baseline/vrt
//! records, and an honest failures/ directory that is populated, not hidden.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::json;

use crate::model::{ScenarioVerdict, ProofMetrics};
use crate::proof::ProofRun;

pub fn emit(out_dir: &Path, run: &ProofRun) -> Result<()> {
    for sub in ["baseline", "vrt", "reports", "failures", "agent-transcripts"] {
        fs::create_dir_all(out_dir.join(sub))?;
    }

    fs::write(
        out_dir.join("metrics.json"),
        serde_json::to_string_pretty(&run.metrics)?,
    )
    .context("write metrics.json")?;

    for outcome in &run.outcomes {
        fs::write(
            out_dir.join("reports").join(format!("{}.json", outcome.id)),
            serde_json::to_string_pretty(outcome)?,
        )?;
        fs::write(
            out_dir.join("baseline").join(format!("{}.json", outcome.id)),
            serde_json::to_string_pretty(&outcome.baseline_commands)?,
        )?;
        fs::write(
            out_dir.join("vrt").join(format!("{}.json", outcome.id)),
            serde_json::to_string_pretty(&json!({
                "confidence": outcome.confidence,
                "commands_run": outcome.commands_run,
                "commands_avoided": outcome.commands_avoided,
                "full_builds_avoided": outcome.full_builds_avoided,
                "measured_saved_time_ms": outcome.measured_saved_time_ms,
                "estimated_saved_time_ms": outcome.estimated_saved_time_ms,
                "residual_risks": outcome.residual_risks,
                "hard_failures": outcome.hard_failures,
                "verdict": format!("{:?}", outcome.verdict),
            }))?,
        )?;
        if outcome.verdict == ScenarioVerdict::Fail {
            fs::write(
                out_dir.join("failures").join(format!("{}.json", outcome.id)),
                serde_json::to_string_pretty(&json!({
                    "scenario": outcome.id,
                    "title": outcome.title,
                    "hard_failures": outcome.hard_failures,
                    "failed_assertions": outcome
                        .assertions
                        .iter()
                        .filter(|a| !a.passed)
                        .collect::<Vec<_>>(),
                }))?,
            )?;
        }
    }

    for (naive, vrt) in &run.transcripts {
        fs::write(
            out_dir
                .join("agent-transcripts")
                .join(format!("{}-naive.json", naive.scenario)),
            serde_json::to_string_pretty(naive)?,
        )?;
        fs::write(
            out_dir
                .join("agent-transcripts")
                .join(format!("{}-vrt.json", vrt.scenario)),
            serde_json::to_string_pretty(vrt)?,
        )?;
    }

    for (i, fc) in run.false_confidence.iter().enumerate() {
        fs::write(
            out_dir
                .join("failures")
                .join(format!("false-confidence-{i}.json")),
            serde_json::to_string_pretty(fc)?,
        )?;
    }

    fs::write(out_dir.join("summary.md"), summary_md(run)).context("write summary.md")?;
    Ok(())
}

fn pct(saved: u128, baseline: u128) -> String {
    if baseline == 0 {
        return "n/a".into();
    }
    format!("{:.0}%", (saved as f64 / baseline as f64) * 100.0)
}

fn summary_md(run: &ProofRun) -> String {
    let m = &run.metrics;
    let mut s = String::new();
    s.push_str("# VRT Adversarial Proof — Summary\n\n");
    s.push_str(&format!("- Commit: `{}`\n", m.commit));
    s.push_str(&format!("- Generated: {}\n", m.generated_at));
    s.push_str(&format!("- Schema: `{}`\n\n", m.schema_version));

    s.push_str("## Verdict (Canvas §18)\n\n");
    s.push_str(&format!("**Verdict: {}**\n\n", run.overall.label()));
    for (id, name, v) in &run.propositions {
        s.push_str(&format!("- {id}. {name}: **{}**\n", v.label()));
    }
    s.push_str(&format!("\n- Hard failures: {}\n", m.hard_failure_count));
    s.push_str(&format!(
        "- False confidence cases: {}\n",
        m.false_confidence_cases
    ));
    s.push_str(&format!(
        "- Measured saved time: {} ms\n",
        m.measured_saved_time_ms
    ));
    s.push_str(&format!(
        "- Estimated saved time (VRT self-report, separate): {} ms\n\n",
        m.estimated_saved_time_ms
    ));

    s.push_str("## Governance counters (must be 0 — Canvas §8.3)\n\n");
    s.push_str(&format!(
        "- skipped_as_passed: {}\n- release_overclaim: {}\n- stale_evidence_reuse: {}\n- high_risk_underverified: {}\n\n",
        m.skipped_as_passed_count,
        m.release_overclaim_count,
        m.stale_evidence_reuse_count,
        m.high_risk_underverified_count
    ));

    let a = &m.agent;
    s.push_str("## Agent behaviour A/B (Canvas §6.3, Proposition B)\n\n");
    s.push_str("Deterministic naive vs VRT-guided policies over real per-scenario outputs.\n\n");
    s.push_str(&format!(
        "- expensive_commands_avoided: **{:.0}%** (naive {} → vrt {}; bar ≥30%)\n",
        a.expensive_commands_avoided_pct, a.naive_expensive_total, a.vrt_expensive_total
    ));
    s.push_str(&format!(
        "- explain_after_failure_rate: **{:.0}%** over {} failure scenarios (bar ≥80%)\n",
        a.explain_after_failure_rate * 100.0,
        a.failure_scenarios
    ));
    s.push_str(&format!(
        "- ignored_do_not_run_count: **{}** (bar =0)\n",
        a.ignored_do_not_run_count
    ));
    s.push_str(&format!(
        "- residual_risk_preserved_rate: **{:.0}%** ({}/{}; bar ≥95%)\n",
        a.residual_risk_preserved_rate * 100.0,
        a.residual_risks_preserved_total,
        a.residual_risks_received_total
    ));
    s.push_str(&format!(
        "- log_lines_read_by_agent: naive {} → vrt {}\n\n",
        a.log_lines_read_naive, a.log_lines_read_vrt
    ));

    s.push_str("## Scenarios\n\n");
    for o in &run.outcomes {
        s.push_str(&format!("### {} — {:?}\n\n", o.title, o.verdict));
        s.push_str(&format!("- Fixture: `{}`\n", o.fixture));
        s.push_str(&format!(
            "- Confidence: local={} merge={} release={}\n",
            o.confidence.local, o.confidence.merge, o.confidence.release
        ));
        if o.baseline_total_ms > 0 {
            s.push_str(&format!(
                "- Measured: baseline {}ms vs vrt {}ms → saved {}ms ({} of baseline)\n",
                o.baseline_total_ms,
                o.vrt_total_ms,
                o.measured_saved_time_ms,
                pct(o.measured_saved_time_ms, o.baseline_total_ms)
            ));
        } else {
            s.push_str(&format!("- Measured: vrt {}ms; baseline not fully measured (see notes)\n", o.vrt_total_ms));
        }
        s.push_str(&format!(
            "- commands_run={} commands_avoided={} full_builds_avoided={} ci_shifted_left={}\n",
            o.commands_run, o.commands_avoided, o.full_builds_avoided, o.ci_failures_shifted_left
        ));
        for a in &o.assertions {
            let mark = if a.passed { "✓" } else { "✗" };
            s.push_str(&format!("  - {mark} {} — {}\n", a.name, a.detail));
        }
        for h in &o.hard_failures {
            s.push_str(&format!("  - ⛔ HARD FAILURE [{}] {}\n", h.code, h.detail));
        }
        for n in &o.notes {
            s.push_str(&format!("  - ℹ {n}\n"));
        }
        s.push('\n');
    }

    s.push_str("## Known limitations\n\n");
    s.push_str("- Proposition B is measured from DETERMINISTIC agent policies over real VRT outputs, not a live LLM. The metrics are falsifiable (they degrade if VRT omits do_not_run / root causes / residual risks), but a live-LLM A/B eval remains future work and is not claimed as done (Canvas §20).\n");
    s.push_str("- Baseline commands for fixtures without an installed toolchain are reported `not_available` and excluded from measured savings (Canvas §2.1).\n");
    s.push_str("- This package proves local feedback and governance properties; it does not claim to replace CI (Canvas §20.2).\n");
    s
}

/// Console one-liner for the CLI.
pub fn console_verdict(run: &ProofRun, m: &ProofMetrics) -> String {
    format!(
        "Verdict: {} | scenarios {}/{} pass, {} fail, {} n/a | hard failures {} | false confidence {} | measured saved {}ms",
        run.overall.label(),
        m.scenarios_passed,
        m.scenarios_total,
        m.scenarios_failed,
        m.scenarios_not_applicable,
        m.hard_failure_count,
        m.false_confidence_cases,
        m.measured_saved_time_ms,
    )
}
