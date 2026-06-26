//! Proposition B is only meaningful if it can FAIL. These tests prove the
//! agent-efficiency metrics are contingent on VRT actually supplying its
//! affordances — strip them and the bar is missed (Canvas §10.4, §14).

use serde_json::json;
use xtask::agent::{aggregate, naive_transcript, vrt_transcript};
use xtask::model::{CommandRun, RunStatus};

fn cmd(label: &str, status: RunStatus, lines: u64) -> CommandRun {
    CommandRun {
        label: label.into(),
        command: format!("npm run {label}"),
        status,
        exit_code: Some(if status == RunStatus::Failed { 1 } else { 0 }),
        duration_ms: 10,
        measured: true,
        output_lines: lines,
    }
}

// A healthy VRT report for a small TS change: typecheck only, build/test/lint
// skipped with residual risks.
fn good_ui_report() -> serde_json::Value {
    json!({
        "status": "passed",
        "checks_run": 1,
        "residual_risks": ["build not proven", "tests not proven", "lint not proven"],
        "evidence": {
            "checks": [{"name": "workspace-typecheck", "command": "npm run typecheck", "status": "passed"}],
            "skipped": [{"capability_id": "workspace-build", "residual_risk": "x"}]
        }
    })
}

#[test]
fn vrt_agent_avoids_expensive_commands_the_naive_agent_runs() {
    let baseline = vec![
        cmd("build", RunStatus::Passed, 5),
        cmd("test", RunStatus::Passed, 3),
        cmd("lint", RunStatus::Passed, 1),
        cmd("typecheck", RunStatus::Passed, 1),
    ];
    let naive = naive_transcript("ui", &baseline);
    let vrt = vrt_transcript("ui", &good_ui_report(), &serde_json::Value::Null);
    let m = aggregate(&[(naive, vrt)]);
    // naive ran build+test (2 expensive); vrt ran none.
    assert_eq!(m.naive_expensive_total, 2);
    assert_eq!(m.vrt_expensive_total, 0);
    assert_eq!(m.expensive_commands_avoided_pct, 100.0);
}

#[test]
fn good_outputs_clear_the_efficiency_bar() {
    let baseline = vec![
        cmd("build", RunStatus::Passed, 5),
        cmd("test", RunStatus::Passed, 3),
    ];
    let naive = naive_transcript("ui", &baseline);
    let vrt = vrt_transcript("ui", &good_ui_report(), &serde_json::Value::Null);
    let m = aggregate(&[(naive, vrt)]);
    assert!(m.passes_efficiency_bar());
}

#[test]
fn bar_fails_when_vrt_discloses_no_residual_risk_while_skipping() {
    // FALSIFIABILITY: strip residual_risks → preserved rate 0 → bar missed.
    let report = json!({
        "status": "passed",
        "checks_run": 1,
        "residual_risks": [],
        "evidence": {
            "checks": [{"name": "workspace-typecheck", "command": "npm run typecheck", "status": "passed"}],
            "skipped": [{"capability_id": "workspace-build", "residual_risk": "x"}]
        }
    });
    let baseline = vec![cmd("build", RunStatus::Passed, 5)];
    let naive = naive_transcript("ui", &baseline);
    let vrt = vrt_transcript("ui", &report, &serde_json::Value::Null);
    let m = aggregate(&[(naive, vrt)]);
    assert_eq!(m.residual_risk_preserved_rate, 0.0);
    assert!(!m.passes_efficiency_bar());
}

#[test]
fn bar_fails_when_explain_yields_no_guidance_on_failure() {
    // FALSIFIABILITY: a failure with an empty explain → explain rate 0 → miss.
    let report = json!({
        "status": "failed",
        "checks_run": 1,
        "residual_risks": ["build not proven"],
        "evidence": {
            "checks": [{"name": "workspace-typecheck", "command": "npm run typecheck", "status": "failed"}],
            "skipped": [{"capability_id": "workspace-build", "residual_risk": "x"}]
        }
    });
    let empty_explain = json!({"root_cause_candidates": [], "do_not_run": []});
    let baseline = vec![cmd("build", RunStatus::Failed, 200)];
    let naive = naive_transcript("te", &baseline);
    let vrt = vrt_transcript("te", &report, &empty_explain);
    let m = aggregate(&[(naive, vrt)]);
    assert_eq!(m.failure_scenarios, 1);
    assert_eq!(m.explain_after_failure_rate, 0.0);
    assert!(!m.passes_efficiency_bar());
}

#[test]
fn vrt_agent_reads_fewer_log_lines_than_naive_on_failure() {
    let report = json!({
        "status": "failed",
        "checks_run": 1,
        "residual_risks": ["build not proven"],
        "evidence": {
            "checks": [{"name": "workspace-typecheck", "command": "npm run typecheck", "status": "failed"}],
            "skipped": []
        }
    });
    let explain = json!({
        "root_cause_candidates": ["src/math.ts:6:3 TS2322"],
        "do_not_run": [{"command": "full build", "reason": "low info gain"}]
    });
    let baseline = vec![cmd("build", RunStatus::Failed, 200)];
    let naive = naive_transcript("te", &baseline);
    let vrt = vrt_transcript("te", &report, &explain);
    let m = aggregate(&[(naive, vrt)]);
    assert_eq!(m.log_lines_read_naive, 200);
    assert!(m.log_lines_read_vrt < m.log_lines_read_naive);
    assert_eq!(m.ignored_do_not_run_count, 0);
}
