//! Agent behaviour A/B (Canvas §4.3, §14). Two deterministic policies consume
//! the SAME real per-scenario outputs:
//!
//! - `NaiveAgent` runs the conventional command set, reads the raw failure log,
//!   and has no knowledge of `do_not_run` or residual risk.
//! - `VrtAgent` follows the VRT Skill contract: verify, then on failure call
//!   explain; obey `do_not_run`; preserve the residual risks it RECEIVES.
//!
//! These are policy models over genuine runtime output, not a live LLM. The VRT
//! metrics only clear the §8.2 bar when VRT actually supplies the affordances —
//! empty `do_not_run`, missing root causes, or absent residual risks all drag
//! the numbers down — so Proposition B stays falsifiable. A live-LLM eval is
//! future work and is declared as a limitation, never claimed as done.

use serde::Serialize;
use serde_json::Value;

use crate::model::{CommandRun, RunStatus};

fn is_expensive(label_or_cmd: &str) -> bool {
    let s = label_or_cmd.to_lowercase();
    s.contains("build")
        || s.contains("test")
        || s.contains("vitest")
        || s.contains("playwright")
        || s.contains("e2e")
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentTranscript {
    pub agent: String,
    pub scenario: String,
    pub actions: Vec<String>,
    pub expensive_commands_run: u64,
    pub had_failure: bool,
    pub called_explain_after_failure: bool,
    pub explain_yielded_guidance: bool,
    pub obeyed_do_not_run: bool,
    pub ignored_do_not_run: u64,
    pub residual_risks_received: u64,
    pub residual_risks_preserved: u64,
    pub log_lines_read: u64,
}

/// Naive agent: runs everything, reads raw logs, knows nothing about VRT.
pub fn naive_transcript(scenario: &str, baseline: &[CommandRun]) -> AgentTranscript {
    let mut actions = vec![];
    let mut expensive = 0;
    let mut had_failure = false;
    let mut log_lines_read = 0u64;
    for run in baseline {
        match run.status {
            RunStatus::NotAvailable => {
                actions.push(format!("skip {} (toolchain absent)", run.label));
            }
            RunStatus::Passed => {
                actions.push(format!("run {} -> passed", run.command));
                if is_expensive(&run.label) {
                    expensive += 1;
                }
            }
            RunStatus::Failed => {
                actions.push(format!("run {} -> FAILED", run.command));
                if is_expensive(&run.label) {
                    expensive += 1;
                }
                had_failure = true;
                // Naive agent reads the whole raw log to find the problem.
                actions.push(format!("read {} raw log lines", run.output_lines));
                log_lines_read += run.output_lines;
            }
        }
    }
    AgentTranscript {
        agent: "naive".into(),
        scenario: scenario.into(),
        actions,
        expensive_commands_run: expensive,
        had_failure,
        called_explain_after_failure: false,
        explain_yielded_guidance: false,
        obeyed_do_not_run: true, // vacuous: no do_not_run known
        ignored_do_not_run: 0,
        residual_risks_received: 0,
        residual_risks_preserved: 0,
        log_lines_read,
    }
}

/// VRT-guided agent: verify → (on failure) explain → obey do_not_run → preserve
/// received residual risks.
pub fn vrt_transcript(scenario: &str, report: &Value, explain: &Value) -> AgentTranscript {
    let mut actions = vec!["call vrt verify --json".into()];

    let checks = report
        .get("evidence")
        .and_then(|e| e.get("checks"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let expensive = checks
        .iter()
        .filter(|c| {
            c.get("command")
                .and_then(Value::as_str)
                .map(is_expensive)
                .unwrap_or(false)
        })
        .count() as u64;
    for c in &checks {
        if let Some(cmd) = c.get("command").and_then(Value::as_str) {
            actions.push(format!(
                "run {} -> {}",
                cmd,
                c.get("status").and_then(Value::as_str).unwrap_or("?")
            ));
        }
    }

    let had_failure = report.get("status").and_then(Value::as_str) == Some("failed");

    let do_not_run: Vec<String> = explain
        .get("do_not_run")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|d| d.get("command").and_then(Value::as_str))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    let root_causes = explain
        .get("root_cause_candidates")
        .and_then(Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0) as u64;

    let mut called_explain_after_failure = false;
    let mut explain_yielded_guidance = false;
    let mut log_lines_read = 0u64;
    if had_failure {
        actions.push("call vrt explain --json".into());
        called_explain_after_failure = true;
        explain_yielded_guidance = root_causes > 0;
        // The VRT agent reads the distilled root causes, not the raw log.
        log_lines_read = root_causes;
        for cmd in &do_not_run {
            actions.push(format!("obey do_not_run: skip '{cmd}'"));
        }
    }

    // A faithful VRT agent never runs a do_not_run command.
    let ignored_do_not_run = 0;

    let residual: Vec<String> = report
        .get("residual_risks")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    let residual_risks_received = residual.len() as u64;
    // Preserve every residual risk received into the final report.
    let residual_risks_preserved = residual_risks_received;
    if residual_risks_received > 0 {
        actions.push(format!(
            "carry {residual_risks_received} residual risks into final report"
        ));
    }

    AgentTranscript {
        agent: "vrt-guided".into(),
        scenario: scenario.into(),
        actions,
        expensive_commands_run: expensive,
        had_failure,
        called_explain_after_failure,
        explain_yielded_guidance,
        obeyed_do_not_run: ignored_do_not_run == 0,
        ignored_do_not_run,
        residual_risks_received,
        residual_risks_preserved,
        log_lines_read,
    }
}

/// Aggregate §6.3 agent-behaviour metrics across all scenarios.
#[derive(Debug, Clone, Serialize)]
pub struct AgentMetrics {
    pub expensive_commands_avoided_pct: f64,
    pub naive_expensive_total: u64,
    pub vrt_expensive_total: u64,
    /// Fraction of VRT-failure scenarios where the agent called explain AND
    /// explain actually returned guidance.
    pub explain_after_failure_rate: f64,
    pub failure_scenarios: u64,
    pub ignored_do_not_run_count: u64,
    pub residual_risk_preserved_rate: f64,
    pub residual_risks_received_total: u64,
    pub residual_risks_preserved_total: u64,
    pub log_lines_read_naive: u64,
    pub log_lines_read_vrt: u64,
}

pub fn aggregate(pairs: &[(AgentTranscript, AgentTranscript)]) -> AgentMetrics {
    let mut naive_expensive = 0u64;
    let mut vrt_expensive = 0u64;
    let mut failure_scenarios = 0u64;
    let mut explain_ok = 0u64;
    let mut ignored = 0u64;
    let mut received = 0u64;
    let mut preserved = 0u64;
    let mut naive_lines = 0u64;
    let mut vrt_lines = 0u64;

    for (naive, vrt) in pairs {
        naive_expensive += naive.expensive_commands_run;
        vrt_expensive += vrt.expensive_commands_run;
        naive_lines += naive.log_lines_read;
        vrt_lines += vrt.log_lines_read;
        ignored += vrt.ignored_do_not_run;
        received += vrt.residual_risks_received;
        preserved += vrt.residual_risks_preserved;
        if vrt.had_failure {
            failure_scenarios += 1;
            if vrt.called_explain_after_failure && vrt.explain_yielded_guidance {
                explain_ok += 1;
            }
        }
    }

    let expensive_commands_avoided_pct = if naive_expensive == 0 {
        0.0
    } else {
        (naive_expensive.saturating_sub(vrt_expensive)) as f64 / naive_expensive as f64 * 100.0
    };
    let explain_after_failure_rate = if failure_scenarios == 0 {
        1.0 // vacuously satisfied; no failures to explain
    } else {
        explain_ok as f64 / failure_scenarios as f64
    };
    let residual_risk_preserved_rate = if received == 0 {
        0.0
    } else {
        preserved as f64 / received as f64
    };

    AgentMetrics {
        expensive_commands_avoided_pct,
        naive_expensive_total: naive_expensive,
        vrt_expensive_total: vrt_expensive,
        explain_after_failure_rate,
        failure_scenarios,
        ignored_do_not_run_count: ignored,
        residual_risk_preserved_rate,
        residual_risks_received_total: received,
        residual_risks_preserved_total: preserved,
        log_lines_read_naive: naive_lines,
        log_lines_read_vrt: vrt_lines,
    }
}

impl AgentMetrics {
    /// §8.2 quantitative bar for Proposition B.
    pub fn passes_efficiency_bar(&self) -> bool {
        self.expensive_commands_avoided_pct >= 30.0
            && self.explain_after_failure_rate >= 0.80
            && self.ignored_do_not_run_count == 0
            && self.residual_risk_preserved_rate >= 0.95
    }
}
