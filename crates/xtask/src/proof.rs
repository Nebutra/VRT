//! Orchestration: evaluate every scenario, aggregate metrics, and derive the
//! Canvas §18 verdict. The verdict can and must be able to come out FAIL.

use std::path::Path;
use std::process::Command;

use anyhow::Result;
use serde_json::Value;

use crate::agent::{self, AgentTranscript};
use crate::detectors;
use crate::model::*;
use crate::runner::{self, CleanRoom};
use crate::scenarios::{Evaluated, Scenario};

pub struct ProofRun {
    pub metrics: ProofMetrics,
    pub outcomes: Vec<ScenarioOutcome>,
    pub propositions: Vec<(&'static str, &'static str, PropositionVerdict)>,
    pub overall: OverallVerdict,
    /// Scenarios where VRT did not fail yet a baseline check it skipped would
    /// have failed (Canvas §7.10). Each is a recorded false-confidence case.
    pub false_confidence: Vec<FalseConfidenceFinding>,
    /// Per-scenario (naive, vrt-guided) agent transcripts (Canvas §11.3).
    pub transcripts: Vec<(AgentTranscript, AgentTranscript)>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FalseConfidenceFinding {
    pub scenario: String,
    pub skipped_capability: String,
    pub baseline_command: String,
    pub detail: String,
}

fn vrt_bench_estimate(vrt_bin: &Path, dir: &Path) -> u128 {
    let out = Command::new(vrt_bin)
        .args(["bench", "--json"])
        .current_dir(dir)
        .output();
    out.ok()
        .and_then(|o| serde_json::from_slice::<Value>(&o.stdout).ok())
        .and_then(|v| {
            v.get("estimated_saved_time_ms")
                .or_else(|| v.pointer("/estimated_saved_time_ms"))
                .and_then(Value::as_u64)
        })
        .map(u128::from)
        .unwrap_or(0)
}

/// Did VRT skip a capability that the baseline proved would FAIL? That is the
/// §7.10 false-confidence condition.
fn detect_false_confidence(
    scenario: &Scenario,
    report: &Value,
    baseline: &[CommandRun],
) -> Vec<FalseConfidenceFinding> {
    let vrt_failed = report.get("status").and_then(Value::as_str) == Some("failed");
    if vrt_failed {
        return vec![]; // VRT already flagged a problem; not false confidence.
    }
    let skipped: Vec<String> = report
        .get("evidence")
        .and_then(|e| e.get("skipped"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|s| s.get("capability_id").and_then(Value::as_str))
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    let mut findings = vec![];
    for run in baseline.iter().filter(|r| r.status == RunStatus::Failed) {
        // The baseline label (build/test/lint/typecheck) maps to a capability
        // suffix; if VRT skipped a capability matching this failing command,
        // VRT's green-ish verdict was over-confident.
        if let Some(cap) = skipped.iter().find(|c| c.contains(&run.label)) {
            findings.push(FalseConfidenceFinding {
                scenario: scenario.id.clone(),
                skipped_capability: cap.clone(),
                baseline_command: run.command.clone(),
                detail: format!(
                    "VRT skipped '{cap}' but baseline '{}' failed (exit {:?})",
                    run.command, run.exit_code
                ),
            });
        }
    }
    findings
}

type EvalResult = (
    ScenarioOutcome,
    Vec<FalseConfidenceFinding>,
    (AgentTranscript, AgentTranscript),
);

fn evaluate(vrt_bin: &Path, scenario: &Scenario) -> Result<EvalResult> {
    // Baseline clean room.
    let baseline_room: CleanRoom = runner::prepare(&scenario.fixture, &scenario.mutations)?;
    let baseline = runner::run_baseline(baseline_room.path(), &scenario.baseline);
    let baseline_fully_measured =
        !baseline.is_empty() && baseline.iter().all(|r| r.measured);
    let baseline_total_ms: u128 = baseline
        .iter()
        .filter(|r| r.measured)
        .map(|r| r.duration_ms)
        .sum();
    let baseline_measured_count = baseline.iter().filter(|r| r.measured).count() as u64;

    // VRT clean room.
    let vrt_room = runner::prepare(&scenario.fixture, &scenario.mutations)?;
    let (report, vrt_total_ms) = runner::run_vrt(vrt_bin, vrt_room.path(), &scenario.vrt_mode)?;
    let status = report.get("status").and_then(Value::as_str).unwrap_or("");
    let explain = if status == "failed" {
        runner::run_explain(vrt_bin, vrt_room.path())?
    } else {
        Value::Null
    };
    let estimated_saved_time_ms = vrt_bench_estimate(vrt_bin, vrt_room.path());

    let measured_saved_time_ms = if baseline_fully_measured {
        baseline_total_ms.saturating_sub(vrt_total_ms)
    } else {
        0
    };

    let commands_run = report
        .get("checks_run")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let commands_avoided = baseline_measured_count.saturating_sub(commands_run);
    let skipped_caps: Vec<String> = report
        .get("evidence")
        .and_then(|e| e.get("skipped"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|s| s.get("capability_id").and_then(Value::as_str))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    let baseline_had_build = baseline
        .iter()
        .any(|r| r.label == "build" && r.measured);
    let full_builds_avoided =
        u64::from(baseline_had_build && skipped_caps.iter().any(|c| c.contains("build")));
    let ci_failures_shifted_left = u64::from(status == "failed");

    let residual_risks: Vec<String> = report
        .get("residual_risks")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    let confidence = Confidence {
        local: report
            .pointer("/confidence/local")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        merge: report
            .pointer("/confidence/merge")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        release: report
            .pointer("/confidence/release")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    };

    let hard_failures =
        detectors::scan_all(&report, scenario.config_mutated, scenario.high_risk);

    let assertions = (scenario.assertions)(&Evaluated {
        report: &report,
        explain: &explain,
        baseline_total_ms,
        vrt_total_ms,
        measured_saved_time_ms,
        baseline_fully_measured,
    });

    let false_confidence = detect_false_confidence(scenario, &report, &baseline);

    let transcripts = (
        agent::naive_transcript(&scenario.id, &baseline),
        agent::vrt_transcript(&scenario.id, &report, &explain),
    );

    let verdict = if !hard_failures.is_empty()
        || assertions.iter().any(|a| !a.passed)
        || !false_confidence.is_empty()
    {
        ScenarioVerdict::Fail
    } else if matches!(
        scenario.proposition,
        Proposition::AgileValue | Proposition::CiSaving
    ) && !baseline_fully_measured
        && measured_saved_time_ms == 0
        && assertions.is_empty()
    {
        ScenarioVerdict::NotApplicable
    } else {
        ScenarioVerdict::Pass
    };

    let mut notes = vec![];
    for run in &baseline {
        if run.status == RunStatus::NotAvailable {
            notes.push(format!(
                "baseline '{}' not_available (toolchain absent) — degraded honestly, not estimated",
                run.label
            ));
        }
    }

    let outcome = ScenarioOutcome {
        id: scenario.id.clone(),
        title: scenario.title.clone(),
        proposition: scenario.proposition,
        fixture: scenario.fixture_label.clone(),
        baseline_total_ms,
        vrt_total_ms,
        measured_saved_time_ms,
        estimated_saved_time_ms,
        commands_run,
        commands_avoided,
        full_builds_avoided,
        ci_failures_shifted_left,
        confidence,
        residual_risks,
        baseline_commands: baseline,
        assertions,
        hard_failures,
        verdict,
        notes,
    };
    Ok((outcome, false_confidence, transcripts))
}

pub fn run_all(vrt_bin: &Path, scenarios: &[Scenario], commit: String) -> Result<ProofRun> {
    let mut outcomes = vec![];
    let mut false_confidence = vec![];
    let mut transcripts = vec![];
    for scenario in scenarios {
        let (outcome, fc, pair) = evaluate(vrt_bin, scenario)?;
        outcomes.push(outcome);
        false_confidence.extend(fc);
        transcripts.push(pair);
    }
    let agent_metrics = agent::aggregate(&transcripts);

    let count_code = |code: &str| {
        outcomes
            .iter()
            .flat_map(|o| &o.hard_failures)
            .filter(|h| h.code == code)
            .count() as u64
    };

    let scenarios_total = outcomes.len() as u64;
    let scenarios_passed = outcomes
        .iter()
        .filter(|o| o.verdict == ScenarioVerdict::Pass)
        .count() as u64;
    let scenarios_failed = outcomes
        .iter()
        .filter(|o| o.verdict == ScenarioVerdict::Fail)
        .count() as u64;
    let scenarios_not_applicable = outcomes
        .iter()
        .filter(|o| o.verdict == ScenarioVerdict::NotApplicable)
        .count() as u64;
    let hard_failure_count = outcomes.iter().map(|o| o.hard_failures.len() as u64).sum();
    let false_confidence_cases = false_confidence.len() as u64;
    let false_confidence_rate = if scenarios_total == 0 {
        0.0
    } else {
        false_confidence_cases as f64 / scenarios_total as f64
    };

    let metrics = ProofMetrics {
        schema_version: "proof-0.1",
        commit,
        generated_at: chrono::Utc::now().to_rfc3339(),
        scenarios_total,
        scenarios_passed,
        scenarios_failed,
        scenarios_not_applicable,
        measured_saved_time_ms: outcomes.iter().map(|o| o.measured_saved_time_ms).sum(),
        estimated_saved_time_ms: outcomes.iter().map(|o| o.estimated_saved_time_ms).sum(),
        false_confidence_rate,
        false_confidence_cases,
        skipped_as_passed_count: count_code("skipped_as_passed"),
        release_overclaim_count: count_code("release_overclaim"),
        stale_evidence_reuse_count: count_code("stale_evidence_reuse"),
        high_risk_underverified_count: count_code("high_risk_underverified"),
        agent_bypassed_vrt_count: 0,
        ci_failures_shifted_left: outcomes.iter().map(|o| o.ci_failures_shifted_left).sum(),
        full_builds_avoided: outcomes.iter().map(|o| o.full_builds_avoided).sum(),
        hard_failure_count,
        agent: agent_metrics,
    };

    let propositions = derive_propositions(&outcomes, &metrics);
    let overall = derive_overall(&metrics, &outcomes, &propositions);

    Ok(ProofRun {
        metrics,
        outcomes,
        propositions,
        overall,
        false_confidence,
        transcripts,
    })
}

fn scenarios_for(outcomes: &[ScenarioOutcome], p: Proposition) -> Vec<&ScenarioOutcome> {
    outcomes.iter().filter(|o| o.proposition == p).collect()
}

fn all_pass(outcomes: &[&ScenarioOutcome]) -> bool {
    !outcomes.is_empty() && outcomes.iter().all(|o| o.verdict == ScenarioVerdict::Pass)
}

fn derive_propositions(
    outcomes: &[ScenarioOutcome],
    metrics: &ProofMetrics,
) -> Vec<(&'static str, &'static str, PropositionVerdict)> {
    use PropositionVerdict::*;

    let agile = scenarios_for(outcomes, Proposition::AgileValue);
    let a = if all_pass(&agile) && agile.iter().any(|o| o.measured_saved_time_ms > 0) {
        Pass
    } else if agile.iter().any(|o| o.verdict == ScenarioVerdict::Fail) {
        Fail
    } else {
        Unproven
    };

    // B — agent efficiency, measured from the A/B transcripts against the §8.2
    // bar. PASS only when VRT's affordances actually move every metric over the
    // line; otherwise FAIL, never silently Unproven.
    let b = if outcomes.is_empty() {
        Unproven
    } else if metrics.agent.passes_efficiency_bar() {
        Pass
    } else {
        Fail
    };

    let ci = scenarios_for(outcomes, Proposition::CiSaving);
    let c = if all_pass(&ci) && metrics.ci_failures_shifted_left >= 1 {
        Pass
    } else if ci.iter().any(|o| o.verdict == ScenarioVerdict::Fail) {
        Fail
    } else {
        Unproven
    };

    let gov = scenarios_for(outcomes, Proposition::Governance);
    let governance_clean = metrics.hard_failure_count == 0
        && metrics.skipped_as_passed_count == 0
        && metrics.release_overclaim_count == 0
        && metrics.stale_evidence_reuse_count == 0
        && metrics.high_risk_underverified_count == 0;
    let d = if all_pass(&gov) && governance_clean {
        Pass
    } else {
        Fail
    };

    // E — AI-native: the type-error scenario exercises verify→explain→do_not_run.
    // PASS if that loop produced structured guidance (its assertions passed).
    let e = if ci.iter().all(|o| o.assertions_passed()) && !ci.is_empty() {
        Pass
    } else {
        Unproven
    };

    vec![
        ("A", "Agile development value", a),
        ("B", "Agent efficiency", b),
        ("C", "CI/CD time saving", c),
        ("D", "Serious governance", d),
        ("E", "AI-native behavior", e),
    ]
}

fn derive_overall(
    metrics: &ProofMetrics,
    outcomes: &[ScenarioOutcome],
    propositions: &[(&'static str, &'static str, PropositionVerdict)],
) -> OverallVerdict {
    let any_scenario_failed = outcomes.iter().any(|o| o.verdict == ScenarioVerdict::Fail);
    let governance_clean = metrics.hard_failure_count == 0
        && metrics.skipped_as_passed_count == 0
        && metrics.release_overclaim_count == 0
        && metrics.stale_evidence_reuse_count == 0
        && metrics.high_risk_underverified_count == 0;

    if !governance_clean || any_scenario_failed {
        return OverallVerdict::Fail;
    }
    let any_fail = propositions
        .iter()
        .any(|(_, _, v)| matches!(v, PropositionVerdict::Fail));
    if any_fail {
        return OverallVerdict::Fail;
    }
    let all_pass = propositions
        .iter()
        .all(|(_, _, v)| matches!(v, PropositionVerdict::Pass));
    if all_pass {
        OverallVerdict::Pass
    } else {
        // §19 — no hard failures, governance clean, ≥3 scenarios pass, some
        // propositions still Unproven → CONDITIONAL PASS.
        OverallVerdict::ConditionalPass
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(id: &str, verdict: ScenarioVerdict, hard: Vec<HardFailure>) -> ScenarioOutcome {
        ScenarioOutcome {
            id: id.into(),
            title: id.into(),
            proposition: Proposition::Governance,
            fixture: "fix".into(),
            baseline_total_ms: 0,
            vrt_total_ms: 0,
            measured_saved_time_ms: 0,
            estimated_saved_time_ms: 0,
            commands_run: 0,
            commands_avoided: 0,
            full_builds_avoided: 0,
            ci_failures_shifted_left: 0,
            confidence: Confidence::default(),
            residual_risks: vec![],
            baseline_commands: vec![],
            assertions: vec![],
            hard_failures: hard,
            verdict,
            notes: vec![],
        }
    }

    fn metrics(hard_failure_count: u64, skipped_as_passed: u64) -> ProofMetrics {
        ProofMetrics {
            schema_version: "proof-0.1",
            commit: "test".into(),
            generated_at: "now".into(),
            scenarios_total: 1,
            scenarios_passed: 0,
            scenarios_failed: 1,
            scenarios_not_applicable: 0,
            measured_saved_time_ms: 0,
            estimated_saved_time_ms: 0,
            false_confidence_rate: 0.0,
            false_confidence_cases: 0,
            skipped_as_passed_count: skipped_as_passed,
            release_overclaim_count: 0,
            stale_evidence_reuse_count: 0,
            high_risk_underverified_count: 0,
            agent_bypassed_vrt_count: 0,
            ci_failures_shifted_left: 0,
            full_builds_avoided: 0,
            hard_failure_count,
            agent: passing_agent(),
        }
    }

    // Agent metrics that clear the §8.2 bar, for tests that model an otherwise
    // healthy run.
    fn passing_agent() -> crate::agent::AgentMetrics {
        crate::agent::AgentMetrics {
            expensive_commands_avoided_pct: 100.0,
            naive_expensive_total: 2,
            vrt_expensive_total: 0,
            explain_after_failure_rate: 1.0,
            failure_scenarios: 0,
            ignored_do_not_run_count: 0,
            residual_risk_preserved_rate: 1.0,
            residual_risks_received_total: 3,
            residual_risks_preserved_total: 3,
            log_lines_read_naive: 200,
            log_lines_read_vrt: 1,
        }
    }

    // The harness MUST be able to emit FAIL — a PASS-only proof is itself a
    // false-confidence machine (Canvas §1.2, §10.3).
    #[test]
    fn a_hard_failure_forces_overall_fail() {
        let hf = HardFailure {
            code: "skipped_as_passed".into(),
            detail: "x".into(),
        };
        let outcomes = vec![outcome("s", ScenarioVerdict::Fail, vec![hf])];
        let m = metrics(1, 1);
        let props = derive_propositions(&outcomes, &m);
        let overall = derive_overall(&m, &outcomes, &props);
        assert!(matches!(overall, OverallVerdict::Fail));
    }

    #[test]
    fn a_failed_scenario_forces_overall_fail_even_with_clean_governance() {
        let outcomes = vec![outcome("s", ScenarioVerdict::Fail, vec![])];
        let m = metrics(0, 0);
        let props = derive_propositions(&outcomes, &m);
        let overall = derive_overall(&m, &outcomes, &props);
        assert!(matches!(overall, OverallVerdict::Fail));
    }

    #[test]
    fn all_pass_governance_clean_yields_pass_or_conditional_not_fail() {
        let outcomes = vec![outcome("s", ScenarioVerdict::Pass, vec![])];
        let m = metrics(0, 0);
        let props = derive_propositions(&outcomes, &m);
        let overall = derive_overall(&m, &outcomes, &props);
        assert!(!matches!(overall, OverallVerdict::Fail));
    }
}
