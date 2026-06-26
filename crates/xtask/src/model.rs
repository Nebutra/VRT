//! Data model for the VRT adversarial proof package.
//!
//! Every field here is chosen to make self-deception expensive: measured and
//! estimated savings are separate types, skipped work is never folded into a
//! pass count, and a hard failure is a first-class value that gates the verdict.

use serde::Serialize;

/// Outcome of a single executed command. `NotAvailable` is a real, honest
/// state: when a toolchain is missing we record it instead of inventing a
/// duration (Canvas §2.1, §7.7).
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Passed,
    Failed,
    NotAvailable,
}

impl RunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RunStatus::Passed => "passed",
            RunStatus::Failed => "failed",
            RunStatus::NotAvailable => "not_available",
        }
    }
}

/// A baseline command actually executed in the clean room. `measured` is true
/// only when the command really ran and the duration is wall-clock truth.
#[derive(Debug, Clone, Serialize)]
pub struct CommandRun {
    pub label: String,
    pub command: String,
    pub status: RunStatus,
    pub exit_code: Option<i32>,
    pub duration_ms: u128,
    pub measured: bool,
    /// Lines of stdout+stderr the command produced — the raw log a naive agent
    /// would read on failure (Canvas §6.1 log_lines_read_by_agent).
    pub output_lines: u64,
}

/// A single assertion checked against a scenario's VRT evidence.
///
/// `blocking` assertions encode governance-critical behaviour: a blocking
/// failure fails the scenario. `advisory` assertions encode quality/DX/
/// observability expectations that VRT does not yet meet — their failure is a
/// recorded gap, surfaced loudly but not treated as a governance break (the
/// §7 hard-failure detectors guard the true governance invariants).
#[derive(Debug, Clone, Serialize)]
pub struct AssertionResult {
    pub name: String,
    pub passed: bool,
    pub blocking: bool,
    pub detail: String,
}

/// A Canvas §7 hard-failure occurrence. Any non-empty set forces an overall
/// FAIL verdict regardless of timing wins.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HardFailure {
    /// Stable code, e.g. "skipped_as_passed", "release_overclaim".
    pub code: String,
    pub detail: String,
}

/// Which adversarial proposition (Canvas §3) a scenario primarily exercises.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum Proposition {
    /// A — agile-development value (faster feedback loop).
    AgileValue,
    /// B — agent efficiency (fewer expensive commands).
    AgentEfficiency,
    /// C — CI/CD time saving (shift-left).
    CiSaving,
    /// D — serious governance (no false confidence).
    Governance,
    /// E — AI-native behaviour.
    AiNative,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioVerdict {
    Pass,
    Fail,
    /// All blocking assertions passed, but one or more advisory assertions
    /// failed — a real, recorded gap that does not break governance.
    PassWithGaps,
    /// The scenario could not be measured because a required toolchain was
    /// absent. Honest non-result, never silently counted as a pass.
    NotApplicable,
}

/// Confidence triple lifted verbatim from VRT's agent report.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Confidence {
    pub local: String,
    pub merge: String,
    pub release: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioOutcome {
    pub id: String,
    pub title: String,
    pub proposition: Proposition,
    pub fixture: String,
    /// Real wall-clock for the naive-agent baseline command set.
    pub baseline_total_ms: u128,
    /// Real wall-clock for the VRT verify invocation.
    pub vrt_total_ms: u128,
    /// baseline - vrt, but only when BOTH sides really executed (measured).
    pub measured_saved_time_ms: u128,
    /// VRT's own conservative estimate, reported separately and never merged
    /// into the measured figure (Canvas §2.2).
    pub estimated_saved_time_ms: u128,
    pub commands_run: u64,
    pub commands_avoided: u64,
    pub full_builds_avoided: u64,
    pub ci_failures_shifted_left: u64,
    pub confidence: Confidence,
    pub residual_risks: Vec<String>,
    pub baseline_commands: Vec<CommandRun>,
    pub assertions: Vec<AssertionResult>,
    pub hard_failures: Vec<HardFailure>,
    pub verdict: ScenarioVerdict,
    /// Notes that keep the result honest (e.g. why a command was not_available).
    pub notes: Vec<String>,
}

impl ScenarioOutcome {
    /// All blocking (governance-critical) assertions passed.
    pub fn blocking_passed(&self) -> bool {
        self.assertions.iter().filter(|a| a.blocking).all(|a| a.passed)
    }

    /// Advisory assertions that failed — recorded gaps, not governance breaks.
    pub fn advisory_gaps(&self) -> Vec<&AssertionResult> {
        self.assertions
            .iter()
            .filter(|a| !a.blocking && !a.passed)
            .collect()
    }
}

/// Aggregate metrics written to `.vrt-proof/metrics.json`. Mirrors Canvas
/// §11.2 `proof-0.1` and extends it with the governance counters §6.2 demands.
#[derive(Debug, Clone, Serialize)]
pub struct ProofMetrics {
    pub schema_version: &'static str,
    pub commit: String,
    pub generated_at: String,
    pub scenarios_total: u64,
    pub scenarios_passed: u64,
    pub scenarios_failed: u64,
    pub scenarios_not_applicable: u64,
    pub measured_saved_time_ms: u128,
    pub estimated_saved_time_ms: u128,
    pub false_confidence_rate: f64,
    pub false_confidence_cases: u64,
    pub skipped_as_passed_count: u64,
    pub release_overclaim_count: u64,
    pub stale_evidence_reuse_count: u64,
    pub high_risk_underverified_count: u64,
    pub agent_bypassed_vrt_count: u64,
    pub ci_failures_shifted_left: u64,
    pub full_builds_avoided: u64,
    pub hard_failure_count: u64,
    /// Advisory gaps surfaced by scenarios — observability/DX/timing
    /// expectations VRT does not (always) meet. Real, recorded, NOT §7 hard
    /// failures. A non-zero count caps the suite at CONDITIONAL PASS (§20).
    pub advisory_gaps: u64,
    /// §6.3 agent-behaviour metrics (Proposition B).
    pub agent: crate::agent::AgentMetrics,
}

/// Per-proposition pass/fail used by the Canvas §18 verdict block.
#[derive(Debug, Clone, Copy, Serialize)]
pub enum PropositionVerdict {
    Pass,
    Fail,
    /// Not enough evidence in this run to claim either way (kept distinct from
    /// PASS so we never over-claim — Canvas §20).
    Unproven,
}

impl PropositionVerdict {
    pub fn label(self) -> &'static str {
        match self {
            PropositionVerdict::Pass => "PASS",
            PropositionVerdict::Fail => "FAIL",
            PropositionVerdict::Unproven => "UNPROVEN",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum OverallVerdict {
    Pass,
    Fail,
    ConditionalPass,
}

impl OverallVerdict {
    pub fn label(self) -> &'static str {
        match self {
            OverallVerdict::Pass => "PASS",
            OverallVerdict::Fail => "FAIL",
            OverallVerdict::ConditionalPass => "CONDITIONAL PASS",
        }
    }
}
