//! Adversarial tests for the Canvas §7 hard-failure detectors.
//!
//! Every detector has a POSITIVE control (the bad thing happened → detector
//! fires) and a NEGATIVE control (an innocent change → detector stays silent).
//! This is the §10.4 anti-cheat rule: a detector that can only say "fine" is
//! itself a false-confidence machine.

use serde_json::json;
use xtask::detectors;

// ---------------------------------------------------------------------------
// §7.1 — skipped is not passed
// ---------------------------------------------------------------------------

#[test]
fn skipped_as_passed_fires_when_a_skipped_capability_is_also_reported_passed() {
    let report = json!({
        "checks_run": 1,
        "checks_skipped": 1,
        "residual_risks": ["build not proven"],
        "confidence": {"local": "high", "merge": "low", "release": "insufficient"},
        "evidence": {
            "checks": [{"name": "workspace-build", "status": "passed"}],
            "skipped": [{"capability_id": "workspace-build", "residual_risk": "x"}]
        }
    });
    assert!(detectors::skipped_as_passed(&report).is_some());
}

#[test]
fn skipped_as_passed_fires_when_skips_exist_but_no_residual_risk_is_disclosed() {
    // Skipping work without disclosing residual risk is the §2.3 governance
    // failure dressed up as a pass.
    let report = json!({
        "checks_run": 1,
        "checks_skipped": 2,
        "residual_risks": [],
        "confidence": {"local": "high", "merge": "low", "release": "insufficient"},
        "evidence": {"checks": [{"name": "typecheck", "status": "passed"}], "skipped": []}
    });
    assert!(detectors::skipped_as_passed(&report).is_some());
}

#[test]
fn skipped_as_passed_silent_on_honest_skip_with_disclosed_residual_risk() {
    // NEGATIVE control: skips exist, are disclosed, and never claimed as passed.
    let report = json!({
        "checks_run": 1,
        "checks_skipped": 1,
        "residual_risks": ["Production bundler behavior not verified."],
        "confidence": {"local": "high", "merge": "low", "release": "insufficient"},
        "evidence": {
            "checks": [{"name": "workspace-typecheck", "status": "passed"}],
            "skipped": [{"capability_id": "workspace-build", "residual_risk": "Production bundler behavior not verified."}]
        }
    });
    assert!(detectors::skipped_as_passed(&report).is_none());
}

// ---------------------------------------------------------------------------
// §7.2 — dev confidence is not release readiness
// ---------------------------------------------------------------------------

#[test]
fn release_overclaim_fires_on_release_high_without_a_release_gate() {
    let report = json!({
        "confidence": {"local": "high", "merge": "medium", "release": "high"}
    });
    assert!(detectors::release_overclaim(&report).is_some());
}

#[test]
fn release_overclaim_fires_on_release_ready_label() {
    let report = json!({
        "confidence": {"local": "high", "merge": "medium", "release": "ready"}
    });
    assert!(detectors::release_overclaim(&report).is_some());
}

#[test]
fn release_overclaim_silent_on_insufficient_release_confidence() {
    // NEGATIVE control: the honest dev-mode answer.
    let report = json!({
        "confidence": {"local": "high", "merge": "low", "release": "insufficient"}
    });
    assert!(detectors::release_overclaim(&report).is_none());
}

// ---------------------------------------------------------------------------
// §7.3 — stale evidence is not current proof
// ---------------------------------------------------------------------------

#[test]
fn stale_reuse_fires_when_config_changed_but_evidence_was_reused_without_staleness() {
    let report = json!({
        "checks_reused": 2,
        "evidence": {"stale_reasons": []}
    });
    // A build-config mutation happened in this scenario.
    assert!(detectors::stale_reuse(&report, true).is_some());
}

#[test]
fn stale_reuse_silent_when_no_config_change_occurred() {
    // NEGATIVE control: reusing evidence after a copy-text change is correct.
    let report = json!({
        "checks_reused": 2,
        "evidence": {"stale_reasons": []}
    });
    assert!(detectors::stale_reuse(&report, false).is_none());
}

#[test]
fn stale_reuse_silent_when_config_changed_and_staleness_was_recorded() {
    // NEGATIVE control: config changed AND VRT invalidated reuse → correct.
    let report = json!({
        "checks_reused": 0,
        "evidence": {"stale_reasons": ["config hash changed; previous checks were not reused"]}
    });
    assert!(detectors::stale_reuse(&report, true).is_none());
}

// ---------------------------------------------------------------------------
// §7.4 — high-risk change handled as low-risk
// ---------------------------------------------------------------------------

#[test]
fn high_risk_underverified_fires_when_release_not_insufficient_for_high_risk_change() {
    let report = json!({
        "confidence": {"local": "high", "merge": "medium", "release": "medium"}
    });
    assert!(detectors::high_risk_underverified(&report, true).is_some());
}

#[test]
fn high_risk_underverified_silent_when_release_insufficient_for_high_risk_change() {
    // NEGATIVE control: high-risk change correctly held to insufficient release.
    let report = json!({
        "confidence": {"local": "medium", "merge": "low", "release": "insufficient"}
    });
    assert!(detectors::high_risk_underverified(&report, true).is_none());
}

#[test]
fn high_risk_underverified_silent_for_low_risk_change() {
    // NEGATIVE control: not a high-risk scenario, so the gate does not apply.
    let report = json!({
        "confidence": {"local": "high", "merge": "medium", "release": "medium"}
    });
    assert!(detectors::high_risk_underverified(&report, false).is_none());
}
