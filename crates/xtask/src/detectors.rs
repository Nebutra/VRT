//! Canvas §7 hard-failure detectors.
//!
//! Each is a pure function over a parsed `vrt verify --json` agent report. They
//! encode the non-negotiable governance invariants: skipped is not passed, dev
//! confidence is not release readiness, stale evidence is not current proof, and
//! a high-risk change may not be waved through as low-risk.

use crate::model::HardFailure;
use serde_json::Value;

fn fail(code: &str, detail: impl Into<String>) -> HardFailure {
    HardFailure {
        code: code.to_string(),
        detail: detail.into(),
    }
}

/// §7.1 — a skipped capability must never be reported as passed, and any skip
/// must carry a disclosed residual risk.
pub fn skipped_as_passed(report: &Value) -> Option<HardFailure> {
    let evidence = report.get("evidence");
    let passed: Vec<&str> = evidence
        .and_then(|e| e.get("checks"))
        .and_then(Value::as_array)
        .map(|checks| {
            checks
                .iter()
                .filter(|c| c.get("status").and_then(Value::as_str) == Some("passed"))
                .filter_map(|c| c.get("name").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();

    let skipped: Vec<&str> = evidence
        .and_then(|e| e.get("skipped"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|s| s.get("capability_id").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();

    if let Some(overlap) = skipped.iter().find(|id| passed.contains(id)) {
        return Some(fail(
            "skipped_as_passed",
            format!("capability '{overlap}' appears in both passed checks and skipped list"),
        ));
    }

    // Skips with no disclosed residual risk are silent governance debt.
    let skipped_count = report
        .get("checks_skipped")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let residual_count = report
        .get("residual_risks")
        .and_then(Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0);
    if skipped_count > 0 && residual_count == 0 {
        return Some(fail(
            "skipped_as_passed",
            format!("{skipped_count} checks skipped but zero residual risks disclosed"),
        ));
    }

    None
}

/// §7.2 — release confidence may not claim readiness without a release gate.
/// Local dev/merge runs must keep `release` at a non-ready value.
pub fn release_overclaim(report: &Value) -> Option<HardFailure> {
    let release = report
        .get("confidence")
        .and_then(|c| c.get("release"))
        .and_then(Value::as_str)
        .unwrap_or("insufficient");
    if matches!(release, "high" | "ready" | "release_ready") {
        return Some(fail(
            "release_overclaim",
            format!("release confidence '{release}' claimed without a release gate"),
        ));
    }
    None
}

/// §7.3 — if a build/config input changed, prior evidence may not be reused as
/// current proof unless VRT recorded why it became stale.
pub fn stale_reuse(report: &Value, config_mutated: bool) -> Option<HardFailure> {
    if !config_mutated {
        return None;
    }
    let reused = report
        .get("checks_reused")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let stale_reasons = report
        .get("evidence")
        .and_then(|e| e.get("stale_reasons"))
        .and_then(Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0);
    if reused > 0 && stale_reasons == 0 {
        return Some(fail(
            "stale_evidence_reuse",
            format!("config changed but {reused} checks reused with no stale_reasons recorded"),
        ));
    }
    None
}

/// §7.4 — a high-risk change (auth/billing/schema/migration) may not yield a
/// release confidence above `insufficient` from a local run.
pub fn high_risk_underverified(report: &Value, high_risk: bool) -> Option<HardFailure> {
    if !high_risk {
        return None;
    }
    let release = report
        .get("confidence")
        .and_then(|c| c.get("release"))
        .and_then(Value::as_str)
        .unwrap_or("insufficient");
    if release != "insufficient" {
        return Some(fail(
            "high_risk_underverified",
            format!("high-risk change but release confidence is '{release}', not 'insufficient'"),
        ));
    }
    None
}

/// Run every universal detector for a scenario and collect what fired.
pub fn scan_all(report: &Value, config_mutated: bool, high_risk: bool) -> Vec<HardFailure> {
    [
        skipped_as_passed(report),
        release_overclaim(report),
        stale_reuse(report, config_mutated),
        high_risk_underverified(report, high_risk),
    ]
    .into_iter()
    .flatten()
    .collect()
}
