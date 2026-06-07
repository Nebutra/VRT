use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

use tempfile::TempDir;
use vrt_core::{
    analyze_change, build_capability_graph, close_session_context, explain_evidence,
    initialize_project, install_skill, install_token_rules, list_session_contexts,
    plan_verification, record_false_confidence_case, render_agent_report, render_junit,
    render_markdown_report, render_otel_trace, render_sarif, render_token_report,
    resolve_verification_mode, run_verification, run_verification_brokered,
    run_verification_continue, show_session_context, start_worktree_session,
    token_compatibility_manifest, token_rules_markdown, Detection, PackageManager, ReportFormat,
    RiskTag, TokenProfile, VerificationMode,
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

    assert_eq!(profile.schema_version, 1);
    assert_eq!(profile.package_manager, PackageManager::Pnpm);
    assert!(profile.frameworks.contains(&Detection::NextJs));
    assert!(profile.languages.contains(&Detection::TypeScript));
    assert!(profile.tools.contains(&Detection::Vitest));
    let profile_json =
        fs::read_to_string(dir.path().join(".vrt/profile.json")).expect("profile json");
    let profile_value: serde_json::Value =
        serde_json::from_str(&profile_json).expect("profile value");
    assert_eq!(profile_value["schema_version"], 1);
    let config = fs::read_to_string(dir.path().join(".vrt/config.toml")).expect("config");
    assert!(config.contains("schema_version = 1"));
}

#[test]
fn profiler_reports_missing_env_validation_as_release_weak_spot() {
    let dir = fixture();

    let profile = initialize_project(dir.path()).expect("init project");

    assert!(profile.weak_spots.iter().any(|spot| {
        spot.id == "no-env-validation"
            && spot.message.contains("environment")
            && spot.message.contains("release")
    }));
}

#[test]
fn profiler_reports_missing_test_script_as_behavior_weak_spot() {
    let dir = fixture();
    fs::write(
        dir.path().join("package.json"),
        r#"{
  "scripts": {
    "typecheck": "tsc --noEmit",
    "build": "next build"
  },
  "dependencies": {
    "next": "16.0.0"
  },
  "devDependencies": {
    "typescript": "5.9.0"
  }
}"#,
    )
    .expect("package json");

    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");

    assert!(profile
        .weak_spots
        .iter()
        .any(|spot| { spot.id == "no-test-script" && spot.message.contains("behavior") }));
    assert!(!graph.capabilities.iter().any(|cap| cap.kind == "unit_test"));
}

#[test]
fn profiler_reports_missing_migration_safety_for_database_projects() {
    let dir = fixture();
    fs::create_dir_all(dir.path().join("prisma")).expect("prisma dir");
    fs::write(
        dir.path().join("prisma/schema.prisma"),
        r#"datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}

generator client {
  provider = "prisma-client-js"
}

model User {
  id String @id
}
"#,
    )
    .expect("schema");

    let profile = initialize_project(dir.path()).expect("init project");

    assert!(profile.tools.contains(&Detection::Prisma));
    assert!(profile.weak_spots.iter().any(|spot| {
        spot.id == "no-migration-safety-check"
            && spot.message.contains("migration")
            && spot.message.contains("release")
    }));
}

#[test]
fn profiler_warns_about_destructive_package_scripts() {
    let dir = fixture();
    fs::write(
        dir.path().join("package.json"),
        r#"{
  "scripts": {
    "typecheck": "tsc --noEmit",
    "test": "vitest run",
    "build": "next build",
    "reset:db": "prisma db push --force-reset"
  },
  "dependencies": {
    "next": "16.0.0",
    "@prisma/client": "6.19.0"
  },
  "devDependencies": {
    "typescript": "5.9.0",
    "vitest": "4.0.0",
    "prisma": "6.19.0"
  }
}"#,
    )
    .expect("package json");

    let profile = initialize_project(dir.path()).expect("init project");
    let destructive = profile
        .weak_spots
        .iter()
        .find(|spot| spot.id == "destructive-script")
        .expect("destructive weak spot");

    assert!(destructive.message.contains("reset:db"));
    assert!(destructive
        .message
        .to_ascii_lowercase()
        .contains("destructive"));
    assert!(destructive
        .suggestion
        .contains("Run destructive scripts manually"));
}

#[test]
fn profiler_detects_drizzle_config_file_without_dependency_metadata() {
    let dir = fixture();
    fs::write(
        dir.path().join("drizzle.config.ts"),
        "export default { schema: './src/db/schema.ts' }\n",
    )
    .expect("drizzle config");

    let profile = initialize_project(dir.path()).expect("init project");

    assert!(profile.tools.contains(&Detection::Drizzle));
}

#[test]
fn drizzle_config_changes_are_database_schema_risk_and_require_escalation() {
    let dir = fixture();
    fs::write(
        dir.path().join("drizzle.config.ts"),
        "export default { schema: './src/db/schema.ts' }\n",
    )
    .expect("drizzle config");
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .expect("git add drizzle");
    Command::new("git")
        .args(["commit", "-m", "add drizzle config"])
        .current_dir(dir.path())
        .output()
        .expect("git commit drizzle");
    fs::write(
        dir.path().join("drizzle.config.ts"),
        "export default { schema: './src/db/schema.ts', out: './drizzle' }\n",
    )
    .expect("changed drizzle config");
    let profile = initialize_project(dir.path()).expect("init project");

    let change = analyze_change(dir.path(), &profile).expect("change");

    assert!(change.risk_tags.contains(&RiskTag::DatabaseSchema));
    assert!(change.risk_tags.contains(&RiskTag::Migration));
    assert!(change.requires_escalation);
}

#[test]
fn profiler_does_not_report_env_or_migration_weak_spots_when_scripts_exist() {
    let dir = fixture();
    fs::write(
        dir.path().join("package.json"),
        r#"{
  "scripts": {
    "typecheck": "tsc --noEmit",
    "test": "vitest run",
    "build": "next build",
    "env:check": "tsx scripts/validate-env.ts",
    "migration:check": "prisma migrate diff --from-empty --to-schema-datamodel prisma/schema.prisma"
  },
  "dependencies": {
    "next": "16.0.0"
  },
  "devDependencies": {
    "typescript": "5.9.0",
    "vitest": "4.0.0",
    "prisma": "7.0.0"
  }
}"#,
    )
    .expect("package json");
    fs::create_dir_all(dir.path().join("prisma")).expect("prisma dir");
    fs::write(
        dir.path().join("prisma/schema.prisma"),
        r#"datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}

generator client {
  provider = "prisma-client-js"
}

model User {
  id String @id
}
"#,
    )
    .expect("schema");

    let profile = initialize_project(dir.path()).expect("init project");

    assert!(!profile
        .weak_spots
        .iter()
        .any(|spot| spot.id == "no-env-validation"));
    assert!(!profile
        .weak_spots
        .iter()
        .any(|spot| spot.id == "no-migration-safety-check"));
}

#[test]
fn env_validation_script_becomes_release_capability_for_env_changes() {
    let dir = fixture();
    fs::write(
        dir.path().join("package.json"),
        r#"{
  "scripts": {
    "typecheck": "tsc --noEmit",
    "test": "vitest run",
    "build": "next build",
    "env:check": "tsx scripts/validate-env.ts"
  },
  "dependencies": {
    "next": "16.0.0"
  },
  "devDependencies": {
    "typescript": "5.9.0",
    "vitest": "4.0.0",
    "tsx": "4.20.0"
  }
}"#,
    )
    .expect("package json");
    fs::write(
        dir.path().join(".env.example"),
        "DATABASE_URL=postgres://example\n",
    )
    .expect("env baseline");
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .expect("git add env baseline");
    Command::new("git")
        .args(["commit", "-m", "add env validation"])
        .current_dir(dir.path())
        .output()
        .expect("git commit env baseline");
    fs::write(
        dir.path().join(".env.example"),
        "DATABASE_URL=postgres://example\nSTRIPE_SECRET_KEY=sk_test\n",
    )
    .expect("env change");

    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let dev_plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("dev plan");
    let release_plan = plan_verification(&profile, &graph, &change, VerificationMode::Release)
        .expect("release plan");

    let env_validate = graph
        .capabilities
        .iter()
        .find(|cap| cap.kind == "env_validate")
        .expect("env validation capability");
    assert_eq!(env_validate.id, "workspace-env-validate");
    assert_eq!(env_validate.command, "pnpm env:check");
    assert!(env_validate
        .proves
        .iter()
        .any(|proof| proof.contains("Environment")));
    assert!(change.risk_tags.contains(&RiskTag::Env));
    assert!(change.requires_escalation);
    assert!(dev_plan
        .steps
        .iter()
        .any(|step| step.capability_id == "workspace-env-validate"));
    assert!(release_plan
        .steps
        .iter()
        .any(|step| step.capability_id == "workspace-env-validate"));
}

#[test]
fn migration_safety_script_becomes_release_capability_for_migration_changes() {
    let dir = fixture();
    fs::write(
        dir.path().join("package.json"),
        r#"{
  "scripts": {
    "typecheck": "tsc --noEmit",
    "test": "vitest run",
    "build": "next build",
    "migration:check": "prisma migrate diff --from-empty --to-schema-datamodel prisma/schema.prisma"
  },
  "dependencies": {
    "next": "16.0.0"
  },
  "devDependencies": {
    "typescript": "5.9.0",
    "vitest": "4.0.0",
    "prisma": "7.0.0"
  }
}"#,
    )
    .expect("package json");
    fs::create_dir_all(dir.path().join("prisma")).expect("prisma dir");
    fs::write(
        dir.path().join("prisma/schema.prisma"),
        r#"datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}

generator client {
  provider = "prisma-client-js"
}

model User {
  id String @id
}
"#,
    )
    .expect("schema baseline");
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .expect("git add migration baseline");
    Command::new("git")
        .args(["commit", "-m", "add migration safety"])
        .current_dir(dir.path())
        .output()
        .expect("git commit migration baseline");
    fs::write(
        dir.path().join("prisma/schema.prisma"),
        r#"datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}

generator client {
  provider = "prisma-client-js"
}

model User {
  id    String @id
  email String @unique
}
"#,
    )
    .expect("changed schema");

    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let dev_plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("dev plan");
    let release_plan = plan_verification(&profile, &graph, &change, VerificationMode::Release)
        .expect("release plan");

    let migration_safety = graph
        .capabilities
        .iter()
        .find(|cap| cap.kind == "migration_safety")
        .expect("migration safety capability");
    assert_eq!(migration_safety.id, "workspace-migration-safety");
    assert_eq!(migration_safety.command, "pnpm migration:check");
    assert!(migration_safety
        .proves
        .iter()
        .any(|proof| proof.contains("Migration")));
    assert!(change.risk_tags.contains(&RiskTag::DatabaseSchema));
    assert!(change.requires_escalation);
    assert!(dev_plan
        .steps
        .iter()
        .any(|step| step.capability_id == "workspace-prisma-validate"));
    assert!(dev_plan
        .steps
        .iter()
        .any(|step| step.capability_id == "workspace-migration-safety"));
    assert!(release_plan
        .steps
        .iter()
        .any(|step| step.capability_id == "workspace-migration-safety"));
}

#[test]
fn runner_refuses_destructive_verification_commands_without_executing_them() {
    let dir = fixture();
    fs::create_dir_all(dir.path().join("safety-sentinel")).expect("sentinel dir");
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.steps.truncate(1);
    plan.steps[0].command = "rm -rf safety-sentinel".to_string();

    let evidence = run_verification(dir.path(), &profile, &change, &plan).expect("run");

    assert_eq!(evidence.validity, "partial");
    assert_eq!(evidence.checks[0].status, "failed");
    assert_eq!(evidence.checks[0].exit_code, None);
    assert!(evidence.checks[0]
        .summary
        .to_ascii_lowercase()
        .contains("refused destructive command"));
    assert!(dir.path().join("safety-sentinel").exists());
    let raw_log =
        fs::read_to_string(dir.path().join(&evidence.checks[0].raw_log)).expect("raw safety log");
    assert!(raw_log.contains("[vrt] refused destructive command"));
}

#[test]
fn profiler_detects_github_actions_ci_workflows() {
    let dir = fixture();
    fs::create_dir_all(dir.path().join(".github/workflows")).expect("workflow dir");
    fs::write(
        dir.path().join(".github/workflows/ci.yml"),
        r#"name: CI
on:
  pull_request:
  push:
jobs:
  verify:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        node: [20, 22]
    steps:
      - run: pnpm run typecheck
      - run: pnpm test
      - run: pnpm run build
      - run: pnpm run e2e
"#,
    )
    .expect("workflow");

    let profile = initialize_project(dir.path()).expect("init project");
    let profile_json =
        fs::read_to_string(dir.path().join(".vrt/profile.json")).expect("profile json");
    let profile_value: serde_json::Value =
        serde_json::from_str(&profile_json).expect("profile value");

    let ci = profile_value["ci"].as_array().expect("ci array");
    assert_eq!(ci.len(), 1);
    assert_eq!(ci[0]["provider"], "github-actions");
    assert_eq!(ci[0]["path"], ".github/workflows/ci.yml");
    assert_eq!(ci[0]["name"], "CI");
    assert!(ci[0]["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .any(|command| command == "pnpm run typecheck"));
    assert_eq!(ci[0]["runs_typecheck"], true);
    assert_eq!(ci[0]["runs_test"], true);
    assert_eq!(ci[0]["runs_build"], true);
    assert_eq!(ci[0]["runs_e2e"], true);
    assert_eq!(ci[0]["has_matrix"], true);
    assert!(!profile
        .weak_spots
        .iter()
        .any(|spot| spot.id == "no-ci-config"));
}

#[test]
fn unknown_project_without_package_json_degrades_to_empty_profile_with_weak_spots() {
    let dir = TempDir::new().expect("temp dir");
    fs::write(dir.path().join("README.md"), "# unknown\n").unwrap();
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

    let profile = initialize_project(dir.path()).expect("unknown profile");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");

    assert_eq!(profile.package_manager, PackageManager::Unknown);
    assert_eq!(profile.workspace_kind, "unknown");
    assert!(profile.scripts.is_empty());
    assert!(graph.capabilities.is_empty());
    assert!(profile
        .weak_spots
        .iter()
        .any(|spot| spot.id == "no-package-json"));
    assert!(profile
        .weak_spots
        .iter()
        .any(|spot| spot.id == "no-typecheck-script"));
    assert!(profile
        .weak_spots
        .iter()
        .any(|spot| spot.id == "no-test-script"));
    let weak_spots_json = serde_json::to_value(&profile.weak_spots).expect("weak spots json");
    let weak_spots = weak_spots_json.as_array().expect("weak spots array");
    assert!(weak_spots.iter().any(|spot| {
        spot["id"] == "no-typecheck-script"
            && spot["suggestion"]
                .as_str()
                .expect("typecheck suggestion")
                .contains("typecheck")
    }));
    assert!(weak_spots.iter().any(|spot| {
        spot["id"] == "no-build-script"
            && spot["suggestion"]
                .as_str()
                .expect("build suggestion")
                .contains("build")
    }));
    assert!(profile
        .weak_spots
        .iter()
        .any(|spot| spot.id == "no-ci-config"));
    assert!(dir.path().join(".vrt/profile.json").exists());
    assert!(dir.path().join(".vrt/config.toml").exists());
}

#[test]
fn config_default_mode_is_used_when_no_mode_is_requested() {
    let dir = fixture();
    initialize_project(dir.path()).expect("init project");
    fs::write(
        dir.path().join(".vrt/config.toml"),
        r#"schema_version = 1

[policy]
default_mode = "merge"
"#,
    )
    .expect("write config");

    let configured = resolve_verification_mode(dir.path(), None).expect("configured mode");
    let explicit =
        resolve_verification_mode(dir.path(), Some(VerificationMode::Release)).expect("explicit");

    assert_eq!(configured, VerificationMode::Merge);
    assert_eq!(explicit, VerificationMode::Release);
}

#[test]
fn initialize_project_preserves_existing_policy_config() {
    let dir = fixture();
    initialize_project(dir.path()).expect("init project");
    fs::write(
        dir.path().join(".vrt/config.toml"),
        r#"schema_version = 1

[policy]
default_mode = "release"
"#,
    )
    .expect("write custom config");

    initialize_project(dir.path()).expect("re-init project");
    let configured = resolve_verification_mode(dir.path(), None).expect("configured mode");

    assert_eq!(configured, VerificationMode::Release);
}

#[test]
fn policy_strict_areas_escalate_matching_low_risk_changes() {
    let dir = fixture();
    initialize_project(dir.path()).expect("init project");
    fs::write(
        dir.path().join(".vrt/config.toml"),
        r#"schema_version = 1

[policy]
default_mode = "dev"

[policy.strict]
areas = ["docs"]
"#,
    )
    .expect("write config");
    fs::write(dir.path().join("README.md"), "# baseline\n").expect("baseline readme");
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .expect("git add baseline");
    Command::new("git")
        .args(["commit", "-m", "baseline with strict docs"])
        .current_dir(dir.path())
        .output()
        .expect("git commit baseline");
    fs::write(dir.path().join("README.md"), "# docs change\n").expect("readme change");

    let profile = initialize_project(dir.path()).expect("re-init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let plan = plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");

    assert!(change.risk_tags.contains(&RiskTag::Docs));
    assert!(change.requires_escalation);
    assert!(plan
        .steps
        .iter()
        .any(|step| step.capability_id == "workspace-lint"));
    assert!(plan
        .steps
        .iter()
        .any(|step| step.capability_id == "workspace-build"));
    let plan_json = serde_json::to_value(&plan).expect("plan json");
    assert!(plan_json["steps"]
        .as_array()
        .expect("plan steps")
        .iter()
        .any(|step| step["capability_id"] == "workspace-build"
            && step["safety_level"] == "expensive"));
    assert_eq!(plan.expected_confidence.local, "medium");
}

#[test]
fn policy_relaxed_areas_keep_marketing_ui_changes_in_fast_loop() {
    let dir = fixture();
    initialize_project(dir.path()).expect("init project");
    fs::write(
        dir.path().join(".vrt/config.toml"),
        r#"schema_version = 1

[policy]
default_mode = "dev"

[policy.strict]
areas = ["ui"]

[policy.relaxed]
areas = ["marketing"]
"#,
    )
    .expect("write config");
    fs::create_dir_all(dir.path().join("marketing")).expect("marketing dir");
    fs::write(
        dir.path().join("marketing/landing.tsx"),
        "export function Landing() { return <main>Baseline</main>; }\n",
    )
    .expect("baseline landing");
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .expect("git add baseline");
    Command::new("git")
        .args(["commit", "-m", "baseline marketing"])
        .current_dir(dir.path())
        .output()
        .expect("git commit baseline");
    fs::write(
        dir.path().join("marketing/landing.tsx"),
        "export function Landing() { return <main>Experiment</main>; }\n",
    )
    .expect("landing change");

    let profile = initialize_project(dir.path()).expect("re-init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let plan = plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");

    assert!(change.risk_tags.contains(&RiskTag::Marketing));
    assert!(change.risk_tags.contains(&RiskTag::UiComponent));
    assert!(!change.requires_escalation);
    assert!(plan
        .steps
        .iter()
        .any(|step| step.capability_id == "workspace-test"));
    assert!(!plan
        .steps
        .iter()
        .any(|step| step.capability_id == "workspace-build"));
    assert_eq!(plan.expected_confidence.local, "high");
}

#[test]
fn release_policy_can_disclose_full_build_as_optional_residual_risk() {
    let dir = fixture();
    initialize_project(dir.path()).expect("init project");
    fs::write(
        dir.path().join(".vrt/config.toml"),
        r#"schema_version = 1

[policy]
default_mode = "dev"

[release]
require_full_build = false
require_ci = false
"#,
    )
    .expect("write config");

    let profile = initialize_project(dir.path()).expect("re-init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Release).expect("plan");

    assert!(!plan
        .steps
        .iter()
        .any(|step| step.capability_id == "workspace-build"));
    assert!(plan
        .skipped
        .iter()
        .any(|skip| skip.capability_id == "workspace-build"
            && skip.residual_risk.contains("Production")));
    assert_eq!(plan.expected_confidence.release, "insufficient");
}

#[test]
fn release_policy_requires_external_ci_evidence_when_configured() {
    let dir = fixture();
    initialize_project(dir.path()).expect("init project");

    let profile = initialize_project(dir.path()).expect("re-init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Release).expect("plan");

    assert!(profile.ci.is_empty());
    assert!(plan.escalations.iter().any(|escalation| {
        escalation.level == "ci" && escalation.reason.contains("external CI evidence")
    }));
}

#[test]
fn repository_examples_cover_primary_js_ts_project_shapes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cases = [
        (
            "next-single-app",
            "single",
            Detection::NextJs,
            None,
            "workspace-typecheck",
            "npm run typecheck",
            "safe",
        ),
        (
            "vite-ts-app",
            "single",
            Detection::Vite,
            None,
            "workspace-test",
            "npm run test",
            "safe",
        ),
        (
            "prisma-next-app",
            "single",
            Detection::NextJs,
            Some(Detection::Prisma),
            "workspace-prisma-validate",
            "npx prisma validate",
            "safe",
        ),
        (
            "next-turbo-pnpm",
            "turbo",
            Detection::NextJs,
            Some(Detection::Turborepo),
            "workspace-build",
            "pnpm turbo run build --affected",
            "expensive",
        ),
        (
            "nx-workspace",
            "nx",
            Detection::Vite,
            Some(Detection::Nx),
            "workspace-test",
            "pnpm nx affected -t test",
            "safe",
        ),
    ];

    for (example, workspace_kind, framework, tool, capability_id, command, safety_level) in cases {
        let example_root = root.join("examples").join(example);
        let profile = vrt_core::profile_project(&example_root)
            .unwrap_or_else(|error| panic!("profile {example}: {error}"));
        let graph = build_capability_graph(&example_root, &profile)
            .unwrap_or_else(|error| panic!("graph {example}: {error}"));

        assert_eq!(profile.workspace_kind, workspace_kind, "{example}");
        assert!(
            profile.frameworks.contains(&framework),
            "{example} should detect {framework:?}"
        );
        if let Some(tool) = tool {
            assert!(
                profile.tools.contains(&tool),
                "{example} should detect {tool:?}"
            );
        }
        let capability = graph
            .capabilities
            .iter()
            .find(|cap| cap.id == capability_id && cap.command == command)
            .unwrap_or_else(|| panic!("{example} should expose {capability_id} as {command}"));
        let capability_json = serde_json::to_value(capability).expect("capability json");
        assert_eq!(
            capability_json["safety_level"], safety_level,
            "{example} safety level"
        );
    }
}

#[test]
fn playwright_smoke_script_becomes_browser_smoke_capability() {
    let dir = fixture();
    fs::write(
        dir.path().join("package.json"),
        r#"{
  "scripts": {
    "typecheck": "tsc --noEmit",
    "test": "vitest run",
    "e2e": "playwright test --project=chromium --grep @smoke",
    "build": "next build"
  },
  "dependencies": {
    "next": "16.0.0"
  },
  "devDependencies": {
    "typescript": "5.9.0",
    "vitest": "4.0.0",
    "@playwright/test": "1.55.0"
  }
}"#,
    )
    .expect("package json");
    fs::write(
        dir.path().join("playwright.config.ts"),
        "export default { testDir: './e2e' }\n",
    )
    .expect("playwright config");
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .expect("git add playwright");
    Command::new("git")
        .args(["commit", "-m", "add playwright smoke"])
        .current_dir(dir.path())
        .output()
        .expect("git commit playwright");
    fs::write(
        dir.path().join("apps/web/components/pricing-card.tsx"),
        "export function PricingCard() { return <article /> }\n",
    )
    .expect("component change");

    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let dev_plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("dev plan");
    let merge_plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Merge).expect("merge plan");

    assert!(profile.tools.contains(&Detection::Playwright));
    assert!(!profile
        .weak_spots
        .iter()
        .any(|spot| spot.id == "no-playwright-smoke"));
    let smoke = graph
        .capabilities
        .iter()
        .find(|cap| cap.kind == "browser_smoke")
        .expect("browser smoke capability");
    assert_eq!(smoke.id, "workspace-browser-smoke");
    assert_eq!(smoke.command, "pnpm e2e");
    assert!(smoke
        .proves
        .iter()
        .any(|proof| proof.contains("browser smoke")));
    assert!(dev_plan
        .skipped
        .iter()
        .any(|skip| skip.capability_id == "workspace-browser-smoke"
            && skip.residual_risk.contains("Browser smoke behavior")));
    assert!(merge_plan
        .steps
        .iter()
        .any(|step| step.capability_id == "workspace-browser-smoke"
            && step.stop_on_failure
            && step.timeout_ms == Some(360_000)));
}

#[test]
fn skill_install_generates_rules_for_common_agent_surfaces_idempotently() {
    let dir = fixture();

    install_skill(dir.path()).expect("install skill");
    install_skill(dir.path()).expect("install skill twice");

    let expected = [
        "AGENTS.md",
        "CLAUDE.md",
        "GEMINI.md",
        ".cursor/rules/vrt.md",
        ".windsurf/rules/vrt.md",
        ".codex/skills/vrt/SKILL.md",
        ".vrt/skill/vrt.md",
    ];
    for path in expected {
        let source = fs::read_to_string(dir.path().join(path))
            .unwrap_or_else(|error| panic!("read {path}: {error}"));
        assert!(source.contains("VRT Verification Skill"), "{path}");
        assert!(source.contains("vrt verify --json"), "{path}");
        assert!(source.contains("skipped checks"), "{path}");
    }

    let agents = fs::read_to_string(dir.path().join("AGENTS.md")).expect("agents");
    assert_eq!(agents.matches(".vrt/skill/vrt.md").count(), 1);
    let claude = fs::read_to_string(dir.path().join("CLAUDE.md")).expect("claude");
    assert_eq!(claude.matches(".vrt/skill/vrt.md").count(), 1);
    let gemini = fs::read_to_string(dir.path().join("GEMINI.md")).expect("gemini");
    assert_eq!(gemini.matches(".vrt/skill/vrt.md").count(), 1);
}

#[test]
fn planner_runs_signal_checks_before_build_and_discloses_skipped_risk() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");

    let plan = plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");

    assert!(change.risk_tags.contains(&RiskTag::UiComponent));
    let serialized = serde_json::to_string(&change.risk_tags).expect("risk tags json");
    assert!(serialized.contains("ui_component"));
    assert!(change.affected_nodes.contains(&"apps-web".to_string()));
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
fn build_config_change_is_high_signal_boundary_and_keeps_build_selected() {
    let dir = fixture();
    fs::write(
        dir.path().join("next.config.ts"),
        "export default { reactStrictMode: true }\n",
    )
    .expect("next config change");
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");

    let plan = plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");

    assert!(change.risk_tags.contains(&RiskTag::BuildConfig));
    let serialized = serde_json::to_string(&change.risk_tags).expect("risk tags json");
    assert!(serialized.contains("build_config"));
    assert!(change.global_boundary_changed);
    assert!(plan
        .steps
        .iter()
        .any(|step| step.capability_id == "workspace-build"));
    assert!(!plan
        .skipped
        .iter()
        .any(|skip| skip.capability_id == "workspace-build"));
}

#[test]
fn package_entrypoint_change_is_package_boundary_and_escalates_plan() {
    let dir = fixture();
    fs::create_dir_all(dir.path().join("apps/web")).expect("app dir");
    fs::create_dir_all(dir.path().join("packages/ui/src")).expect("package dir");
    fs::write(
        dir.path().join("apps/web/package.json"),
        r#"{
  "name": "@acme/web",
  "dependencies": {
    "@acme/ui": "workspace:*"
  }
}"#,
    )
    .expect("web package");
    fs::write(
        dir.path().join("packages/ui/package.json"),
        r#"{
  "name": "@acme/ui",
  "exports": {
    ".": "./src/index.ts"
  }
}"#,
    )
    .expect("ui package");
    fs::write(
        dir.path().join("packages/ui/src/index.ts"),
        "export const token = 'initial';\n",
    )
    .expect("initial export");
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .expect("git add package");
    Command::new("git")
        .args(["commit", "-m", "add package"])
        .current_dir(dir.path())
        .output()
        .expect("git commit package");
    fs::write(
        dir.path().join("packages/ui/src/index.ts"),
        "export const token = 'changed';\n",
    )
    .expect("changed export");
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");

    let plan = plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");

    assert!(change.risk_tags.contains(&RiskTag::SharedPackage));
    assert!(change.risk_tags.contains(&RiskTag::PackageBoundary));
    let serialized = serde_json::to_string(&change.risk_tags).expect("risk tags json");
    assert!(serialized.contains("package_boundary"));
    assert!(change.affected_nodes.contains(&"packages-ui".to_string()));
    assert!(change.affected_nodes.contains(&"apps-web".to_string()));
    assert!(change.global_boundary_changed);
    assert!(change.requires_escalation);
    assert!(plan
        .steps
        .iter()
        .any(|step| step.capability_id == "workspace-build"));
    assert!(plan
        .escalations
        .iter()
        .any(|escalation| escalation.level == "merge"));
}

#[test]
fn prisma_schema_change_runs_schema_validation_and_requires_escalation() {
    let dir = fixture();
    fs::write(
        dir.path().join("package.json"),
        r#"{
  "scripts": {
    "typecheck": "tsc --noEmit",
    "lint": "eslint .",
    "test": "vitest run",
    "build": "next build",
    "prisma:generate": "prisma generate"
  },
  "dependencies": {
    "next": "16.0.0"
  },
  "devDependencies": {
    "typescript": "5.9.0",
    "vitest": "4.0.0",
    "prisma": "7.0.0"
  }
}"#,
    )
    .expect("package json");
    fs::create_dir_all(dir.path().join("prisma")).expect("prisma dir");
    fs::write(
        dir.path().join("prisma/schema.prisma"),
        r#"datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}

generator client {
  provider = "prisma-client-js"
}

model User {
  id String @id
}
"#,
    )
    .expect("initial schema");
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .expect("git add prisma");
    Command::new("git")
        .args(["commit", "-m", "add prisma"])
        .current_dir(dir.path())
        .output()
        .expect("git commit prisma");
    fs::write(
        dir.path().join("prisma/schema.prisma"),
        r#"datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}

generator client {
  provider = "prisma-client-js"
}

model User {
  id    String @id
  email String @unique
}
"#,
    )
    .expect("changed schema");
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");

    let plan = plan_verification(&profile, &graph, &change, VerificationMode::Merge).expect("plan");

    assert!(change.risk_tags.contains(&RiskTag::DatabaseSchema));
    let serialized = serde_json::to_string(&change.risk_tags).expect("risk tags json");
    assert!(serialized.contains("database_schema"));
    assert!(change.requires_escalation);
    assert!(plan
        .steps
        .iter()
        .any(|step| step.capability_id == "workspace-prisma-validate"
            && step.command == "pnpm exec prisma validate"));
    assert!(graph
        .capabilities
        .iter()
        .any(|cap| cap.id == "workspace-prisma-generate"
            && cap.kind == "schema_generate"
            && cap.command == "pnpm prisma:generate"));
    assert!(plan
        .steps
        .iter()
        .any(|step| step.capability_id == "workspace-prisma-generate"));
    assert_eq!(plan.expected_confidence.release, "insufficient");
}

#[test]
fn turborepo_capabilities_use_affected_turbo_run_commands() {
    let dir = fixture();
    fs::write(
        dir.path().join("turbo.json"),
        r#"{"tasks":{"typecheck":{},"lint":{},"test":{},"build":{}}}"#,
    )
    .unwrap();

    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");

    assert_eq!(profile.workspace_kind, "turbo");
    assert!(profile.tools.contains(&Detection::Turborepo));
    let typecheck = graph
        .capabilities
        .iter()
        .find(|cap| cap.kind == "typecheck")
        .expect("typecheck capability");
    assert_eq!(typecheck.command, "pnpm turbo run typecheck --affected");
    let build = graph
        .capabilities
        .iter()
        .find(|cap| cap.kind == "build")
        .expect("build capability");
    assert_eq!(build.command, "pnpm turbo run build --affected");
}

#[test]
fn nx_capabilities_use_nx_affected_commands() {
    let dir = fixture();
    fs::write(dir.path().join("nx.json"), r#"{"namedInputs":{}}"#).unwrap();

    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");

    assert_eq!(profile.workspace_kind, "nx");
    assert!(profile.tools.contains(&Detection::Nx));
    let test = graph
        .capabilities
        .iter()
        .find(|cap| cap.kind == "unit_test")
        .expect("test capability");
    assert_eq!(test.command, "pnpm nx affected -t test");
    let lint = graph
        .capabilities
        .iter()
        .find(|cap| cap.kind == "lint")
        .expect("lint capability");
    assert_eq!(lint.command, "pnpm nx affected -t lint");
}

#[test]
fn related_test_script_is_preferred_over_full_test_for_dev_loops() {
    let dir = fixture();
    fs::write(
        dir.path().join("package.json"),
        r#"{
  "scripts": {
    "typecheck": "tsc --noEmit",
    "test": "vitest run",
    "test:related": "vitest related --run",
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
    .expect("package json");

    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let plan = plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");

    let related = graph
        .capabilities
        .iter()
        .find(|cap| cap.kind == "unit_test")
        .expect("related unit test capability");
    assert_eq!(related.id, "workspace-related-test");
    assert_eq!(related.command, "pnpm test:related");
    assert!(related
        .proves
        .iter()
        .any(|proof| proof.contains("Related test behavior")));

    let full = graph
        .capabilities
        .iter()
        .find(|cap| cap.kind == "full_test")
        .expect("full test capability");
    assert_eq!(full.id, "workspace-test");
    assert_eq!(full.command, "pnpm test");

    assert!(plan
        .steps
        .iter()
        .any(|step| step.capability_id == "workspace-related-test"));
    assert!(plan
        .skipped
        .iter()
        .any(|skip| skip.capability_id == "workspace-test"
            && skip.residual_risk.contains("Full test suite")));
}

#[test]
fn biome_check_script_becomes_format_check_capability_for_merge_plans() {
    let dir = fixture();
    fs::write(
        dir.path().join("package.json"),
        r#"{
  "scripts": {
    "typecheck": "tsc --noEmit",
    "test": "vitest run",
    "check:format": "biome check .",
    "build": "next build"
  },
  "dependencies": {
    "next": "16.0.0"
  },
  "devDependencies": {
    "typescript": "5.9.0",
    "vitest": "4.0.0",
    "@biomejs/biome": "2.3.0"
  }
}"#,
    )
    .expect("package json");
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .expect("git add biome");
    Command::new("git")
        .args(["commit", "-m", "add biome check"])
        .current_dir(dir.path())
        .output()
        .expect("git commit biome");
    fs::write(
        dir.path().join("apps/web/components/pricing-card.tsx"),
        "export function PricingCard() { return <article /> }\n",
    )
    .expect("component change");

    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let dev_plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("dev plan");
    let merge_plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Merge).expect("merge plan");

    assert!(profile.tools.contains(&Detection::Biome));
    assert!(graph.capabilities.iter().any(|cap| {
        cap.id == "workspace-format-check"
            && cap.kind == "format_check"
            && cap.command == "pnpm check:format"
            && cap.proves.iter().any(|proof| proof.contains("format"))
    }));
    assert!(dev_plan
        .skipped
        .iter()
        .any(|skip| skip.capability_id == "workspace-format-check"
            && skip.residual_risk.contains("Format")));
    assert!(merge_plan
        .steps
        .iter()
        .any(|step| step.capability_id == "workspace-format-check"
            && step.timeout_ms == Some(120_000)));
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

    assert_eq!(evidence.schema_version, 1);
    let evidence_json = serde_json::to_value(&evidence).expect("evidence json");
    assert_eq!(
        evidence_json["config_hash"]
            .as_str()
            .expect("config hash")
            .len(),
        64
    );
    assert!(evidence_json["toolchain_version"]
        .as_str()
        .expect("toolchain version")
        .starts_with("vrt-core/"));
    assert!(
        evidence_json["relevant_inputs_hash"]
            .as_str()
            .expect("relevant inputs hash")
            .len()
            == 64
    );
    assert!(evidence_json["env_assumptions"]
        .as_array()
        .expect("env assumptions")
        .iter()
        .any(|item| item == "local process environment captured by project-owned commands"));
    assert!(evidence_json["broker_job_id"].is_null());
    assert_eq!(evidence_json["queue_wait_ms"], 0);
    assert_eq!(evidence_json["lock_wait_ms"], 0);
    assert_eq!(evidence_json["singleflight"]["role"], "none");
    assert!(evidence_json["singleflight"]["key"].is_null());
    assert!(evidence_json["singleflight"]["shared_from_evidence_id"].is_null());
    assert!(evidence_json["resource_locks"]
        .as_array()
        .expect("resource locks")
        .iter()
        .any(|lock| lock["resource_id"] == "source-tree"
            && lock["kind"] == "filesystem"
            && lock["mode"] == "shared"));
    assert_eq!(evidence_json["runner_pool"], "cheap");
    assert_eq!(evidence.validity.as_str(), "partial");
    assert_eq!(evidence.checks[0].status.as_str(), "failed");
    assert_eq!(evidence_json["checks"][0]["safety_level"], "safe");
    assert!(evidence.dirty_state.is_dirty);
    assert!(
        evidence
            .dirty_state
            .changed_files
            .iter()
            .any(|file| file.contains("apps/web/components/pricing-card.tsx")),
        "dirty files: {:?}",
        evidence.dirty_state.changed_files
    );
    assert!(dir.path().join(&evidence.checks[0].raw_log).exists());
    assert_eq!(evidence.confidence.release.as_str(), "insufficient");
    assert!(dir.path().join(&evidence.report_path).exists());
}

#[test]
fn agent_report_includes_failure_guidance_and_full_evidence() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.steps.truncate(1);
    plan.steps[0].command = "sh -c 'echo \"src/app.ts:4:12 - error TS2322: Type string is not assignable to number\" >&2; exit 2'".to_string();

    let evidence = run_verification(dir.path(), &profile, &change, &plan).expect("run");
    let report = render_agent_report(dir.path(), &evidence);

    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["status"], "failed");
    assert_eq!(report["evidence_id"], evidence.evidence_id);
    assert_eq!(report["evidence"]["evidence_id"], evidence.evidence_id);
    assert_eq!(report["failure_kind"], "type_error");
    assert!(report["root_cause_candidates"][0]
        .as_str()
        .unwrap()
        .contains("src/app.ts:4:12"));
    assert!(report["recommended_next_action"]
        .as_str()
        .unwrap()
        .contains("verify --continue"));
    assert_eq!(report["do_not_run"][0]["command"], "full build");
    assert_eq!(report["checks_run"], 1);
    assert!(report["checks_skipped"].as_u64().unwrap() >= 1);
    assert_eq!(report["confidence"]["release"], "insufficient");
    assert!(report["raw_log"].as_str().unwrap().contains(".raw.log"));
}

#[test]
fn records_false_confidence_case_against_latest_evidence_and_bench_counts_it() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.steps.truncate(1);
    plan.steps[0].command = "sh -c 'echo typecheck ok'".to_string();

    let evidence = run_verification(dir.path(), &profile, &change, &plan).expect("run");
    let case = record_false_confidence_case(
        dir.path(),
        None,
        "vrt verify --mode release --full",
        "next build failed on dynamic import that dev mode did not cover",
    )
    .expect("record false confidence");
    let cases = vrt_core::list_false_confidence_cases(dir.path()).expect("list cases");
    let bench = vrt_core::bench_summary(dir.path()).expect("bench");

    assert_eq!(case.schema_version, 1);
    assert_eq!(case.evidence_id, evidence.evidence_id);
    assert_eq!(case.previous_confidence.release, "insufficient");
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].diff_hash, evidence.diff_hash);
    assert_eq!(bench["false_confidence_cases"], 1);
    assert_eq!(bench["evidence_records"], 1);
    assert_eq!(bench["false_confidence_rate"], 1.0);
}

#[test]
fn verification_reuses_exact_cached_evidence_without_rerunning_commands() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.steps.truncate(1);
    plan.steps[0].command =
        "sh -c 'count=$(cat cache-count 2>/dev/null || echo 0); expr $count + 1 > cache-count'"
            .to_string();

    let first = run_verification(dir.path(), &profile, &change, &plan).expect("first run");
    let second = run_verification(dir.path(), &profile, &change, &plan).expect("cached run");
    let count = fs::read_to_string(dir.path().join("cache-count")).expect("cache count");

    assert_eq!(count.trim(), "1");
    assert_ne!(second.evidence_id, first.evidence_id);
    assert_eq!(
        second.continued_from.as_deref(),
        Some(first.evidence_id.as_str())
    );
    assert!(second.checks.is_empty());
    assert_eq!(second.reused_checks.len(), first.checks.len());
    assert_eq!(second.reused_checks[0].name, first.checks[0].name);
    assert_eq!(second.validity, "valid");
    assert!(dir.path().join(&second.report_path).exists());
    assert!(dir
        .path()
        .join(".vrt/cache/evidence")
        .read_dir()
        .expect("cache entries")
        .next()
        .is_some());

    let lock_dir = dir.path().join(".vrt/run.lock");
    fs::create_dir_all(&lock_dir).expect("lock dir");
    fs::write(
        lock_dir.join("lock.json"),
        r#"{"session_id":"other-session","plan_id":"different-plan"}"#,
    )
    .expect("lock file");
    let error =
        run_verification(dir.path(), &profile, &change, &plan).expect_err("lock wins over cache");
    fs::remove_dir_all(lock_dir).expect("cleanup test lock");

    assert!(error
        .to_string()
        .contains("verification is already running"));
    let count_after_lock = fs::read_to_string(dir.path().join("cache-count")).expect("cache count");
    assert_eq!(count_after_lock.trim(), "1");
}

#[test]
fn verification_cache_rejects_evidence_with_stale_env_assumptions() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.steps.truncate(1);
    plan.steps[0].command =
        "sh -c 'count=$(cat cache-count 2>/dev/null || echo 0); expr $count + 1 > cache-count'"
            .to_string();

    let first = run_verification(dir.path(), &profile, &change, &plan).expect("first run");
    let first_path = dir.path().join(&first.report_path);
    let mut evidence_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&first_path).expect("first evidence"))
            .expect("parse evidence");
    evidence_json["env_assumptions"] =
        serde_json::json!(["stale env assumption from a different runtime"]);
    fs::write(
        &first_path,
        serde_json::to_string_pretty(&evidence_json).expect("serialize evidence"),
    )
    .expect("mutate evidence");

    let second = run_verification(dir.path(), &profile, &change, &plan).expect("second run");
    let count = fs::read_to_string(dir.path().join("cache-count")).expect("cache count");

    assert_eq!(count.trim(), "2");
    assert_ne!(
        second.continued_from.as_deref(),
        Some(first.evidence_id.as_str())
    );
    assert_eq!(second.checks.len(), 1);
    assert!(second.reused_checks.is_empty());
}

#[test]
fn verification_cache_rejects_evidence_with_stale_relevant_inputs() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.steps.truncate(1);
    plan.steps[0].command =
        "sh -c 'count=$(cat cache-count 2>/dev/null || echo 0); expr $count + 1 > cache-count'"
            .to_string();

    let first = run_verification(dir.path(), &profile, &change, &plan).expect("first run");
    let first_path = dir.path().join(&first.report_path);
    let mut evidence_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&first_path).expect("first evidence"))
            .expect("parse evidence");
    evidence_json["relevant_inputs_hash"] =
        serde_json::json!("0000000000000000000000000000000000000000000000000000000000000000");
    fs::write(
        &first_path,
        serde_json::to_string_pretty(&evidence_json).expect("serialize evidence"),
    )
    .expect("mutate evidence");

    let second = run_verification(dir.path(), &profile, &change, &plan).expect("second run");
    let count = fs::read_to_string(dir.path().join("cache-count")).expect("cache count");

    assert_eq!(count.trim(), "2");
    assert_ne!(
        second.continued_from.as_deref(),
        Some(first.evidence_id.as_str())
    );
    assert_eq!(second.checks.len(), 1);
    assert!(second.reused_checks.is_empty());
}

#[test]
fn bench_reports_cache_hit_reuse_and_saved_time_breakdown() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.steps.truncate(1);
    plan.steps[0].command = "sh -c 'echo typecheck ok'".to_string();
    plan.skipped.push(vrt_core::SkippedCheck {
        capability_id: "workspace-build".to_string(),
        reason: "test skipped build".to_string(),
        residual_risk: "Production build behavior not verified.".to_string(),
    });

    let first = run_verification(dir.path(), &profile, &change, &plan).expect("first run");
    let second = run_verification(dir.path(), &profile, &change, &plan).expect("cached run");
    let bench = vrt_core::bench_summary(dir.path()).expect("bench");

    assert_eq!(bench["evidence_id"], second.evidence_id);
    assert_eq!(bench["evidence_records"], 2);
    assert_eq!(bench["cache_hits"], 1);
    assert_eq!(bench["reruns_avoided"], 1);
    assert_eq!(bench["reused_checks"], first.checks.len());
    assert_eq!(bench["evidence_reuse_rate"], 0.5);
    assert_eq!(bench["cache_hit_rate"], 0.5);
    assert!(
        bench["estimated_saved_time_ms"]
            .as_u64()
            .expect("saved time")
            >= 120_000
    );
    assert!(
        bench["saved_by"]["skipped_expensive_checks_ms"]
            .as_u64()
            .expect("skipped saved")
            >= 120_000
    );
    assert!(
        bench["saved_by"]["evidence_reuse_ms"]
            .as_u64()
            .expect("reuse saved")
            >= 1
    );
    assert!(bench["log_lines_compressed"].as_u64().is_some());
    assert!(
        bench["agent_tokens_saved_estimate"]
            .as_u64()
            .expect("token savings estimate")
            > 0
    );
    assert_eq!(bench["queue_wait_time_ms"], 0);
    assert_eq!(bench["lock_wait_time_ms"], 0);
    assert_eq!(bench["singleflight_hits"], 0);
    assert_eq!(bench["singleflight_saved_time_ms"], 0);
    assert_eq!(bench["resource_conflicts_avoided"], 0);
    assert_eq!(bench["duplicate_commands_avoided"], 0);
    assert_eq!(bench["shared_evidence_count"], 0);
    assert!(bench["runner_pool_utilization"]["cheap"].as_f64().is_some());
    assert!(bench["session_count"].as_u64().expect("session count") >= 1);
}

#[test]
fn bench_reports_ci_failures_shifted_left_from_local_failed_evidence() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.steps.truncate(1);
    plan.steps[0].capability_id = "workspace-typecheck".to_string();
    plan.steps[0].command = "sh -c 'echo apps/web/page.tsx:1: type error >&2; exit 2'".to_string();
    plan.skipped.push(vrt_core::SkippedCheck {
        capability_id: "workspace-build".to_string(),
        reason: "build waits for typecheck".to_string(),
        residual_risk: "CI build would still need proof.".to_string(),
    });

    run_verification(dir.path(), &profile, &change, &plan).expect("failed local proof");
    let bench = vrt_core::bench_summary(dir.path()).expect("bench");

    assert_eq!(bench["early_failures"], 1);
    assert_eq!(bench["ci_failures_shifted_left"], 1);
}

#[test]
fn verification_refuses_to_run_when_worktree_lock_is_active() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.steps.truncate(1);
    plan.steps[0].command = "sh -c 'echo should-not-run > lock-test-ran'".to_string();
    let lock_dir = dir.path().join(".vrt/run.lock");
    fs::create_dir_all(&lock_dir).expect("lock dir");
    fs::write(
        lock_dir.join("lock.json"),
        r#"{"session_id":"existing-session","plan_id":"existing-plan"}"#,
    )
    .expect("lock file");

    let error = run_verification(dir.path(), &profile, &change, &plan)
        .expect_err("active lock should stop verification");

    let message = error.to_string();
    assert!(message.contains("verification is already running"));
    assert!(message.contains("existing-session"));
    assert!(!dir.path().join("lock-test-ran").exists());
}

#[test]
fn verification_joins_same_plan_singleflight_and_reuses_latest_evidence() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.steps.truncate(1);
    plan.steps[0].command = "sh -c 'echo typecheck ok'".to_string();
    let first = run_verification(dir.path(), &profile, &change, &plan).expect("first run");

    let lock_dir = dir.path().join(".vrt/run.lock");
    fs::create_dir_all(&lock_dir).expect("lock dir");
    fs::write(
        lock_dir.join("lock.json"),
        serde_json::json!({
            "session_id": "existing-session",
            "plan_id": plan.plan_id,
            "started_at": "2026-06-07T00:00:00Z",
            "pid": 12345
        })
        .to_string(),
    )
    .expect("lock file");
    plan.steps[0].command = "sh -c 'echo should-not-run > singleflight-ran'".to_string();

    let joined = run_verification(dir.path(), &profile, &change, &plan).expect("joined run");

    fs::remove_dir_all(lock_dir).expect("cleanup test lock");
    assert_ne!(joined.evidence_id, first.evidence_id);
    assert_eq!(
        joined.continued_from.as_deref(),
        Some(first.evidence_id.as_str())
    );
    assert_eq!(joined.plan_id, first.plan_id);
    assert_eq!(joined.singleflight.role, "follower");
    assert_eq!(
        joined.singleflight.shared_from_evidence_id.as_deref(),
        Some(first.evidence_id.as_str())
    );
    assert!(joined
        .singleflight
        .key
        .as_deref()
        .is_some_and(|key| key.contains("workspace-typecheck")));
    assert!(joined.checks.is_empty());
    assert_eq!(joined.reused_checks.len(), first.checks.len());
    assert!(!dir.path().join("singleflight-ran").exists());

    let bench = vrt_core::bench_summary(dir.path()).expect("bench");
    assert_eq!(bench["singleflight_hits"], 1);
    assert_eq!(bench["duplicate_commands_avoided"], 1);
    assert_eq!(bench["shared_evidence_count"], 1);
}

#[test]
fn brokered_verification_records_job_id_and_job_state() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.steps.truncate(1);
    plan.steps[0].command = "sh -c 'echo broker typecheck ok'".to_string();

    let evidence =
        run_verification_brokered(dir.path(), &profile, &change, &plan).expect("brokered run");
    let broker_job_id = evidence.broker_job_id.as_deref().expect("broker job id");
    let job_path = dir
        .path()
        .join(".vrt/broker/jobs")
        .join(format!("{broker_job_id}.json"));
    let job: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(job_path).expect("job json")).expect("parse job");

    assert!(broker_job_id.starts_with("job_"));
    assert_eq!(job["job_id"], broker_job_id);
    assert_eq!(job["session_id"], plan.session_id);
    assert_eq!(job["plan_id"], plan.plan_id);
    assert_eq!(job["status"], "passed");
    assert_eq!(job["evidence_id"], evidence.evidence_id);
    assert!(evidence.queue_wait_ms < 1000);
    assert_eq!(
        job["queue_wait_ms"].as_u64().unwrap() as u128,
        evidence.queue_wait_ms
    );
    assert_eq!(evidence.singleflight.role, "none");
}

#[test]
fn brokered_verification_waits_for_exclusive_resource_lock() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Release).expect("plan");
    plan.steps
        .retain(|step| step.capability_id.contains("build"));
    if plan.steps.is_empty() {
        panic!("expected build step");
    }
    plan.steps[0].command = "sh -c 'echo build ok'".to_string();

    let lock_dir = dir.path().join(".vrt/broker/locks/.next.lock");
    fs::create_dir_all(&lock_dir).expect("preexisting lock");
    fs::write(
        lock_dir.join("lock.json"),
        serde_json::json!({
            "resource_id": ".next",
            "job_id": "job_existing",
            "created_at": chrono::Utc::now()
        })
        .to_string(),
    )
    .expect("lock json");
    let releaser_path = lock_dir.clone();
    let _releaser = thread::spawn(move || {
        thread::sleep(Duration::from_millis(120));
        let _ = fs::remove_dir_all(releaser_path);
    });

    let evidence =
        run_verification_brokered(dir.path(), &profile, &change, &plan).expect("brokered run");

    assert_eq!(evidence.validity, "valid");
    assert!(
        evidence.lock_wait_ms >= 50,
        "waited {}",
        evidence.lock_wait_ms
    );
    assert!(evidence
        .resource_locks
        .iter()
        .any(|lock| lock.resource_id == ".next"
            && lock.mode == "exclusive"
            && lock.waited_ms >= 50));
    assert!(!lock_dir.exists());
}

#[test]
fn brokered_verification_waits_for_runner_pool_slot() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Release).expect("plan");
    plan.steps
        .retain(|step| step.capability_id.contains("build"));
    plan.steps[0].command = "sh -c 'echo build ok'".to_string();

    let slot_dir = dir.path().join(".vrt/broker/pools/expensive/slot-0.lock");
    fs::create_dir_all(&slot_dir).expect("preexisting pool slot");
    fs::write(
        slot_dir.join("slot.json"),
        serde_json::json!({
            "pool": "expensive",
            "slot": 0,
            "job_id": "job_existing",
            "created_at": chrono::Utc::now()
        })
        .to_string(),
    )
    .expect("slot json");
    let releaser_path = slot_dir.clone();
    let _releaser = thread::spawn(move || {
        thread::sleep(Duration::from_millis(120));
        let _ = fs::remove_dir_all(releaser_path);
    });

    let evidence =
        run_verification_brokered(dir.path(), &profile, &change, &plan).expect("brokered run");

    assert_eq!(evidence.validity, "valid");
    assert!(
        evidence.queue_wait_ms >= 50,
        "waited {}",
        evidence.queue_wait_ms
    );
    assert_eq!(evidence.runner_pool, "expensive");
    assert!(!slot_dir.exists());
}

#[test]
fn brokered_verification_cleans_stale_resource_lock() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Release).expect("plan");
    plan.steps
        .retain(|step| step.capability_id.contains("build"));
    plan.steps[0].command = "sh -c 'echo build ok'".to_string();

    let lock_dir = dir.path().join(".vrt/broker/locks/.next.lock");
    fs::create_dir_all(&lock_dir).expect("stale lock");
    fs::write(
        lock_dir.join("lock.json"),
        r#"{"resource_id":".next","job_id":"job_stale","created_at":"2020-01-01T00:00:00Z"}"#,
    )
    .expect("stale lock json");

    let evidence =
        run_verification_brokered(dir.path(), &profile, &change, &plan).expect("brokered run");

    assert_eq!(evidence.validity, "valid");
    assert!(
        evidence.lock_wait_ms < 200,
        "waited {}",
        evidence.lock_wait_ms
    );
    assert!(!lock_dir.exists());
    let locks = vrt_core::lock_list(dir.path());
    assert_eq!(locks["held"], 0);
}

#[test]
fn lock_list_reports_active_broker_resource_locks() {
    let dir = fixture();
    let lock_dir = dir.path().join(".vrt/broker/locks/.next.lock");
    fs::create_dir_all(&lock_dir).expect("active lock");
    fs::write(
        lock_dir.join("lock.json"),
        serde_json::json!({
            "schema_version": 1,
            "resource_id": ".next",
            "kind": "filesystem",
            "mode": "exclusive",
            "reason": "Production builds write framework build output.",
            "job_id": "job_active",
            "created_at": chrono::Utc::now()
        })
        .to_string(),
    )
    .expect("active lock json");

    let locks = vrt_core::lock_list(dir.path());

    assert_eq!(locks["held"], 1);
    assert_eq!(locks["locks"][0]["resource_id"], ".next");
    assert_eq!(locks["locks"][0]["status"], "held");
    assert_eq!(locks["locks"][0]["job_id"], "job_active");
}

#[test]
fn queue_status_reports_active_runner_pool_slots() {
    let dir = fixture();
    let slot_dir = dir.path().join(".vrt/broker/pools/expensive/slot-0.lock");
    fs::create_dir_all(&slot_dir).expect("active pool slot");
    fs::write(
        slot_dir.join("slot.json"),
        serde_json::json!({
            "schema_version": 1,
            "pool": "expensive",
            "slot": 0,
            "job_id": "job_active",
            "created_at": chrono::Utc::now()
        })
        .to_string(),
    )
    .expect("active slot json");

    let queue = vrt_core::queue_status(dir.path());

    assert_eq!(queue["runner_pool"]["expensive"]["limit"], 1);
    assert_eq!(queue["runner_pool"]["expensive"]["running"], 1);
    assert_eq!(queue["runner_pool"]["cheap"]["running"], 0);
}

#[test]
fn starts_git_worktree_session_and_records_metadata_in_both_roots() {
    let dir = fixture();
    let worktree_parent = TempDir::new().expect("worktree parent");
    let worktree_path = worktree_parent.path().join("agent-a");

    let session = start_worktree_session(dir.path(), &worktree_path, Some("vrt-test-agent-a"))
        .expect("start worktree session");
    let sessions = vrt_core::list_worktree_sessions(dir.path()).expect("list sessions");
    let root_status = vrt_core::current_worktree_session(dir.path()).expect("root status");
    let worktree_status =
        vrt_core::current_worktree_session(&worktree_path).expect("worktree status");

    assert_eq!(session.schema_version, 1);
    assert_eq!(session.branch, "vrt-test-agent-a");
    assert_eq!(session.status, "active");
    assert!(worktree_path.join(".git").exists());
    assert!(dir
        .path()
        .join(".vrt/sessions")
        .join(format!("{}.json", session.session_id))
        .exists());
    assert!(worktree_path.join(".vrt/session.json").exists());
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, session.session_id);
    assert_eq!(root_status.session_id, session.session_id);
    assert_eq!(worktree_status.session_id, session.session_id);
    assert!(worktree_status
        .instructions
        .iter()
        .any(|instruction| instruction.contains("VRT_SESSION_ID")));
}

#[test]
fn multi_agent_session_view_summarizes_worktree_evidence() {
    let dir = fixture();
    let worktree_parent = TempDir::new().expect("worktree parent");
    let worktree_path = worktree_parent.path().join("agent-view");
    let session = start_worktree_session(dir.path(), &worktree_path, Some("vrt-test-agent-view"))
        .expect("start worktree session");
    fs::write(
        worktree_path.join("apps/web/components/pricing-card.tsx"),
        "export function PricingCard() { return <article /> }\n",
    )
    .expect("worktree change");
    let profile = initialize_project(&worktree_path).expect("init worktree");
    let graph = build_capability_graph(&worktree_path, &profile).expect("graph");
    let change = analyze_change(&worktree_path, &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.session_id = session.session_id.clone();
    plan.steps.truncate(1);
    plan.steps[0].command = "sh -c 'echo typecheck ok'".to_string();

    let evidence =
        run_verification(&worktree_path, &profile, &change, &plan).expect("run worktree");
    let view = vrt_core::multi_agent_session_view(dir.path()).expect("session view");

    assert_eq!(view.schema_version, 1);
    assert_eq!(view.sessions.len(), 1);
    assert_eq!(view.sessions[0].session.session_id, session.session_id);
    let latest = view.sessions[0]
        .latest_evidence
        .as_ref()
        .expect("latest evidence");
    assert_eq!(latest.evidence_id, evidence.evidence_id);
    assert_eq!(latest.checks_run, 1);
    assert_eq!(latest.checks_failed, 0);
    assert!(latest.checks_skipped >= 1);
    assert_eq!(latest.confidence.release, "insufficient");
    assert!(view.sessions[0].active_lock.is_none());
}

#[test]
fn verify_registers_v02_ephemeral_session_context_and_close_marks_it_closed() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.session_id = "session_v02_ephemeral".to_string();
    plan.steps.truncate(1);
    plan.steps[0].command = "sh -c 'echo typecheck ok'".to_string();

    let evidence = run_verification(dir.path(), &profile, &change, &plan).expect("run");
    let sessions = list_session_contexts(dir.path()).expect("list sessions");
    let session = show_session_context(dir.path(), &plan.session_id).expect("show session");

    assert!(sessions
        .iter()
        .any(|session| session.session_id == plan.session_id));
    assert_eq!(session.schema_version, 1);
    assert_eq!(session.session_id, plan.session_id);
    assert_eq!(session.agent_kind, "unknown");
    assert_eq!(
        session.repo_root,
        dir.path().canonicalize().unwrap().display().to_string()
    );
    assert_eq!(session.working_dir, session.repo_root);
    assert!(!session.worktree.enabled);
    assert!(!session.worktree.managed_by_vrt);
    assert_eq!(session.base_commit, change.base_commit);
    assert_eq!(session.diff_hash, change.diff_hash);
    assert_eq!(session.dirty_state, "dirty");
    assert_eq!(session.status, "active");
    assert_eq!(
        session.last_evidence_id.as_deref(),
        Some(evidence.evidence_id.as_str())
    );

    let closed = close_session_context(dir.path(), &plan.session_id).expect("close session");
    assert_eq!(closed.status, "closed");
    let shown = show_session_context(dir.path(), &plan.session_id).expect("show closed session");
    assert_eq!(shown.status, "closed");
}

#[test]
fn verification_times_out_long_running_steps_and_records_evidence() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.steps.truncate(1);
    plan.steps[0].command = "sleep 2; echo done-marker".to_string();
    plan.steps[0].timeout_ms = Some(50);

    let evidence = run_verification(dir.path(), &profile, &change, &plan).expect("run");

    assert_eq!(evidence.validity, "partial");
    assert_eq!(evidence.checks[0].status, "failed");
    assert_eq!(evidence.checks[0].exit_code, None);
    assert!(evidence.checks[0].summary.contains("timed out after 50ms"));
    let raw_log =
        fs::read_to_string(dir.path().join(&evidence.checks[0].raw_log)).expect("raw timeout log");
    assert!(raw_log.contains("[vrt] timed out after 50ms"));
    assert!(!raw_log.lines().any(|line| line == "done-marker"));
}

#[test]
fn explain_normalizes_typescript_diagnostics_into_actionable_root_cause() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.steps.truncate(1);
    plan.steps[0].command = r#"cat <<'EOF' >&2
apps/web/components/pricing-card.tsx(42,7): error TS2741: Property 'planId' is missing in type '{}' but required in type 'PricingCardProps'.
apps/web/page.tsx(11,3): error TS2322: Type 'string' is not assignable to type 'number'.
Found 2 errors in 2 files.
EOF
exit 2"#
        .to_string();

    let evidence = run_verification(dir.path(), &profile, &change, &plan).expect("run");
    let explanation = explain_evidence(&evidence, dir.path());

    assert_eq!(explanation.failure_kind, "type_error");
    assert_eq!(
        explanation.root_cause_candidates[0],
        "apps/web/components/pricing-card.tsx:42:7 TS2741 Property 'planId' is missing in type '{}' but required in type 'PricingCardProps'."
    );
    assert!(explanation.downstream_noise_hidden > 0);
}

#[test]
fn explain_prioritizes_typescript_diagnostics_over_package_manager_noise() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.steps.truncate(1);
    plan.steps[0].command = r#"cat <<'EOF' >&2
ELIFECYCLE Command failed with exit code 2.
src/app.ts:4:12 - error TS2322: Type string is not assignable to number
EOF
exit 2"#
        .to_string();

    let evidence = run_verification(dir.path(), &profile, &change, &plan).expect("run");
    let explanation = explain_evidence(&evidence, dir.path());

    assert_eq!(
        explanation.root_cause_candidates[0],
        "src/app.ts:4:12 TS2322 Type string is not assignable to number"
    );
}

#[test]
fn explain_combines_vitest_failure_header_with_assertion_message() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.steps.truncate(1);
    plan.steps[0].capability_id = "workspace-test".to_string();
    plan.steps[0].command = r#"cat <<'EOF' >&2
 FAIL  apps/web/components/pricing-card.test.tsx > PricingCard > renders plan id
AssertionError: expected 'Basic' to contain 'plan_123'
 ❯ apps/web/components/pricing-card.test.tsx:17:24
 ❯ runTest node_modules/vitest/dist/index.js:123:4
EOF
exit 1"#
        .to_string();

    let evidence = run_verification(dir.path(), &profile, &change, &plan).expect("run");
    let explanation = explain_evidence(&evidence, dir.path());

    assert_eq!(explanation.failure_kind, "test_failure");
    assert_eq!(
        explanation.root_cause_candidates[0],
        "apps/web/components/pricing-card.test.tsx > PricingCard > renders plan id: AssertionError: expected 'Basic' to contain 'plan_123'"
    );
    assert!(explanation
        .root_cause_candidates
        .iter()
        .any(|candidate| candidate == "apps/web/components/pricing-card.test.tsx:17:24"));
}

#[test]
fn explain_combines_jest_failure_title_with_expectation_and_source_location() {
    let dir = fixture();
    let profile = initialize_project(dir.path()).expect("init project");
    let graph = build_capability_graph(dir.path(), &profile).expect("capability graph");
    let change = analyze_change(dir.path(), &profile).expect("change");
    let mut plan =
        plan_verification(&profile, &graph, &change, VerificationMode::Dev).expect("plan");
    plan.steps.truncate(1);
    plan.steps[0].capability_id = "workspace-test".to_string();
    plan.steps[0].command = r#"cat <<'EOF' >&2
FAIL apps/web/components/pricing-card.test.tsx
  PricingCard
    ✕ renders plan id (12 ms)

  ● PricingCard › renders plan id

    expect(received).toContain(expected)

    Expected substring: "plan_123"
    Received string: "Basic"

      17 | expect(label).toContain('plan_123')
         |                ^

      at Object.<anonymous> (apps/web/components/pricing-card.test.tsx:17:16)
EOF
exit 1"#
        .to_string();

    let evidence = run_verification(dir.path(), &profile, &change, &plan).expect("run");
    let explanation = explain_evidence(&evidence, dir.path());

    assert_eq!(explanation.failure_kind, "test_failure");
    assert_eq!(
        explanation.root_cause_candidates[0],
        "PricingCard > renders plan id: Expected substring: \"plan_123\""
    );
    assert!(explanation
        .root_cause_candidates
        .iter()
        .any(|candidate| candidate == "Received string: \"Basic\""));
    assert!(explanation
        .root_cause_candidates
        .iter()
        .any(|candidate| candidate == "apps/web/components/pricing-card.test.tsx:17:16"));
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
fn continue_does_not_reuse_checks_when_config_changed() {
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
        dir.path().join(".vrt/config.toml"),
        r#"schema_version = 1

[policy]
default_mode = "merge"
"#,
    )
    .expect("change vrt config");
    let second =
        run_verification_continue(dir.path(), &profile, &change, &plan).expect("continue run");

    assert_eq!(
        second.continued_from.as_deref(),
        Some(first.evidence_id.as_str())
    );
    assert!(second.reused_checks.is_empty());
    assert!(second
        .stale_reasons
        .iter()
        .any(|reason| reason.contains("config hash changed")));
    assert_eq!(second.checks.len(), 2);
}

#[test]
fn renders_sarif_junit_and_otel_from_failed_evidence() {
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

    let otel = render_otel_trace(&evidence);
    let spans = otel["resourceSpans"][0]["scopeSpans"][0]["spans"]
        .as_array()
        .expect("otel spans");
    assert!(spans.iter().any(|span| span["name"] == "vrt.verify"));
    assert!(
        spans
            .iter()
            .any(|span| span["name"] == "vrt.check.workspace-typecheck"
                && span["status"]["code"] == 2)
    );
    assert!(spans.iter().any(|span| span["name"]
        .as_str()
        .unwrap()
        .starts_with("vrt.skipped.workspace-build")));
    assert_eq!(
        otel["resourceSpans"][0]["resource"]["attributes"][0]["value"]["stringValue"],
        "vrt"
    );

    let markdown = render_markdown_report(&evidence);
    assert!(markdown.contains("## VRT Verification Report"));
    assert!(markdown.contains(&evidence.evidence_id));
    assert!(markdown.contains("workspace-typecheck"));
    assert!(markdown.contains("safety: safe"));
    assert!(markdown.contains("apps/web/components/pricing-card.tsx:12"));
    assert!(markdown.contains(".raw.log"));
    assert!(markdown.contains("Production bundler behavior not verified"));
    assert!(markdown.contains("release: insufficient"));

    let markdown_path = dir.path().join(".vrt/reports/report.md");
    vrt_core::export_report(dir.path(), ReportFormat::Markdown, &markdown_path)
        .expect("export markdown report");
    let exported = fs::read_to_string(markdown_path).expect("markdown report");
    assert!(exported.contains(&evidence.evidence_id));
}

#[test]
fn token_reports_are_compact_and_preserve_evidence_references() {
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

    let rtk = render_token_report(&evidence, TokenProfile::Rtk);
    assert!(rtk.lines().count() <= 8);
    assert!(rtk.contains("VRT|status=partial"));
    assert!(rtk.contains("evidence="));
    assert!(rtk.contains("raw="));
    assert!(rtk.contains("release=insufficient"));

    let headroom = render_token_report(&evidence, TokenProfile::Headroom);
    let parsed: serde_json::Value = serde_json::from_str(&headroom).expect("headroom json");
    assert_eq!(parsed["tool"], "vrt");
    assert_eq!(parsed["profile"], "headroom");
    assert_eq!(parsed["status"], "partial");
    assert!(parsed["refs"]["evidence"]
        .as_str()
        .unwrap()
        .contains(".vrt/evidence/"));
    assert!(parsed["checks"][0]["raw_log"]
        .as_str()
        .unwrap()
        .contains(".raw.log"));
}

#[test]
fn token_compatibility_manifest_preserves_reversible_evidence_refs() {
    let manifest = token_compatibility_manifest();

    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["tools"]["rtk"]["mode"], "cli-proxy");
    assert_eq!(manifest["tools"]["headroom"]["mode"], "structured-context");
    assert!(manifest["preserve"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item == ".vrt/evidence"));
    assert!(manifest["retrieval"]["mcp_resources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item == "vrt://latest-evidence"));
    assert!(manifest["commands"]["rtk_verify"]
        .as_str()
        .unwrap()
        .contains("--token-profile rtk"));
    assert!(manifest["commands"]["headroom_verify"]
        .as_str()
        .unwrap()
        .contains("--token-profile headroom"));
    assert_eq!(
        manifest["tools"]["rtk"]["agent_setup"]["codex"],
        "rtk init -g --codex"
    );
    assert_eq!(
        manifest["tools"]["headroom"]["agent_setup"]["codex"],
        "headroom wrap codex"
    );
    assert!(manifest["tools"]["headroom"]["entrypoints"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item == "headroom proxy --port 8787"));
    assert!(manifest["tools"]["headroom"]["mcp_tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item == "headroom_retrieve"));
}

#[test]
fn token_rules_preserve_raw_paths_for_rtk_and_headroom() {
    let dir = fixture();

    install_token_rules(dir.path()).expect("install token rules");
    install_token_rules(dir.path()).expect("install token rules twice");

    let expected = [
        ".vrt/token-saving/RTK_HEADROOM.md",
        ".cursor/rules/vrt-token-saving.md",
        ".windsurf/rules/vrt-token-saving.md",
        ".codex/skills/vrt-token-saving/SKILL.md",
    ];
    for path in expected {
        let rules = fs::read_to_string(dir.path().join(path))
            .unwrap_or_else(|error| panic!("read {path}: {error}"));
        assert!(rules.contains("rtk"), "{path}");
        assert!(rules.contains("headroom"), "{path}");
        assert!(rules.contains("raw_log"), "{path}");
        assert!(rules.contains(".vrt/evidence"), "{path}");
    }
    assert!(token_rules_markdown().contains("Do not compress away"));

    for path in ["AGENTS.md", "CLAUDE.md", "GEMINI.md"] {
        let doc = fs::read_to_string(dir.path().join(path))
            .unwrap_or_else(|error| panic!("read {path}: {error}"));
        assert_eq!(doc.matches(".vrt/token-saving/RTK_HEADROOM.md").count(), 1);
    }
}
