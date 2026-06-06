use std::fs;
use std::process::Command;

use tempfile::TempDir;
use vrt_core::{
    analyze_change, build_capability_graph, initialize_project, plan_verification, render_junit,
    render_sarif, run_verification, run_verification_continue, Detection, PackageManager, RiskTag,
    VerificationMode,
};

fn fixture() -> TempDir {
    let dir = TempDir::new().expect("temp dir");
    fs::write(
        dir.path().join("package.json"),
        r#"{
  "scripts": {
    "typecheck": "tsc --noEmit",
    "lint": "eslint .",
    "test": "vitest run",
    "build": "next build"
  },
  "dependencies": {
    "next": "16.0.0"
  },
  "devDependencies": {
    "typescript": "5.9.0",
    "vitest": "4.0.0"
  }
}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\n",
    )
    .unwrap();
    fs::write(dir.path().join("tsconfig.json"), "{}\n").unwrap();
    fs::write(dir.path().join("next.config.ts"), "export default {}\n").unwrap();
    fs::create_dir_all(dir.path().join("apps/web/components")).unwrap();
    fs::write(
        dir.path().join("apps/web/components/pricing-card.tsx"),
        "export function PricingCard() { return <div /> }\n",
    )
    .unwrap();
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.email", "vrt@example.com"])
        .current_dir(dir.path())
        .output()
        .expect("git config email");
    Command::new("git")
        .args(["config", "user.name", "VRT"])
        .current_dir(dir.path())
        .output()
        .expect("git config name");
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(dir.path())
        .output()
        .expect("git commit");
    fs::write(
        dir.path().join("apps/web/components/pricing-card.tsx"),
        "export function PricingCard() { return <section /> }\n",
    )
    .unwrap();
    dir
}

#[test]
fn init_profiles_js_ts_next_project_and_writes_vrt_files() {
    let dir = fixture();

    let profile = initialize_project(dir.path()).expect("init project");

    assert_eq!(profile.package_manager, PackageManager::Pnpm);
    assert!(profile.frameworks.contains(&Detection::NextJs));
    assert!(profile.languages.contains(&Detection::TypeScript));
    assert!(profile.tools.contains(&Detection::Vitest));
    assert!(dir.path().join(".vrt/profile.json").exists());
    assert!(dir.path().join(".vrt/config.toml").exists());
}

#[test]
fn planner_runs_signal_checks_before_build_and_discloses_skipped_risk() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");

    let plan = plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");

    assert!(change.risk_tags.contains(&RiskTag::UiComponent));
    assert!(plan
        .steps
        .iter()
        .any(|step| step.capability_id.contains("typecheck")));
    assert!(plan
        .steps
        .iter()
        .any(|step| step.capability_id.contains("test")));
    assert!(plan
        .skipped
        .iter()
        .any(|skip| skip.capability_id.contains("build")
            && skip.residual_risk.contains("Production")));
    assert_eq!(plan.steps[0].order, 1);
}

#[test]
fn verification_records_partial_evidence_and_raw_logs_on_failure() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.steps.truncate(1);
    plan.steps[0].command = "sh -c 'echo type error at src/app.ts:4 >&2; exit 2'".to_string();

    let evidence = run_verification(dir.path(), &profile, &change, &plan).expect("run");

    assert_eq!(evidence.validity.as_str(), "partial");
    assert_eq!(evidence.checks[0].status.as_str(), "failed");
    assert!(dir.path().join(&evidence.checks[0].raw_log).exists());
    assert_eq!(evidence.confidence.release.as_str(), "insufficient");
    assert!(dir.path().join(&evidence.report_path).exists());
}

#[test]
fn continue_reuses_passed_checks_when_previous_partial_matches_same_diff() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.steps.truncate(2);
    plan.steps[0].command = "sh -c 'echo typecheck ok'".to_string();
    plan.steps[1].command = "sh -c 'echo test failed >&2; exit 1'".to_string();

    let first = run_verification(dir.path(), &profile, &change, &plan).expect("first run");
    let second =
        run_verification_continue(dir.path(), &profile, &change, &plan).expect("continue run");

    assert_eq!(
        second.continued_from.as_deref(),
        Some(first.evidence_id.as_str())
    );
    assert_eq!(second.reused_checks.len(), 1);
    assert_eq!(second.reused_checks[0].name, plan.steps[0].capability_id);
    assert_eq!(second.reused_checks[0].status, "reused");
    assert_eq!(second.checks.len(), 1);
    assert_eq!(second.checks[0].name, plan.steps[1].capability_id);
}

#[test]
fn continue_does_not_reuse_checks_when_diff_changed() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.steps.truncate(2);
    plan.steps[0].command = "sh -c 'echo typecheck ok'".to_string();
    plan.steps[1].command = "sh -c 'echo test failed >&2; exit 1'".to_string();
    let first = run_verification(dir.path(), &profile, &change, &plan).expect("first run");

    fs::write(
        dir.path().join("apps/web/components/pricing-card.tsx"),
        "export function PricingCard() { return <main /> }\n",
    )
    .unwrap();
    let changed = analyze_change(dir.path(), &profile).expect("changed diff");
    let second =
        run_verification_continue(dir.path(), &profile, &changed, &plan).expect("continue run");

    assert_eq!(
        second.continued_from.as_deref(),
        Some(first.evidence_id.as_str())
    );
    assert!(second.reused_checks.is_empty());
    assert!(second
        .stale_reasons
        .iter()
        .any(|reason| reason.contains("diff hash changed")));
    assert_eq!(second.checks.len(), 2);
}

#[test]
fn renders_sarif_and_junit_from_failed_evidence() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.steps.truncate(1);
    plan.steps[0].command =
        "sh -c 'echo apps/web/components/pricing-card.tsx:12: type error >&2; exit 2'".to_string();

    let evidence = run_verification(dir.path(), &profile, &change, &plan).expect("run");

    let sarif = render_sarif(&evidence);
    assert_eq!(sarif["version"], "2.1.0");
    assert_eq!(sarif["runs"][0]["tool"]["driver"]["name"], "VRT");
    assert_eq!(
        sarif["runs"][0]["results"][0]["level"], "error",
        "failed checks should become SARIF errors"
    );
    assert_eq!(
        sarif["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["artifactLocation"]
            ["uri"],
        "apps/web/components/pricing-card.tsx"
    );

    let junit = render_junit(&evidence);
    assert!(junit.contains(r#"<testsuite name="vrt""#));
    assert!(junit.contains(r#"<testcase name="workspace-typecheck""#));
    assert!(junit.contains("<failure"));
    assert!(junit.contains("type error"));
}
