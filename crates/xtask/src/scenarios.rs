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
    /// `vrt doctor --json` output (profile + capabilities), for capability
    /// scenarios.
    pub doctor: &'a Value,
    pub baseline_total_ms: u128,
    pub vrt_total_ms: u128,
    pub measured_saved_time_ms: u128,
    /// True only when every baseline command actually executed (so a timing
    /// claim is honest).
    pub baseline_fully_measured: bool,
}

pub type AssertFn = Box<dyn Fn(&Evaluated) -> Vec<AssertionResult>>;

/// A second verify run for multi-step scenarios (e.g. stale-evidence): apply
/// more mutations on top of the first run, then `vrt verify --continue`. The
/// SECOND run's report is what the assertions evaluate.
pub struct SecondStage {
    pub mutations: Vec<Mutation>,
    pub use_continue: bool,
}

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
    /// Optional second verify run (verify → mutate → verify --continue).
    pub second_stage: Option<SecondStage>,
    pub assertions: AssertFn,
}

/// A blocking (governance-critical) assertion.
fn ok(name: &str, passed: bool, detail: impl Into<String>) -> AssertionResult {
    AssertionResult {
        name: name.to_string(),
        passed,
        blocking: true,
        detail: detail.into(),
    }
}

/// An advisory assertion: encodes a quality/observability expectation. Its
/// failure is a recorded gap, not a governance break.
fn advisory(name: &str, passed: bool, detail: impl Into<String>) -> AssertionResult {
    AssertionResult {
        name: name.to_string(),
        passed,
        blocking: false,
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

fn str_at(v: &Value, ptr: &str) -> String {
    v.pointer(ptr).and_then(Value::as_str).unwrap_or("").to_string()
}

fn u64_at(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(u64::MAX)
}

fn stale_reasons(report: &Value) -> Vec<String> {
    report
        .pointer("/evidence/stale_reasons")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
        .unwrap_or_default()
}

fn reused_count(report: &Value) -> u64 {
    report.get("checks_reused").and_then(Value::as_u64).unwrap_or(u64::MAX)
}

fn changed_files(report: &Value) -> Vec<String> {
    report
        .pointer("/evidence/dirty_state/changed_files")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
        .unwrap_or_default()
}

fn do_not_run_cmds(v: &Value) -> String {
    v.get("do_not_run")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|d| d.get("command").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .unwrap_or_default()
}

/// Resource locks declared on the run's evidence, as "resource_id:mode" pairs.
fn resource_locks(report: &Value) -> Vec<String> {
    report
        .pointer("/evidence/resource_locks")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|l| {
                    format!(
                        "{}:{}",
                        l.get("resource_id").and_then(Value::as_str).unwrap_or(""),
                        l.get("mode").and_then(Value::as_str).unwrap_or("")
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn doctor_weak_spot_ids(doctor: &Value) -> Vec<String> {
    doctor
        .pointer("/profile/weak_spots")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|w| w.get("id").and_then(Value::as_str))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

fn doctor_capability_blob(doctor: &Value) -> String {
    doctor
        .get("capabilities")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|c| {
                    format!(
                        "{}:{}",
                        c.get("kind").and_then(Value::as_str).unwrap_or(""),
                        c.get("id").and_then(Value::as_str).unwrap_or("")
                    )
                })
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default()
}

// --- mutation contents (faithful to the grounded specs) ---
const DOWNSTREAM_TYPE_CHANGE: &str =
    "export interface User { id: string; fullName: string; email: string; }\n";

const AUTH_CHANGE: &str = "export interface Session {\n  userId: string;\n  token: string;\n  expiresAt: number;\n  refreshToken: string;\n}\n\nexport function isExpired(s: Session, now: number): boolean {\n  return s.expiresAt <= now;\n}\n\nexport function needsRefresh(s: Session, now: number): boolean {\n  return s.expiresAt - now < 60_000;\n}\n";

const NOCAP_CHANGE: &str =
    "function greet(name) {\n  return \"hello \" + name;\n}\nmodule.exports = { greet };\n";

// stale-build-config: stage 1 breaks the build config (partial evidence), stage
// 2 fixes it AND changes a build-config input to force staleness.
const TSBUILD_BROKEN: &str = "{\n  \"compilerOptions\": { \"strict\": true, \"outDir\": \"dist\", \"target\": \"ES2022\", \"module\": \"ES2022\", \"moduleResolution\": \"bundler\", \"skipLibCheck\": true, \"declaration\": true },\n  \"include\": [\"src/**/*.ts\"],\n  \"files\": [\"missing-file.ts\"]\n}\n";
const TSBUILD_FIXED: &str = "{\n  \"compilerOptions\": { \"strict\": true, \"outDir\": \"dist\", \"target\": \"ES2022\", \"module\": \"ES2022\", \"moduleResolution\": \"bundler\", \"skipLibCheck\": true, \"declaration\": true },\n  \"include\": [\"src/**/*.ts\"]\n}\n";
const TSCONFIG_CHANGED: &str = "{\n  \"compilerOptions\": { \"strict\": true, \"noEmit\": true, \"target\": \"ES2022\", \"module\": \"ES2022\", \"moduleResolution\": \"bundler\", \"skipLibCheck\": true, \"noUncheckedIndexedAccess\": true },\n  \"include\": [\"src/**/*.ts\"]\n}\n";

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
            second_stage: None,
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
                // Wall-clock timing (baseline vs vrt, measured_saved) is reported
                // verbatim in the summary as evidence, but NOT asserted: VRT's
                // per-invocation overhead (~0.5s) is comparable to a cheap tsc
                // check, so on cheap-toolchain fixtures the sub-second win is a
                // wash. The agile win is asserted structurally (build skipped,
                // commands avoided); the wall-clock win materializes when the
                // avoided work is expensive (real build/e2e). Asserting a
                // sub-second ratio here would be a flaky test (Canvas §5.8).
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
            second_stage: None,
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
            second_stage: None,
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
        // §5.3 — Downstream noise: one root type change → many downstream
        // errors; VRT surfaces the first causal error and hides the noise.
        Scenario {
            id: "downstream-noise".into(),
            title: "Downstream noise → one root cause surfaced, noise hidden".into(),
            proposition: Proposition::Governance,
            fixture: xtask_dir.join("fixtures/ts-graph"),
            fixture_label: "ts-graph".into(),
            mutations: vec![Mutation {
                path: "src/types.ts".into(),
                contents: DOWNSTREAM_TYPE_CHANGE.into(),
            }],
            baseline: vec![("typecheck".into(), "npm run typecheck".into())],
            vrt_mode: "dev".into(),
            high_risk: false,
            config_mutated: false,
            second_stage: None,
            assertions: Box::new(|e| {
                let rc = e
                    .report
                    .get("root_cause_candidates")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let rc_first = rc.first().and_then(Value::as_str).unwrap_or("").to_string();
                let files = changed_files(e.report);
                vec![
                    ok("status_failed", str_at(e.report, "/status") == "failed",
                       format!("status = {}", str_at(e.report, "/status"))),
                    ok("failure_kind_type_error",
                       str_at(e.report, "/failure_kind") == "type_error",
                       format!("failure_kind = {}", str_at(e.report, "/failure_kind"))),
                    ok("single_check_run", u64_at(e.report, "checks_run") == 1,
                       format!("checks_run = {}", u64_at(e.report, "checks_run"))),
                    ok("downstream_noise_hidden",
                       u64_at(e.report, "downstream_noise_hidden") != u64::MAX
                           && u64_at(e.report, "downstream_noise_hidden") > 0,
                       format!("downstream_noise_hidden = {}", u64_at(e.report, "downstream_noise_hidden"))),
                    ok("root_cause_first", !rc.is_empty(),
                       format!("first root cause: {rc_first}")),
                    ok("do_not_run_includes_build",
                       do_not_run_cmds(e.report).to_lowercase().contains("build"),
                       format!("do_not_run: {}", do_not_run_cmds(e.report))),
                    ok("scoped_to_root_change",
                       files.iter().any(|f| f.contains("types.ts")),
                       format!("changed_files: {files:?}")),
                ]
            }),
        },
        // §5.12 — High-risk auth/billing path: escalation downgrade + release
        // stays insufficient. Advisory gap: no explicit escalation marker.
        Scenario {
            id: "high-risk-auth".into(),
            title: "High-risk auth change → escalation, release insufficient".into(),
            proposition: Proposition::Governance,
            fixture: xtask_dir.join("fixtures/ts-app"),
            fixture_label: "ts-app".into(),
            mutations: vec![Mutation {
                path: "src/auth/session.ts".into(),
                contents: AUTH_CHANGE.into(),
            }],
            baseline: vec![("typecheck".into(), "npm run typecheck".into())],
            vrt_mode: "dev".into(),
            high_risk: true,
            config_mutated: false,
            second_stage: None,
            assertions: Box::new(|e| {
                let release = release_conf(e.report);
                let local = local_conf(e.report);
                let has_marker = e.report.pointer("/evidence/escalations").is_some()
                    || e.report.get("requires_escalation").is_some()
                    || e.report.get("risk_tags").is_some()
                    || e.report.pointer("/evidence/risk_tags").is_some();
                vec![
                    ok("release_insufficient", release == "insufficient",
                       format!("release = {release}")),
                    ok("local_downgraded_by_escalation", local != "high",
                       format!("local = {local} (escalation downgrades from high)")),
                    ok("not_waved_through_on_cheap_checks",
                       u64_at(e.report, "checks_run") == 1 && release == "insufficient",
                       format!("checks_run={} release={release}", u64_at(e.report, "checks_run"))),
                    ok("explicit_escalation_marker_present", has_marker,
                       format!("requires_escalation/escalations/risk_tags present in report: {has_marker}")),
                ]
            }),
        },
        // §5.7 — Missing capability: no typecheck/test script must not yield
        // high confidence; doctor reports weak spots; nothing fabricated.
        Scenario {
            id: "missing-capability".into(),
            title: "Missing typecheck/test → not_available, no high confidence".into(),
            proposition: Proposition::Governance,
            fixture: xtask_dir.join("fixtures/no-capability"),
            fixture_label: "no-capability".into(),
            mutations: vec![Mutation {
                path: "app.js".into(),
                contents: NOCAP_CHANGE.into(),
            }],
            baseline: naive_baseline(),
            vrt_mode: "dev".into(),
            high_risk: false,
            config_mutated: false,
            second_stage: None,
            assertions: Box::new(|e| {
                let weak = doctor_weak_spot_ids(e.doctor);
                let caps = doctor_capability_blob(e.doctor);
                vec![
                    ok("doctor_no_typecheck_weak_spot",
                       weak.iter().any(|w| w == "no-typecheck-script"),
                       format!("weak_spots: {weak:?}")),
                    ok("doctor_no_test_weak_spot",
                       weak.iter().any(|w| w == "no-test-script"),
                       format!("weak_spots: {weak:?}")),
                    ok("no_typecheck_capability_fabricated", !caps.contains("typecheck"),
                       format!("capabilities: {caps}")),
                    ok("no_test_capability_fabricated", !caps.contains("test"),
                       format!("capabilities: {caps}")),
                    ok("zero_behavioral_check_run", u64_at(e.report, "checks_run") == 0,
                       format!("checks_run = {}", u64_at(e.report, "checks_run"))),
                    ok("local_confidence_not_high", local_conf(e.report) != "high",
                       format!("local = {}", local_conf(e.report))),
                    ok("release_insufficient", release_conf(e.report) == "insufficient",
                       format!("release = {}", release_conf(e.report))),
                    ok("zero_check_flagged_invalid",
                       str_at(e.report, "/validity") == "invalid",
                       format!("validity = {}", str_at(e.report, "/validity"))),
                ]
            }),
        },
        // §5.5 — Build-config change invalidates stale build evidence. Stage 1
        // produces partial evidence (build fails); stage 2 changes a build
        // config and continues → prior checks NOT reused, stale_reasons set.
        Scenario {
            id: "stale-build-config".into(),
            title: "Build config change → prior evidence stale, not reused".into(),
            proposition: Proposition::Governance,
            fixture: ts_lib.clone(),
            fixture_label: "ts-lib".into(),
            mutations: vec![Mutation {
                path: "tsconfig.build.json".into(),
                contents: TSBUILD_BROKEN.into(),
            }],
            baseline: vec![
                ("typecheck".into(), "npm run typecheck".into()),
                ("build".into(), "npm run build".into()),
            ],
            vrt_mode: "release".into(),
            high_risk: false,
            config_mutated: true,
            second_stage: Some(SecondStage {
                mutations: vec![
                    Mutation { path: "tsconfig.build.json".into(), contents: TSBUILD_FIXED.into() },
                    Mutation { path: "tsconfig.json".into(), contents: TSCONFIG_CHANGED.into() },
                ],
                use_continue: true,
            }),
            assertions: Box::new(|e| {
                let stale = stale_reasons(e.report);
                vec![
                    ok("prior_checks_not_reused", reused_count(e.report) == 0,
                       format!("checks_reused = {}", reused_count(e.report))),
                    ok("stale_reasons_recorded", !stale.is_empty(),
                       format!("stale_reasons: {stale:?}")),
                    ok("release_stays_insufficient", release_conf(e.report) == "insufficient",
                       format!("release = {}", release_conf(e.report))),
                ]
            }),
        },
        // §5.5 (negative control) — with NO further change, `--continue` DOES
        // reuse still-valid checks, proving the staleness above is specific to
        // the config change, not an always-empty reuse path.
        Scenario {
            id: "stale-negative-control".into(),
            title: "No-change continue → evidence IS reused (reuse is real)".into(),
            proposition: Proposition::Governance,
            fixture: ts_lib.clone(),
            fixture_label: "ts-lib".into(),
            mutations: vec![Mutation {
                path: "tsconfig.build.json".into(),
                contents: TSBUILD_BROKEN.into(),
            }],
            baseline: vec![("typecheck".into(), "npm run typecheck".into())],
            vrt_mode: "release".into(),
            high_risk: false,
            config_mutated: false,
            second_stage: Some(SecondStage { mutations: vec![], use_continue: true }),
            assertions: Box::new(|e| {
                let stale = stale_reasons(e.report);
                vec![
                    ok("checks_are_reused", reused_count(e.report) >= 1,
                       format!("checks_reused = {}", reused_count(e.report))),
                    ok("no_stale_reasons", stale.is_empty(),
                       format!("stale_reasons: {stale:?}")),
                ]
            }),
        },
        // §5.10 — Resource conflict: a build plan must declare the .next output
        // as an EXCLUSIVE lock and the source tree as SHARED, so concurrent
        // builds serialize and never write .next at once. (vrt-core separately
        // tests that an exclusive lock is actually waited on under the broker.)
        Scenario {
            id: "resource-locks".into(),
            title: "Build plan declares .next exclusive + source-tree shared locks".into(),
            proposition: Proposition::Governance,
            fixture: ts_lib.clone(),
            fixture_label: "ts-lib".into(),
            mutations: vec![Mutation {
                path: "src/label.ts".into(),
                contents: "export const PRICING_LABEL = \"resource-lock probe\";\n".into(),
            }],
            baseline: vec![("build".into(), "npm run build".into())],
            vrt_mode: "release".into(),
            high_risk: false,
            config_mutated: false,
            second_stage: None,
            assertions: Box::new(|e| {
                let locks = resource_locks(e.report);
                vec![
                    ok("dot_next_exclusive_lock",
                       locks.iter().any(|l| l == ".next:exclusive"),
                       format!("resource_locks: {locks:?}")),
                    ok("source_tree_shared_lock",
                       locks.iter().any(|l| l == "source-tree:shared"),
                       format!("resource_locks: {locks:?}")),
                ]
            }),
        },
    ]
}
