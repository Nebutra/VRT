//! The adversarial benchmark scenarios (Canvas §5). Each scenario applies a
//! real change to a real fixture, runs the naive baseline, runs VRT, and
//! asserts the governance + efficiency properties the change should exhibit.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::model::{AssertionResult, Proposition};
use crate::runner::Mutation;

pub struct Evaluated<'a> {
    pub report: &'a Value,
    pub explain: &'a Value,
    pub baseline_total_ms: u128,
    pub vrt_total_ms: u128,
    pub measured_saved_time_ms: u128,
    /// True only when every baseline command actually executed (so a timing
    /// claim is honest).
    pub baseline_fully_measured: bool,
}

pub type AssertFn = Box<dyn Fn(&Evaluated) -> Vec<AssertionResult>>;

pub struct Scenario {
    pub id: String,
    pub title: String,
    pub proposition: Proposition,
    pub fixture: PathBuf,
    pub fixture_label: String,
    pub mutations: Vec<Mutation>,
    pub baseline: Vec<(String, String)>,
    pub vrt_mode: String,
    pub high_risk: bool,
    pub config_mutated: bool,
    pub assertions: AssertFn,
}

fn ok(name: &str, passed: bool, detail: impl Into<String>) -> AssertionResult {
    AssertionResult {
        name: name.to_string(),
        passed,
        detail: detail.into(),
    }
}

fn release_conf(report: &Value) -> String {
    report
        .get("confidence")
        .and_then(|c| c.get("release"))
        .and_then(Value::as_str)
        .unwrap_or("insufficient")
        .to_string()
}

fn local_conf(report: &Value) -> String {
    report
        .get("confidence")
        .and_then(|c| c.get("local"))
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string()
}

fn skipped_caps(report: &Value) -> Vec<String> {
    report
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
        .unwrap_or_default()
}

fn residual_risks(report: &Value) -> Vec<String> {
    report
        .get("residual_risks")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

const PRICING_LABEL_CHANGED: &str =
    "export const PRICING_LABEL = \"Start your 14-day free trial\";\n";

const MATH_TYPE_ERROR: &str = "export function add(a: number, b: number): number {\n  return a + b;\n}\nexport function mul(a: number, b: number): number {\n  // Intentional type error: returning a string where number is required.\n  return `${a * b}`;\n}\n";

const SCHEMA_FIELD_ADDED: &str = "datasource db {\n  provider = \"postgresql\"\n  url      = env(\"DATABASE_URL\")\n}\n\ngenerator client {\n  provider = \"prisma-client-js\"\n}\n\nmodel User {\n  id        String  @id @default(cuid())\n  email     String  @unique\n  stripeId  String?\n}\n";

/// npm-run baseline matching the naive agent (Canvas §4.1 Baseline 1).
fn naive_baseline() -> Vec<(String, String)> {
    vec![
        ("build".into(), "npm run build".into()),
        ("test".into(), "npm run test".into()),
        ("lint".into(), "npm run lint".into()),
        ("typecheck".into(), "npm run typecheck".into()),
    ]
}

pub fn all_scenarios(xtask_dir: &Path, workspace_root: &Path) -> Vec<Scenario> {
    let ts_lib = xtask_dir.join("fixtures/ts-lib");
    let prisma = workspace_root.join("examples/prisma-next-app");

    vec![
        // §5.1 — UI / small TS change: full build must be skipped (with residual
        // risk), feedback must be measurably faster, release must not be high.
        Scenario {
            id: "ui-small-change".into(),
            title: "UI/TS small change → skip full build, faster feedback".into(),
            proposition: Proposition::AgileValue,
            fixture: ts_lib.clone(),
            fixture_label: "ts-lib".into(),
            mutations: vec![Mutation {
                path: "src/label.ts".into(),
                contents: PRICING_LABEL_CHANGED.into(),
            }],
            baseline: naive_baseline(),
            vrt_mode: "dev".into(),
            high_risk: false,
            config_mutated: false,
            assertions: Box::new(|e| {
                let skipped = skipped_caps(e.report);
                let build_skipped = skipped.iter().any(|c| c.contains("build"));
                let release = release_conf(e.report);
                let residuals = residual_risks(e.report);
                let mut out = vec![
                    ok(
                        "full_build_skipped",
                        build_skipped,
                        format!("skipped capabilities: {skipped:?}"),
                    ),
                    ok(
                        "residual_risk_disclosed",
                        !residuals.is_empty(),
                        format!("{} residual risks disclosed", residuals.len()),
                    ),
                    ok(
                        "release_not_high",
                        release != "high",
                        format!("release confidence = {release}"),
                    ),
                ];
                if e.baseline_fully_measured {
                    out.push(ok(
                        "feedback_faster_than_baseline",
                        e.measured_saved_time_ms > 0,
                        format!(
                            "vrt {}ms vs baseline {}ms (measured saved {}ms)",
                            e.vrt_total_ms, e.baseline_total_ms, e.measured_saved_time_ms
                        ),
                    ));
                    // §8.1 — first-feedback cycle time reduced >= 40%.
                    let threshold = (e.baseline_total_ms as f64 * 0.6) as u128;
                    out.push(ok(
                        "first_feedback_reduced_40pct",
                        e.vrt_total_ms <= threshold,
                        format!(
                            "vrt {}ms must be <= 60% of baseline {}ms ({}ms)",
                            e.vrt_total_ms, e.baseline_total_ms, threshold
                        ),
                    ));
                }
                out
            }),
        },
        // §5.2 — TypeScript type error: VRT fails locally on the cheap check,
        // explain names the root cause, do_not_run includes build.
        Scenario {
            id: "type-error".into(),
            title: "TypeScript type error → fail early, root cause, do_not_run build".into(),
            proposition: Proposition::CiSaving,
            fixture: ts_lib.clone(),
            fixture_label: "ts-lib".into(),
            mutations: vec![Mutation {
                path: "src/math.ts".into(),
                contents: MATH_TYPE_ERROR.into(),
            }],
            baseline: naive_baseline(),
            vrt_mode: "dev".into(),
            high_risk: false,
            config_mutated: false,
            assertions: Box::new(|e| {
                let status = e
                    .report
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let failed_locally = status == "failed"
                    || e.report
                        .get("evidence")
                        .and_then(|ev| ev.get("checks"))
                        .and_then(Value::as_array)
                        .map(|cs| {
                            cs.iter()
                                .any(|c| c.get("status").and_then(Value::as_str) == Some("failed"))
                        })
                        .unwrap_or(false);
                let root_causes = e
                    .explain
                    .get("root_cause_candidates")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(" | ")
                    })
                    .unwrap_or_default();
                let do_not_run = e
                    .explain
                    .get("do_not_run")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|c| c.get("command").and_then(Value::as_str))
                            .collect::<Vec<_>>()
                            .join(" | ")
                    })
                    .unwrap_or_default();
                vec![
                    ok(
                        "failed_locally_before_ci",
                        failed_locally,
                        format!("vrt status = {status}"),
                    ),
                    ok(
                        "root_cause_identified",
                        !root_causes.is_empty(),
                        format!("root cause candidates: {root_causes}"),
                    ),
                    ok(
                        "do_not_run_includes_build",
                        do_not_run.to_lowercase().contains("build"),
                        format!("do_not_run: {do_not_run}"),
                    ),
                ]
            }),
        },
        // §5.4 — Prisma schema change: database_schema risk forces escalation;
        // release stays insufficient; local downgraded from high.
        Scenario {
            id: "prisma-schema".into(),
            title: "Prisma schema change → database_schema risk, escalation, insufficient release"
                .into(),
            proposition: Proposition::Governance,
            fixture: prisma,
            fixture_label: "prisma-next-app".into(),
            mutations: vec![Mutation {
                path: "prisma/schema.prisma".into(),
                contents: SCHEMA_FIELD_ADDED.into(),
            }],
            // Direct binary, NOT `npx`: when prisma is not installed locally
            // this degrades to not_available (0ms) instead of letting npx
            // download it over the network and pollute measured savings.
            baseline: vec![
                ("validate".into(), "prisma validate".into()),
                ("generate".into(), "prisma generate".into()),
            ],
            vrt_mode: "dev".into(),
            high_risk: true,
            config_mutated: false,
            assertions: Box::new(|e| {
                let release = release_conf(e.report);
                let local = local_conf(e.report);
                vec![
                    ok(
                        "release_insufficient",
                        release == "insufficient",
                        format!("release confidence = {release}"),
                    ),
                    ok(
                        "escalation_downgraded_local",
                        local != "high",
                        format!("local confidence = {local} (escalation should downgrade from high)"),
                    ),
                ]
            }),
        },
    ]
}
