use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageManager {
    Pnpm,
    Npm,
    Yarn,
    Bun,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Detection {
    TypeScript,
    NextJs,
    Vite,
    Turborepo,
    Nx,
    Vitest,
    Jest,
    Playwright,
    Prisma,
    Drizzle,
    Eslint,
    Biome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationMode {
    Dev,
    Merge,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RiskTag {
    Docs,
    Marketing,
    Style,
    UiComponent,
    ApiRoute,
    SharedPackage,
    PackageBoundary,
    DatabaseSchema,
    Migration,
    Auth,
    Billing,
    Env,
    Infra,
    Ci,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectProfile {
    pub id: String,
    pub root: String,
    pub profile_hash: String,
    pub package_manager: PackageManager,
    pub workspace_kind: String,
    pub languages: BTreeSet<Detection>,
    pub frameworks: BTreeSet<Detection>,
    pub tools: BTreeSet<Detection>,
    pub scripts: Vec<PackageScript>,
    pub nodes: Vec<ProjectNode>,
    pub weak_spots: Vec<WeakSpot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageScript {
    pub name: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectNode {
    pub id: String,
    pub name: String,
    pub path: String,
    pub kind: String,
    pub framework: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeakSpot {
    pub id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityGraph {
    pub capabilities: Vec<VerificationCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationCapability {
    pub id: String,
    pub kind: String,
    pub command: String,
    pub cwd: String,
    pub scope: String,
    pub cost: String,
    pub confidence_contribution: String,
    pub proves: Vec<String>,
    pub cannot_prove: Vec<String>,
    pub cacheable: bool,
    pub parallelizable: bool,
    pub side_effects: Vec<String>,
    pub resource_requirements: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeSet {
    pub change_id: String,
    pub base_commit: String,
    pub diff_hash: String,
    pub changed_files: Vec<ChangedFile>,
    pub affected_nodes: Vec<String>,
    pub risk_tags: BTreeSet<RiskTag>,
    pub global_boundary_changed: bool,
    pub requires_escalation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangedFile {
    pub path: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationPlan {
    pub plan_id: String,
    pub mode: VerificationMode,
    pub session_id: String,
    pub change_id: String,
    pub profile_hash: String,
    pub steps: Vec<PlanStep>,
    pub skipped: Vec<SkippedCheck>,
    pub escalations: Vec<EscalationRequirement>,
    pub expected_confidence: ConfidenceReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: String,
    pub capability_id: String,
    pub command: String,
    pub cwd: String,
    pub reason: String,
    pub order: u32,
    pub stop_on_failure: bool,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedCheck {
    pub capability_id: String,
    pub reason: String,
    pub residual_risk: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationRequirement {
    pub level: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRecord {
    pub evidence_id: String,
    pub continued_from: Option<String>,
    pub session_id: String,
    pub plan_id: String,
    pub change_id: String,
    pub base_commit: String,
    pub diff_hash: String,
    pub profile_hash: String,
    pub lockfile_hash: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub duration_ms: u128,
    pub checks: Vec<CheckEvidence>,
    pub reused_checks: Vec<CheckEvidence>,
    pub skipped: Vec<SkippedCheck>,
    pub confidence: ConfidenceReport,
    pub raw_log_dir: String,
    pub report_path: String,
    pub validity: String,
    pub stale_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckEvidence {
    pub name: String,
    pub command: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u128,
    pub raw_log: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceReport {
    pub local: String,
    pub merge: String,
    pub release: String,
    pub summary: String,
    pub residual_risks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureExplanation {
    pub status: String,
    pub failure_kind: String,
    pub root_cause_candidates: Vec<String>,
    pub downstream_noise_hidden: usize,
    pub recommended_next_action: String,
    pub do_not_run: Vec<SkippedCommand>,
    pub raw_log: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedCommand {
    pub command: String,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
struct PackageJson {
    #[serde(default)]
    scripts: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    dependencies: std::collections::BTreeMap<String, serde_json::Value>,
    #[serde(rename = "devDependencies", default)]
    dev_dependencies: std::collections::BTreeMap<String, serde_json::Value>,
    name: Option<String>,
}

pub fn initialize_project(root: &Path) -> Result<ProjectProfile> {
    let profile = profile_project(root)?;
    let vrt_dir = root.join(".vrt");
    fs::create_dir_all(&vrt_dir).with_context(|| format!("create {}", vrt_dir.display()))?;
    write_json(vrt_dir.join("profile.json"), &profile)?;
    let config = r#"[policy]
default_mode = "dev"

[policy.strict]
areas = ["auth", "billing", "database", "env", "infra"]

[release]
require_full_build = true
require_ci = true
"#;
    fs::write(vrt_dir.join("config.toml"), config)?;
    Ok(profile)
}

pub fn profile_project(root: &Path) -> Result<ProjectProfile> {
    let package_json = read_package_json(root)?;
    let package_manager = detect_package_manager(root);
    let mut languages = BTreeSet::new();
    let mut frameworks = BTreeSet::new();
    let mut tools = BTreeSet::new();

    if root.join("tsconfig.json").exists() || has_dep(&package_json, "typescript") {
        languages.insert(Detection::TypeScript);
    }
    if has_dep(&package_json, "next") || has_file(root, "next.config") {
        frameworks.insert(Detection::NextJs);
    }
    if has_dep(&package_json, "vite") || has_file(root, "vite.config") {
        frameworks.insert(Detection::Vite);
    }
    if root.join("turbo.json").exists() || has_dep(&package_json, "turbo") {
        tools.insert(Detection::Turborepo);
    }
    if root.join("nx.json").exists() || has_dep(&package_json, "nx") {
        tools.insert(Detection::Nx);
    }
    if has_dep(&package_json, "vitest") {
        tools.insert(Detection::Vitest);
    }
    if has_dep(&package_json, "jest") {
        tools.insert(Detection::Jest);
    }
    if has_dep(&package_json, "@playwright/test") || has_file(root, "playwright.config") {
        tools.insert(Detection::Playwright);
    }
    if root.join("prisma/schema.prisma").exists() || has_dep(&package_json, "prisma") {
        tools.insert(Detection::Prisma);
    }
    if has_dep(&package_json, "drizzle-kit") {
        tools.insert(Detection::Drizzle);
    }
    if has_dep(&package_json, "eslint") || has_script(&package_json, "lint") {
        tools.insert(Detection::Eslint);
    }
    if has_dep(&package_json, "@biomejs/biome") || has_script(&package_json, "format") {
        tools.insert(Detection::Biome);
    }

    let workspace_kind = if tools.contains(&Detection::Nx) {
        "nx".to_string()
    } else if tools.contains(&Detection::Turborepo) {
        "turbo".to_string()
    } else if root.join("pnpm-workspace.yaml").exists() {
        "pnpm-workspace".to_string()
    } else {
        "single".to_string()
    };

    let scripts = package_json
        .scripts
        .iter()
        .map(|(name, command)| PackageScript {
            name: name.clone(),
            command: command.clone(),
        })
        .collect::<Vec<_>>();

    let nodes = discover_nodes(root, &package_json, &frameworks);
    let mut weak_spots = Vec::new();
    if !tools.contains(&Detection::Playwright) {
        weak_spots.push(WeakSpot {
            id: "no-playwright-smoke".to_string(),
            message: "No Playwright smoke test detected; browser behavior is not locally proven."
                .to_string(),
        });
    }
    if !package_json.scripts.contains_key("build") {
        weak_spots.push(WeakSpot {
            id: "no-build-script".to_string(),
            message: "No build script detected; release confidence requires external proof."
                .to_string(),
        });
    }

    let fingerprint = serde_json::json!({
        "package_manager": package_manager,
        "workspace_kind": workspace_kind,
        "languages": languages,
        "frameworks": frameworks,
        "tools": tools,
        "scripts": scripts,
        "nodes": nodes,
    });
    let profile_hash = hash_string(&serde_json::to_string(&fingerprint)?);

    Ok(ProjectProfile {
        id: profile_hash[..12].to_string(),
        root: root.to_string_lossy().to_string(),
        profile_hash,
        package_manager,
        workspace_kind,
        languages,
        frameworks,
        tools,
        scripts,
        nodes,
        weak_spots,
    })
}

pub fn build_capability_graph(root: &Path, profile: &ProjectProfile) -> Result<CapabilityGraph> {
    let mut capabilities = Vec::new();
    let cwd = root.to_string_lossy().to_string();
    add_script_capability(
        profile,
        &mut capabilities,
        "typecheck",
        "typecheck",
        "medium",
        "high",
        &cwd,
    );
    add_script_capability(
        profile,
        &mut capabilities,
        "lint",
        "lint",
        "cheap",
        "medium",
        &cwd,
    );
    add_script_capability(
        profile,
        &mut capabilities,
        "test",
        "unit_test",
        "medium",
        "high",
        &cwd,
    );
    add_script_capability(
        profile,
        &mut capabilities,
        "build",
        "build",
        "expensive",
        "high",
        &cwd,
    );
    if profile.tools.contains(&Detection::Prisma) {
        capabilities.push(VerificationCapability {
            id: "workspace-prisma-validate".to_string(),
            kind: "schema_validate".to_string(),
            command: package_runner(profile, "prisma validate"),
            cwd,
            scope: "workspace".to_string(),
            cost: "cheap".to_string(),
            confidence_contribution: "high".to_string(),
            proves: vec!["Prisma schema parses successfully".to_string()],
            cannot_prove: vec!["Migration safety against a live database".to_string()],
            cacheable: true,
            parallelizable: true,
            side_effects: vec![],
            resource_requirements: vec![],
        });
    }
    Ok(CapabilityGraph { capabilities })
}

pub fn analyze_change(root: &Path, profile: &ProjectProfile) -> Result<ChangeSet> {
    let changed_files = git_changed_files(root)?;
    let name_status = changed_files
        .iter()
        .map(|f| format!("{}\t{}", f.status, f.path))
        .collect::<Vec<_>>()
        .join("\n");
    let worktree_diff = git_output(root, ["diff", "--binary"]).unwrap_or_default();
    let staged_diff = git_output(root, ["diff", "--cached", "--binary"]).unwrap_or_default();
    let diff_raw = format!("{name_status}\n{worktree_diff}\n{staged_diff}");
    let diff_hash = hash_string(&diff_raw);
    let base_commit = git_output(root, ["rev-parse", "HEAD"]).unwrap_or_else(|_| "unknown".into());
    let mut risk_tags = BTreeSet::new();
    for changed in &changed_files {
        classify_path(&changed.path, &mut risk_tags);
    }
    if risk_tags.is_empty() {
        risk_tags.insert(RiskTag::Unknown);
    }
    let global_boundary_changed = risk_tags.iter().any(|tag| {
        matches!(
            tag,
            RiskTag::PackageBoundary | RiskTag::Env | RiskTag::Infra | RiskTag::Ci
        )
    });
    let requires_escalation = global_boundary_changed
        || risk_tags.iter().any(|tag| {
            matches!(
                tag,
                RiskTag::Auth | RiskTag::Billing | RiskTag::DatabaseSchema | RiskTag::Migration
            )
        });
    let affected_nodes = affected_nodes(profile, &changed_files);
    let change_id = diff_hash[..12].to_string();
    Ok(ChangeSet {
        change_id,
        base_commit: base_commit.trim().to_string(),
        diff_hash,
        changed_files,
        affected_nodes,
        risk_tags,
        global_boundary_changed,
        requires_escalation,
    })
}

pub fn plan_verification(
    profile: &ProjectProfile,
    graph: &CapabilityGraph,
    change: &ChangeSet,
    mode: VerificationMode,
) -> Result<VerificationPlan> {
    let session_id = std::env::var("VRT_SESSION_ID")
        .unwrap_or_else(|_| format!("session_{}", Uuid::new_v4().simple()));
    let mut ordered = Vec::new();
    let mut add = |kind: &str, reason: &str| {
        if let Some(cap) = graph.capabilities.iter().find(|cap| cap.kind == kind) {
            let order = ordered.len() as u32 + 1;
            ordered.push(PlanStep {
                id: format!("step_{order}"),
                capability_id: cap.id.clone(),
                command: cap.command.clone(),
                cwd: cap.cwd.clone(),
                reason: reason.to_string(),
                order,
                stop_on_failure: true,
                timeout_ms: Some(timeout_for_kind(kind)),
            });
        }
    };

    if graph.capabilities.iter().any(|cap| cap.kind == "typecheck") {
        add(
            "typecheck",
            "TypeScript changes need a fast, high-signal type proof first.",
        );
    }
    if change.risk_tags.iter().any(|tag| {
        matches!(
            tag,
            RiskTag::UiComponent | RiskTag::SharedPackage | RiskTag::ApiRoute
        )
    }) {
        add(
            "unit_test",
            "Changed executable JS/TS surface; run package-level tests when available.",
        );
    }
    if change
        .risk_tags
        .iter()
        .any(|tag| matches!(tag, RiskTag::DatabaseSchema | RiskTag::Migration))
    {
        add(
            "schema_validate",
            "Database schema or migration changed; validate schema before build.",
        );
    }
    if matches!(mode, VerificationMode::Merge | VerificationMode::Release)
        || change.requires_escalation
    {
        add(
            "lint",
            "Merge/release or high-risk change needs static hygiene proof.",
        );
    }
    if matches!(mode, VerificationMode::Release)
        || change.global_boundary_changed
        || change.requires_escalation
    {
        add(
            "build",
            "Boundary or release-risk change requires production build proof.",
        );
    }

    let selected = ordered
        .iter()
        .map(|step| step.capability_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut skipped = Vec::new();
    for cap in &graph.capabilities {
        if !selected.contains(cap.id.as_str()) {
            let (reason, residual_risk) = skipped_reason(cap);
            skipped.push(SkippedCheck {
                capability_id: cap.id.clone(),
                reason,
                residual_risk,
            });
        }
    }
    let residual_risks = skipped
        .iter()
        .map(|skip| skip.residual_risk.clone())
        .collect::<Vec<_>>();
    let expected_confidence = confidence_for(mode, &ordered, &skipped, change.requires_escalation);
    let plan_fingerprint = serde_json::json!({
        "mode": mode,
        "change": change.change_id,
        "profile": profile.profile_hash,
        "steps": ordered,
        "skipped": skipped,
        "risks": residual_risks,
    });
    let plan_id = hash_string(&serde_json::to_string(&plan_fingerprint)?)[..12].to_string();
    let mut escalations = Vec::new();
    if change.requires_escalation && !matches!(mode, VerificationMode::Release) {
        escalations.push(EscalationRequirement {
            level: "merge".to_string(),
            reason: "High-risk boundary changed; consider merge-mode verification.".to_string(),
        });
    }

    Ok(VerificationPlan {
        plan_id,
        mode,
        session_id,
        change_id: change.change_id.clone(),
        profile_hash: profile.profile_hash.clone(),
        steps: ordered,
        skipped,
        escalations,
        expected_confidence,
    })
}

pub fn run_verification(
    root: &Path,
    profile: &ProjectProfile,
    change: &ChangeSet,
    plan: &VerificationPlan,
) -> Result<EvidenceRecord> {
    run_verification_internal(root, profile, change, plan, None, vec![], vec![])
}

pub fn run_verification_continue(
    root: &Path,
    profile: &ProjectProfile,
    change: &ChangeSet,
    plan: &VerificationPlan,
) -> Result<EvidenceRecord> {
    let previous = read_latest_evidence(root)?;
    let mut continued_plan = plan.clone();
    let mut reused_checks = Vec::new();
    let mut stale_reasons = Vec::new();
    if previous.validity == "partial"
        && previous.profile_hash == profile.profile_hash
        && previous.base_commit == change.base_commit
        && previous.diff_hash == change.diff_hash
    {
        let passed = previous
            .checks
            .iter()
            .filter(|check| check.status == "passed")
            .map(|check| check.name.as_str())
            .collect::<BTreeSet<_>>();
        reused_checks = previous
            .checks
            .iter()
            .filter(|check| check.status == "passed")
            .cloned()
            .map(|mut check| {
                check.status = "reused".to_string();
                check.summary = format!("Reused from evidence {}", previous.evidence_id);
                check
            })
            .collect();
        continued_plan
            .steps
            .retain(|step| !passed.contains(step.capability_id.as_str()));
    } else {
        if previous.validity != "partial" {
            stale_reasons.push("previous evidence is not partial".to_string());
        }
        if previous.profile_hash != profile.profile_hash {
            stale_reasons.push("profile hash changed; previous checks were not reused".to_string());
        }
        if previous.base_commit != change.base_commit {
            stale_reasons.push("base commit changed; previous checks were not reused".to_string());
        }
        if previous.diff_hash != change.diff_hash {
            stale_reasons.push("diff hash changed; previous checks were not reused".to_string());
        }
    }
    run_verification_internal(
        root,
        profile,
        change,
        &continued_plan,
        Some(previous.evidence_id),
        reused_checks,
        stale_reasons,
    )
}

fn run_verification_internal(
    root: &Path,
    profile: &ProjectProfile,
    change: &ChangeSet,
    plan: &VerificationPlan,
    continued_from: Option<String>,
    reused_checks: Vec<CheckEvidence>,
    stale_reasons: Vec<String>,
) -> Result<EvidenceRecord> {
    let evidence_id = format!("ev_{}", Uuid::new_v4().simple());
    let raw_log_dir = PathBuf::from(".vrt").join("evidence").join(&evidence_id);
    fs::create_dir_all(root.join(&raw_log_dir))?;
    let started_at = Utc::now();
    let started = Instant::now();
    let mut checks = Vec::new();
    let mut failed = false;
    for step in &plan.steps {
        let step_started = Instant::now();
        let output = Command::new("sh")
            .arg("-c")
            .arg(&step.command)
            .current_dir(if step.cwd.is_empty() {
                root
            } else {
                Path::new(&step.cwd)
            })
            .output()
            .with_context(|| format!("run {}", step.command))?;
        let duration_ms = step_started.elapsed().as_millis();
        let mut log = String::new();
        log.push_str("$ ");
        log.push_str(&step.command);
        log.push('\n');
        log.push_str(&String::from_utf8_lossy(&output.stdout));
        log.push_str(&String::from_utf8_lossy(&output.stderr));
        let raw_log = raw_log_dir.join(format!("{}.raw.log", sanitize(&step.id)));
        fs::write(root.join(&raw_log), &log)?;
        let status = if output.status.success() {
            "passed"
        } else {
            failed = true;
            "failed"
        };
        checks.push(CheckEvidence {
            name: step.capability_id.clone(),
            command: step.command.clone(),
            status: status.to_string(),
            exit_code: output.status.code(),
            duration_ms,
            raw_log: raw_log.to_string_lossy().to_string(),
            summary: summarize_log(&log),
        });
        if failed && step.stop_on_failure {
            break;
        }
    }
    let finished_at = Utc::now();
    let validity = if failed {
        "partial"
    } else if checks.is_empty() {
        "invalid"
    } else {
        "valid"
    }
    .to_string();
    let confidence = actual_confidence(plan, &checks, &plan.skipped);
    let report_path = PathBuf::from(".vrt")
        .join("evidence")
        .join(&evidence_id)
        .join("evidence.json");
    let evidence = EvidenceRecord {
        evidence_id,
        continued_from,
        session_id: plan.session_id.clone(),
        plan_id: plan.plan_id.clone(),
        change_id: change.change_id.clone(),
        base_commit: change.base_commit.clone(),
        diff_hash: change.diff_hash.clone(),
        profile_hash: profile.profile_hash.clone(),
        lockfile_hash: lockfile_hash(Path::new(&profile.root)),
        started_at,
        finished_at,
        duration_ms: started.elapsed().as_millis(),
        checks,
        reused_checks,
        skipped: plan.skipped.clone(),
        confidence,
        raw_log_dir: raw_log_dir.to_string_lossy().to_string(),
        report_path: report_path.to_string_lossy().to_string(),
        validity,
        stale_reasons,
    };
    write_json(root.join(&evidence.report_path), &evidence)?;
    write_json(root.join(".vrt/latest.json"), &evidence)?;
    Ok(evidence)
}

pub fn explain_latest(root: &Path) -> Result<FailureExplanation> {
    let evidence: EvidenceRecord =
        serde_json::from_str(&fs::read_to_string(root.join(".vrt/latest.json"))?)?;
    Ok(explain_evidence(&evidence, root))
}

pub fn explain_evidence(evidence: &EvidenceRecord, root: &Path) -> FailureExplanation {
    let failed = evidence
        .checks
        .iter()
        .find(|check| check.status == "failed");
    let raw_log = failed.map(|check| check.raw_log.clone());
    let raw_text = raw_log
        .as_ref()
        .and_then(|path| fs::read_to_string(root.join(path)).ok())
        .unwrap_or_default();
    let candidates = extract_root_causes(&raw_text);
    FailureExplanation {
        status: if failed.is_some() { "failed" } else { "passed" }.to_string(),
        failure_kind: failed
            .map(|check| failure_kind(&check.command, &raw_text))
            .unwrap_or("none")
            .to_string(),
        root_cause_candidates: candidates.clone(),
        downstream_noise_hidden: raw_text.lines().count().saturating_sub(candidates.len().min(5)),
        recommended_next_action: if failed.is_some() {
            "Patch the first root cause candidate, then run `vrt verify --continue`.".to_string()
        } else {
            "No failed check found in latest evidence.".to_string()
        },
        do_not_run: vec![SkippedCommand {
            command: "full build".to_string(),
            reason: "A lower-level verification failed; build has low information gain until it is fixed."
                .to_string(),
        }],
        raw_log,
    }
}

pub fn human_report(evidence: &EvidenceRecord) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Verified in {:.1}s\n\n",
        evidence.duration_ms as f64 / 1000.0
    ));
    out.push_str("Ran:\n");
    if !evidence.reused_checks.is_empty() {
        out.push_str("Reused:\n");
        for check in &evidence.reused_checks {
            out.push_str(&format!("- {}: reused\n", check.name));
        }
    }
    for check in &evidence.checks {
        out.push_str(&format!(
            "- {}: {} ({}ms)\n",
            check.name, check.status, check.duration_ms
        ));
    }
    if !evidence.skipped.is_empty() {
        out.push_str("\nSkipped:\n");
        for skipped in &evidence.skipped {
            out.push_str(&format!(
                "- {}: {}\n  Residual risk: {}\n",
                skipped.capability_id, skipped.reason, skipped.residual_risk
            ));
        }
    }
    out.push_str("\nConfidence:\n");
    out.push_str(&format!(
        "- local: {}\n- merge: {}\n- release: {}\n",
        evidence.confidence.local, evidence.confidence.merge, evidence.confidence.release
    ));
    out
}

pub fn skill_markdown() -> &'static str {
    r#"# VRT Verification Skill

This repository uses VRT as the local verification runtime.

## Required behavior

Before running expensive build, test, lint, or typecheck commands directly, call:

```bash
vrt verify --json
```

If verification fails, call:

```bash
vrt explain --json
```

## Rules

- Do not treat skipped checks as passed checks.
- Preserve residual risk in your final response.
- If VRT requires escalation, run the requested escalation.
- If VRT says release confidence is insufficient, do not claim release readiness.
- Prefer `vrt verify --continue` after patching a failure.
- Use raw logs only when summarized evidence is insufficient.

## Reporting

When reporting to a user, include checks run, checks skipped, confidence level, residual risk, and the next recommended verification step.
"#
}

pub fn install_skill(root: &Path) -> Result<()> {
    let skill_dir = root.join(".vrt/skill");
    fs::create_dir_all(&skill_dir)?;
    fs::write(skill_dir.join("VRT.md"), skill_markdown())?;
    let agents = root.join("AGENTS.md");
    let include = "\n\n# VRT\n\nSee `.vrt/skill/VRT.md` for local verification rules.\n";
    if agents.exists() {
        let existing = fs::read_to_string(&agents)?;
        if !existing.contains(".vrt/skill/VRT.md") {
            fs::write(&agents, format!("{existing}{include}"))?;
        }
    } else {
        fs::write(&agents, format!("# Agent Instructions{include}"))?;
    }
    Ok(())
}

pub fn bench_summary(root: &Path) -> Result<serde_json::Value> {
    let latest: EvidenceRecord =
        serde_json::from_str(&fs::read_to_string(root.join(".vrt/latest.json"))?)?;
    let skipped_builds = latest
        .skipped
        .iter()
        .filter(|skip| skip.capability_id.contains("build"))
        .count();
    Ok(serde_json::json!({
        "evidence_id": latest.evidence_id,
        "verification_time_ms": latest.duration_ms,
        "full_builds_avoided": skipped_builds,
        "confidence": latest.confidence,
        "note": "Saved time is estimated conservatively from skipped expensive checks; skipped is not passed."
    }))
}

pub fn export_report(root: &Path, format: ReportFormat, output: &Path) -> Result<()> {
    let evidence = read_latest_evidence(root)?;
    let rendered = match format {
        ReportFormat::Sarif => serde_json::to_string_pretty(&render_sarif(&evidence))?,
        ReportFormat::Junit => render_junit(&evidence),
    };
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(output, rendered)?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub enum ReportFormat {
    Sarif,
    Junit,
}

pub fn render_sarif(evidence: &EvidenceRecord) -> serde_json::Value {
    let results = evidence
        .checks
        .iter()
        .filter(|check| check.status == "failed")
        .map(|check| {
            let (uri, line) =
                parse_location(&check.summary).unwrap_or_else(|| (check.raw_log.clone(), 1));
            serde_json::json!({
                "ruleId": check.name,
                "level": "error",
                "message": {
                    "text": check.summary
                },
                "locations": [
                    {
                        "physicalLocation": {
                            "artifactLocation": {
                                "uri": uri
                            },
                            "region": {
                                "startLine": line
                            }
                        }
                    }
                ],
                "properties": {
                    "command": check.command,
                    "exitCode": check.exit_code,
                    "rawLog": check.raw_log,
                    "evidenceId": evidence.evidence_id
                }
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [
            {
                "tool": {
                    "driver": {
                        "name": "VRT",
                        "informationUri": "https://github.com/nebutra/vrt",
                        "rules": evidence.checks.iter().map(|check| {
                            serde_json::json!({
                                "id": check.name,
                                "name": check.name,
                                "shortDescription": {
                                    "text": check.command
                                }
                            })
                        }).collect::<Vec<_>>()
                    }
                },
                "results": results,
                "properties": {
                    "evidenceId": evidence.evidence_id,
                    "continuedFrom": evidence.continued_from,
                    "validity": evidence.validity,
                    "diffHash": evidence.diff_hash,
                    "profileHash": evidence.profile_hash
                }
            }
        ]
    })
}

pub fn render_junit(evidence: &EvidenceRecord) -> String {
    let cases = evidence
        .reused_checks
        .iter()
        .chain(evidence.checks.iter())
        .collect::<Vec<_>>();
    let failures = cases
        .iter()
        .filter(|check| check.status == "failed")
        .count();
    let time = evidence.duration_ms as f64 / 1000.0;
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push('\n');
    xml.push_str(&format!(
        r#"<testsuite name="vrt" tests="{}" failures="{}" errors="0" skipped="0" time="{:.3}">"#,
        cases.len(),
        failures,
        time
    ));
    xml.push('\n');
    xml.push_str(&format!(
        r#"  <properties><property name="evidence_id" value="{}"/><property name="validity" value="{}"/></properties>"#,
        escape_xml(&evidence.evidence_id),
        escape_xml(&evidence.validity)
    ));
    xml.push('\n');
    for check in cases {
        xml.push_str(&format!(
            r#"  <testcase name="{}" classname="vrt" time="{:.3}">"#,
            escape_xml(&check.name),
            check.duration_ms as f64 / 1000.0
        ));
        if check.status == "failed" {
            xml.push_str(&format!(
                r#"<failure message="{}">command: {}
raw_log: {}</failure>"#,
                escape_xml(&check.summary),
                escape_xml(&check.command),
                escape_xml(&check.raw_log)
            ));
        }
        xml.push_str("</testcase>\n");
    }
    xml.push_str("</testsuite>\n");
    xml
}

pub fn handle_mcp_message(root: &Path, line: &str) -> Result<Option<String>> {
    let request: serde_json::Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => {
            return Ok(Some(jsonrpc_error(
                serde_json::Value::Null,
                -32700,
                format!("Parse error: {error}"),
            )))
        }
    };
    let id = request.get("id").cloned();
    let method = request
        .get("method")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    if id.is_none() {
        return Ok(None);
    }
    let id = id.unwrap_or(serde_json::Value::Null);
    let response = match method {
        "initialize" => jsonrpc_result(
            id,
            serde_json::json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {
                    "tools": {
                        "listChanged": false
                    }
                },
                "serverInfo": {
                    "name": "vrt",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        ),
        "tools/list" => jsonrpc_result(
            id,
            serde_json::json!({
                "tools": mcp_tools()
            }),
        ),
        "tools/call" => {
            let params = request
                .get("params")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let tool_name = params
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            if !mcp_tool_names().contains(&tool_name) {
                jsonrpc_error(id, -32601, format!("Unknown tool: {tool_name}"))
            } else {
                match call_mcp_tool(root, tool_name, &arguments) {
                    Ok(structured) => jsonrpc_result(id, tool_result(structured, false)),
                    Err(error) => jsonrpc_result(
                        id,
                        tool_result(
                            serde_json::json!({
                                "error": error.to_string()
                            }),
                            true,
                        ),
                    ),
                }
            }
        }
        _ => jsonrpc_error(id, -32601, format!("Unknown method: {method}")),
    };
    Ok(Some(response))
}

fn mcp_tool_names() -> Vec<&'static str> {
    vec![
        "analyze_change",
        "plan_verification",
        "run_verification",
        "explain_failure",
        "get_evidence",
        "escalate_verification",
    ]
}

fn mcp_tools() -> Vec<serde_json::Value> {
    vec![
        mcp_tool(
            "analyze_change",
            "Analyze Change",
            "Profile the project and analyze the current Git diff without running verification commands.",
            serde_json::json!({}),
            true,
        ),
        mcp_tool(
            "plan_verification",
            "Plan Verification",
            "Create a dev, merge, or release verification plan for the current Git diff.",
            mode_schema(),
            true,
        ),
        mcp_tool(
            "run_verification",
            "Run Verification",
            "Run the planned project-owned verification checks and write evidence.",
            serde_json::json!({
                "mode": mode_property(),
                "full": {
                    "type": "boolean",
                    "description": "When true, include production build proof if available."
                },
                "continue": {
                    "type": "boolean",
                    "description": "When true, continue from latest partial evidence and reuse still-valid passed checks."
                }
            }),
            false,
        ),
        mcp_tool(
            "explain_failure",
            "Explain Failure",
            "Explain the latest failed evidence and recommend the next agent action.",
            serde_json::json!({}),
            true,
        ),
        mcp_tool(
            "get_evidence",
            "Get Evidence",
            "Read the latest evidence or a specific evidence id from .vrt/evidence.",
            serde_json::json!({
                "evidence_id": {
                    "type": "string",
                    "description": "Optional evidence id. If omitted, .vrt/latest.json is returned."
                }
            }),
            true,
        ),
        mcp_tool(
            "escalate_verification",
            "Escalate Verification",
            "Plan stricter merge or release verification for the current Git diff without running arbitrary shell.",
            serde_json::json!({
                "level": {
                    "type": "string",
                    "enum": ["merge", "release"],
                    "description": "Target verification level."
                }
            }),
            true,
        ),
    ]
}

fn mcp_tool(
    name: &str,
    title: &str,
    description: &str,
    properties: serde_json::Value,
    read_only: bool,
) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties
        },
        "annotations": {
            "readOnlyHint": read_only,
            "destructiveHint": false,
            "openWorldHint": false
        }
    })
}

fn mode_schema() -> serde_json::Value {
    serde_json::json!({
        "mode": mode_property()
    })
}

fn mode_property() -> serde_json::Value {
    serde_json::json!({
        "type": "string",
        "enum": ["dev", "merge", "release"],
        "description": "Verification confidence target."
    })
}

fn call_mcp_tool(
    root: &Path,
    tool_name: &str,
    arguments: &serde_json::Value,
) -> Result<serde_json::Value> {
    match tool_name {
        "analyze_change" => {
            let profile = profile_project(root)?;
            let graph = build_capability_graph(root, &profile)?;
            let change = analyze_change(root, &profile)?;
            Ok(serde_json::json!({
                "profile": profile,
                "capabilities": graph.capabilities,
                "change": change
            }))
        }
        "plan_verification" => {
            let mode = mcp_mode(arguments, "mode", VerificationMode::Dev)?;
            let profile = profile_project(root)?;
            let graph = build_capability_graph(root, &profile)?;
            let change = analyze_change(root, &profile)?;
            let plan = plan_verification(&profile, &graph, &change, mode)?;
            Ok(serde_json::json!({
                "plan": plan
            }))
        }
        "run_verification" => {
            let mode = mcp_mode(arguments, "mode", VerificationMode::Dev)?;
            let full = arguments
                .get("full")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            let continue_after_failure = arguments
                .get("continue")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            let profile = initialize_project(root)?;
            let graph = build_capability_graph(root, &profile)?;
            let change = analyze_change(root, &profile)?;
            let mut plan = plan_verification(&profile, &graph, &change, mode)?;
            if full {
                add_build_step_for_mcp(&graph, &mut plan);
            }
            let evidence = if continue_after_failure {
                run_verification_continue(root, &profile, &change, &plan)?
            } else {
                run_verification(root, &profile, &change, &plan)?
            };
            Ok(serde_json::json!({
                "evidence": evidence
            }))
        }
        "explain_failure" => {
            let explanation = explain_latest(root)?;
            Ok(serde_json::json!({
                "explanation": explanation
            }))
        }
        "get_evidence" => {
            let evidence = if let Some(evidence_id) = arguments
                .get("evidence_id")
                .and_then(|value| value.as_str())
            {
                read_evidence(root, evidence_id)?
            } else {
                read_latest_evidence(root)?
            };
            Ok(serde_json::json!({
                "evidence": evidence
            }))
        }
        "escalate_verification" => {
            let level = arguments
                .get("level")
                .and_then(|value| value.as_str())
                .unwrap_or("merge");
            let mode = match level {
                "merge" => VerificationMode::Merge,
                "release" => VerificationMode::Release,
                other => anyhow::bail!("Unsupported escalation level: {other}"),
            };
            let profile = profile_project(root)?;
            let graph = build_capability_graph(root, &profile)?;
            let change = analyze_change(root, &profile)?;
            let plan = plan_verification(&profile, &graph, &change, mode)?;
            Ok(serde_json::json!({
                "level": level,
                "plan": plan
            }))
        }
        other => anyhow::bail!("Unknown tool: {other}"),
    }
}

fn mcp_mode(
    arguments: &serde_json::Value,
    key: &str,
    default: VerificationMode,
) -> Result<VerificationMode> {
    match arguments.get(key).and_then(|value| value.as_str()) {
        Some("dev") | None => Ok(default),
        Some("merge") => Ok(VerificationMode::Merge),
        Some("release") => Ok(VerificationMode::Release),
        Some(other) => anyhow::bail!("Unsupported verification mode: {other}"),
    }
}

fn add_build_step_for_mcp(graph: &CapabilityGraph, plan: &mut VerificationPlan) {
    if plan
        .steps
        .iter()
        .any(|step| step.capability_id.contains("build"))
    {
        return;
    }
    if let Some(cap) = graph.capabilities.iter().find(|cap| cap.kind == "build") {
        let order = plan.steps.len() as u32 + 1;
        plan.steps.push(PlanStep {
            id: format!("step_{order}"),
            capability_id: cap.id.clone(),
            command: cap.command.clone(),
            cwd: cap.cwd.clone(),
            reason: "MCP full verification requested production build proof.".to_string(),
            order,
            stop_on_failure: true,
            timeout_ms: Some(600_000),
        });
        plan.skipped.retain(|skip| skip.capability_id != cap.id);
    }
}

fn read_latest_evidence(root: &Path) -> Result<EvidenceRecord> {
    let path = root.join(".vrt/latest.json");
    serde_json::from_str(&fs::read_to_string(&path)?)
        .with_context(|| format!("parse {}", path.display()))
}

fn read_evidence(root: &Path, evidence_id: &str) -> Result<EvidenceRecord> {
    if evidence_id.contains('/') || evidence_id.contains('\\') || evidence_id.contains("..") {
        anyhow::bail!("Invalid evidence id");
    }
    let path = root
        .join(".vrt")
        .join("evidence")
        .join(evidence_id)
        .join("evidence.json");
    serde_json::from_str(&fs::read_to_string(&path)?)
        .with_context(|| format!("parse {}", path.display()))
}

fn tool_result(structured: serde_json::Value, is_error: bool) -> serde_json::Value {
    serde_json::json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string_pretty(&structured).unwrap_or_else(|_| "{}".to_string())
            }
        ],
        "structuredContent": structured,
        "isError": is_error
    })
}

fn jsonrpc_result(id: serde_json::Value, result: serde_json::Value) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
    .to_string()
}

fn jsonrpc_error(id: serde_json::Value, code: i64, message: String) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
    .to_string()
}

fn read_package_json(root: &Path) -> Result<PackageJson> {
    let path = root.join("package.json");
    let data = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&data).with_context(|| format!("parse {}", path.display()))
}

fn detect_package_manager(root: &Path) -> PackageManager {
    if root.join("pnpm-lock.yaml").exists() {
        PackageManager::Pnpm
    } else if root.join("package-lock.json").exists() {
        PackageManager::Npm
    } else if root.join("yarn.lock").exists() {
        PackageManager::Yarn
    } else if root.join("bun.lockb").exists() || root.join("bun.lock").exists() {
        PackageManager::Bun
    } else {
        PackageManager::Unknown
    }
}

fn has_dep(package_json: &PackageJson, dep: &str) -> bool {
    package_json.dependencies.contains_key(dep) || package_json.dev_dependencies.contains_key(dep)
}

fn has_script(package_json: &PackageJson, script: &str) -> bool {
    package_json.scripts.contains_key(script)
}

fn has_file(root: &Path, prefix: &str) -> bool {
    ["js", "mjs", "cjs", "ts", "mts"]
        .iter()
        .any(|ext| root.join(format!("{prefix}.{ext}")).exists())
}

fn discover_nodes(
    root: &Path,
    package_json: &PackageJson,
    frameworks: &BTreeSet<Detection>,
) -> Vec<ProjectNode> {
    let mut nodes = Vec::new();
    nodes.push(ProjectNode {
        id: "workspace".to_string(),
        name: package_json
            .name
            .clone()
            .unwrap_or_else(|| "workspace".to_string()),
        path: ".".to_string(),
        kind: "workspace".to_string(),
        framework: frameworks.iter().next().map(|item| format!("{item:?}")),
    });
    for entry in WalkDir::new(root)
        .min_depth(2)
        .max_depth(3)
        .into_iter()
        .flatten()
    {
        if entry.file_name() == "package.json" {
            if let Ok(relative) = entry.path().parent().unwrap_or(root).strip_prefix(root) {
                let path = relative.to_string_lossy().to_string();
                let kind = if path.starts_with("apps/") {
                    "app"
                } else {
                    "package"
                };
                nodes.push(ProjectNode {
                    id: path.replace('/', "-"),
                    name: path.clone(),
                    path,
                    kind: kind.to_string(),
                    framework: None,
                });
            }
        }
    }
    nodes
}

fn add_script_capability(
    profile: &ProjectProfile,
    capabilities: &mut Vec<VerificationCapability>,
    script: &str,
    kind: &str,
    cost: &str,
    contribution: &str,
    cwd: &str,
) {
    if profile.scripts.iter().any(|item| item.name == script) {
        capabilities.push(VerificationCapability {
            id: format!("workspace-{script}"),
            kind: kind.to_string(),
            command: package_script(profile, script),
            cwd: cwd.to_string(),
            scope: "workspace".to_string(),
            cost: cost.to_string(),
            confidence_contribution: contribution.to_string(),
            proves: proves_for_kind(kind),
            cannot_prove: cannot_prove_for_kind(kind),
            cacheable: true,
            parallelizable: kind != "build",
            side_effects: if kind == "build" {
                vec!["dist output".to_string()]
            } else {
                vec![]
            },
            resource_requirements: if kind == "build" {
                vec!["build-cache".to_string()]
            } else {
                vec![]
            },
        });
    }
}

fn package_script(profile: &ProjectProfile, script: &str) -> String {
    match profile.package_manager {
        PackageManager::Pnpm => format!("pnpm {script}"),
        PackageManager::Yarn => format!("yarn {script}"),
        PackageManager::Bun => format!("bun run {script}"),
        PackageManager::Npm | PackageManager::Unknown => format!("npm run {script}"),
    }
}

fn package_runner(profile: &ProjectProfile, command: &str) -> String {
    match profile.package_manager {
        PackageManager::Pnpm => format!("pnpm exec {command}"),
        PackageManager::Yarn => format!("yarn {command}"),
        PackageManager::Bun => format!("bunx {command}"),
        PackageManager::Npm | PackageManager::Unknown => format!("npx {command}"),
    }
}

fn proves_for_kind(kind: &str) -> Vec<String> {
    match kind {
        "typecheck" => vec!["TypeScript type consistency".to_string()],
        "lint" => vec!["Static lint rules".to_string()],
        "unit_test" => vec!["Package-level test behavior".to_string()],
        "build" => vec!["Production bundler compilation".to_string()],
        _ => vec!["Configured project verification capability".to_string()],
    }
}

fn cannot_prove_for_kind(kind: &str) -> Vec<String> {
    match kind {
        "typecheck" => vec![
            "Runtime API correctness".to_string(),
            "browser behavior".to_string(),
        ],
        "lint" => vec!["Type soundness".to_string(), "runtime behavior".to_string()],
        "unit_test" => vec![
            "Production bundler behavior".to_string(),
            "full browser behavior".to_string(),
        ],
        "build" => vec![
            "Business behavior correctness".to_string(),
            "external service correctness".to_string(),
        ],
        _ => vec!["Release readiness".to_string()],
    }
}

fn git_changed_files(root: &Path) -> Result<Vec<ChangedFile>> {
    let output = git_output(root, ["diff", "--name-status"])?;
    let mut changed = parse_name_status(&output);
    let staged = git_output(root, ["diff", "--cached", "--name-status"]).unwrap_or_default();
    changed.extend(parse_name_status(&staged));
    changed.sort_by(|a, b| a.path.cmp(&b.path));
    changed.dedup_by(|a, b| a.path == b.path);
    Ok(changed)
}

fn parse_name_status(output: &str) -> Vec<ChangedFile> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let status = parts.next()?.to_string();
            let path = parts.last()?.to_string();
            Some(ChangedFile { status, path })
        })
        .collect()
}

fn git_output<const N: usize>(root: &Path, args: [&str; N]) -> Result<String> {
    let output = Command::new("git").args(args).current_dir(root).output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        anyhow::bail!("{}", String::from_utf8_lossy(&output.stderr))
    }
}

fn classify_path(path: &str, risk_tags: &mut BTreeSet<RiskTag>) {
    if path.ends_with(".md") || path.starts_with("docs/") {
        risk_tags.insert(RiskTag::Docs);
    }
    if path.ends_with(".css") || path.ends_with(".scss") {
        risk_tags.insert(RiskTag::Style);
    }
    if path.ends_with(".tsx") || path.contains("/components/") {
        risk_tags.insert(RiskTag::UiComponent);
    }
    if path.contains("/api/") || path.contains("route.ts") {
        risk_tags.insert(RiskTag::ApiRoute);
    }
    if path.starts_with("packages/") {
        risk_tags.insert(RiskTag::SharedPackage);
    }
    if path == "package.json"
        || path.ends_with("lock.yaml")
        || path.ends_with("lock.json")
        || path.contains("tsconfig")
    {
        risk_tags.insert(RiskTag::PackageBoundary);
    }
    if path.contains("schema.prisma") {
        risk_tags.insert(RiskTag::DatabaseSchema);
    }
    if path.contains("migration") || path.contains("migrations/") {
        risk_tags.insert(RiskTag::Migration);
    }
    if path.contains("auth") {
        risk_tags.insert(RiskTag::Auth);
    }
    if path.contains("billing") || path.contains("stripe") {
        risk_tags.insert(RiskTag::Billing);
    }
    if path.contains(".env") {
        risk_tags.insert(RiskTag::Env);
    }
    if path.starts_with(".github/") {
        risk_tags.insert(RiskTag::Ci);
    }
    if path.contains("Dockerfile") || path.contains("terraform") || path.contains("deploy") {
        risk_tags.insert(RiskTag::Infra);
    }
}

fn affected_nodes(profile: &ProjectProfile, changed_files: &[ChangedFile]) -> Vec<String> {
    let mut affected = BTreeSet::new();
    for file in changed_files {
        for node in &profile.nodes {
            if node.path != "." && file.path.starts_with(&node.path) {
                affected.insert(node.id.clone());
            }
        }
    }
    if affected.is_empty() {
        affected.insert("workspace".to_string());
    }
    affected.into_iter().collect()
}

fn timeout_for_kind(kind: &str) -> u64 {
    match kind {
        "lint" => 120_000,
        "typecheck" => 180_000,
        "unit_test" => 240_000,
        "build" => 600_000,
        _ => 120_000,
    }
}

fn skipped_reason(cap: &VerificationCapability) -> (String, String) {
    match cap.kind.as_str() {
        "build" => (
            "No package boundary or production config change required build in this mode."
                .to_string(),
            "Production bundler behavior not verified.".to_string(),
        ),
        "lint" => (
            "Lower-cost type/test checks have higher information gain for this change.".to_string(),
            "Lint-only rules are not proven.".to_string(),
        ),
        "unit_test" => (
            "No executable JS/TS risk requiring package tests was detected.".to_string(),
            "Behavior covered only by unit tests is not proven.".to_string(),
        ),
        _ => (
            "Capability not selected by the current verification mode.".to_string(),
            "This capability did not contribute evidence.".to_string(),
        ),
    }
}

fn confidence_for(
    mode: VerificationMode,
    steps: &[PlanStep],
    skipped: &[SkippedCheck],
    requires_escalation: bool,
) -> ConfidenceReport {
    let has_typecheck = steps
        .iter()
        .any(|step| step.capability_id.contains("typecheck"));
    let has_build = steps
        .iter()
        .any(|step| step.capability_id.contains("build"));
    let local = if has_typecheck { "high" } else { "medium" };
    let merge = if has_build || matches!(mode, VerificationMode::Merge | VerificationMode::Release)
    {
        "medium"
    } else {
        "low"
    };
    let release = if matches!(mode, VerificationMode::Release) && has_build {
        "medium"
    } else {
        "insufficient"
    };
    ConfidenceReport {
        local: if requires_escalation { "medium" } else { local }.to_string(),
        merge: merge.to_string(),
        release: release.to_string(),
        summary:
            "Confidence is derived from selected evidence; skipped checks remain residual risk."
                .to_string(),
        residual_risks: skipped
            .iter()
            .map(|skip| skip.residual_risk.clone())
            .collect(),
    }
}

fn actual_confidence(
    plan: &VerificationPlan,
    checks: &[CheckEvidence],
    skipped: &[SkippedCheck],
) -> ConfidenceReport {
    if checks.iter().any(|check| check.status == "failed") {
        ConfidenceReport {
            local: "low".to_string(),
            merge: "none".to_string(),
            release: "insufficient".to_string(),
            summary: "Verification stopped on failure; only partial evidence is valid.".to_string(),
            residual_risks: skipped
                .iter()
                .map(|skip| skip.residual_risk.clone())
                .collect(),
        }
    } else {
        plan.expected_confidence.clone()
    }
}

fn summarize_log(log: &str) -> String {
    extract_root_causes(log)
        .into_iter()
        .next()
        .unwrap_or_else(|| {
            log.lines()
                .rev()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("")
                .to_string()
        })
}

fn extract_root_causes(log: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    for line in log.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('$') || trimmed.starts_with('>') {
            continue;
        }
        let lower = line.to_lowercase();
        if lower.contains("error")
            || lower.contains("failed")
            || lower.contains("panic")
            || lower.contains("exception")
        {
            candidates.push(line.trim().to_string());
        }
        if candidates.len() >= 5 {
            break;
        }
    }
    candidates
}

fn failure_kind(command: &str, log: &str) -> &'static str {
    let lower = format!("{} {}", command.to_lowercase(), log.to_lowercase());
    if lower.contains("type") || lower.contains("tsc") {
        "type_error"
    } else if lower.contains("lint") || lower.contains("eslint") {
        "lint_error"
    } else if lower.contains("test") || lower.contains("vitest") || lower.contains("jest") {
        "test_failure"
    } else if lower.contains("build") {
        "build_failure"
    } else {
        "command_failure"
    }
}

fn parse_location(text: &str) -> Option<(String, u64)> {
    let mut parts = text.split(':');
    let first = parts.next()?;
    let second = parts.next()?;
    if first.is_empty() {
        return None;
    }
    let line = second.parse::<u64>().ok()?;
    Some((first.to_string(), line.max(1)))
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn lockfile_hash(root: &Path) -> Option<String> {
    [
        "pnpm-lock.yaml",
        "package-lock.json",
        "yarn.lock",
        "bun.lock",
        "bun.lockb",
    ]
    .iter()
    .find_map(|name| {
        fs::read(root.join(name))
            .ok()
            .map(|bytes| hash_bytes(&bytes))
    })
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn hash_string(value: &str) -> String {
    hash_bytes(value.as_bytes())
}

fn hash_bytes(value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value);
    format!("{:x}", hasher.finalize())
}

fn write_json(path: PathBuf, value: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(value)?)?;
    Ok(())
}
