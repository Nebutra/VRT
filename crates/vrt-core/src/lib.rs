use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use walkdir::WalkDir;

pub const VRT_SCHEMA_VERSION: u32 = 1;

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
#[serde(rename_all = "snake_case")]
pub enum RiskTag {
    Docs,
    Marketing,
    Style,
    UiComponent,
    ApiRoute,
    SharedPackage,
    PackageBoundary,
    BuildConfig,
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
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
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
    pub ci: Vec<CiWorkflow>,
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
    pub package_name: Option<String>,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiWorkflow {
    pub provider: String,
    pub path: String,
    pub name: String,
    pub jobs: Vec<String>,
    pub commands: Vec<String>,
    #[serde(default)]
    pub runs_typecheck: bool,
    #[serde(default)]
    pub runs_test: bool,
    #[serde(default)]
    pub runs_build: bool,
    #[serde(default)]
    pub runs_e2e: bool,
    #[serde(default)]
    pub has_matrix: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeakSpot {
    pub id: String,
    pub message: String,
    #[serde(default)]
    pub suggestion: String,
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
    #[serde(default)]
    pub safety_level: String,
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
    #[serde(default)]
    pub safety_level: String,
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
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub evidence_id: String,
    pub continued_from: Option<String>,
    pub session_id: String,
    pub plan_id: String,
    pub change_id: String,
    pub base_commit: String,
    pub diff_hash: String,
    pub profile_hash: String,
    pub lockfile_hash: Option<String>,
    #[serde(default)]
    pub config_hash: String,
    #[serde(default)]
    pub toolchain_version: String,
    #[serde(default)]
    pub relevant_inputs_hash: String,
    #[serde(default)]
    pub env_assumptions: Vec<String>,
    pub dirty_state: DirtyState,
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
    #[serde(default)]
    pub broker_job_id: Option<String>,
    #[serde(default)]
    pub queue_wait_ms: u128,
    #[serde(default)]
    pub lock_wait_ms: u128,
    #[serde(default)]
    pub singleflight: SingleflightEvidence,
    #[serde(default)]
    pub resource_locks: Vec<ResourceLockRecord>,
    #[serde(default = "default_runner_pool")]
    pub runner_pool: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirtyState {
    pub is_dirty: bool,
    pub changed_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleflightEvidence {
    pub role: String,
    pub key: Option<String>,
    pub shared_from_evidence_id: Option<String>,
}

impl Default for SingleflightEvidence {
    fn default() -> Self {
        Self {
            role: "none".to_string(),
            key: None,
            shared_from_evidence_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLockRecord {
    pub resource_id: String,
    pub kind: String,
    pub mode: String,
    pub reason: String,
    pub waited_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckEvidence {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub safety_level: String,
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
pub struct FalseConfidenceCase {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub case_id: String,
    pub recorded_at: DateTime<Utc>,
    pub evidence_id: String,
    pub stricter_check: String,
    pub failure_summary: String,
    pub previous_confidence: ConfidenceReport,
    pub previous_validity: String,
    pub diff_hash: String,
    pub profile_hash: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeSession {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub root: String,
    pub worktree_path: String,
    pub branch: String,
    pub base_commit: String,
    pub status: String,
    pub instructions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionWorktreeContext {
    pub enabled: bool,
    pub path: Option<String>,
    pub branch: Option<String>,
    pub managed_by_vrt: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContext {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub session_id: String,
    pub name: Option<String>,
    pub agent_kind: String,
    pub repo_root: String,
    pub working_dir: String,
    pub worktree: SessionWorktreeContext,
    pub base_commit: String,
    pub current_head: String,
    pub diff_hash: String,
    pub dirty_state: String,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub status: String,
    pub last_evidence_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvidenceSummary {
    pub evidence_id: String,
    pub validity: String,
    pub finished_at: DateTime<Utc>,
    pub checks_run: usize,
    pub checks_failed: usize,
    pub checks_reused: usize,
    pub checks_skipped: usize,
    pub confidence: ConfidenceReport,
    pub diff_hash: String,
    pub profile_hash: String,
    pub report_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeSessionView {
    pub session: WorktreeSession,
    pub latest_evidence: Option<SessionEvidenceSummary>,
    pub active_lock: Option<serde_json::Value>,
    pub false_confidence_cases: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiAgentSessionView {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub generated_at: DateTime<Utc>,
    pub root: String,
    pub sessions: Vec<WorktreeSessionView>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TokenProfile {
    Standard,
    Rtk,
    Headroom,
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

impl PackageJson {
    fn empty() -> Self {
        Self {
            scripts: std::collections::BTreeMap::new(),
            dependencies: std::collections::BTreeMap::new(),
            dev_dependencies: std::collections::BTreeMap::new(),
            name: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct VrtConfig {
    policy: Option<VrtPolicyConfig>,
    release: Option<VrtReleaseConfig>,
}

#[derive(Debug, Deserialize)]
struct VrtPolicyConfig {
    default_mode: Option<String>,
    strict: Option<VrtAreaPolicyConfig>,
    relaxed: Option<VrtAreaPolicyConfig>,
}

#[derive(Debug, Deserialize)]
struct VrtAreaPolicyConfig {
    areas: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct VrtReleaseConfig {
    require_full_build: Option<bool>,
    require_ci: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct ReleasePolicy {
    require_full_build: bool,
    require_ci: bool,
}

fn default_schema_version() -> u32 {
    VRT_SCHEMA_VERSION
}

fn default_runner_pool() -> String {
    "cheap".to_string()
}

pub fn initialize_project(root: &Path) -> Result<ProjectProfile> {
    let profile = profile_project(root)?;
    let vrt_dir = root.join(".vrt");
    fs::create_dir_all(&vrt_dir).with_context(|| format!("create {}", vrt_dir.display()))?;
    write_json(vrt_dir.join("profile.json"), &profile)?;
    let config = r#"schema_version = 1

[policy]
default_mode = "dev"

[policy.strict]
areas = ["auth", "billing", "database", "env", "infra"]

[policy.relaxed]
areas = ["docs", "marketing"]

[release]
require_full_build = true
require_ci = true
"#;
    let config_path = vrt_dir.join("config.toml");
    if !config_path.exists() {
        fs::write(config_path, config)?;
    }
    Ok(profile)
}

pub fn resolve_verification_mode(
    root: &Path,
    requested: Option<VerificationMode>,
) -> Result<VerificationMode> {
    if let Some(mode) = requested {
        return Ok(mode);
    }
    let path = root.join(".vrt/config.toml");
    if !path.exists() {
        return Ok(VerificationMode::Dev);
    }
    let config: VrtConfig = toml::from_str(&fs::read_to_string(&path)?)
        .with_context(|| format!("parse {}", path.display()))?;
    match config
        .policy
        .and_then(|policy| policy.default_mode)
        .as_deref()
    {
        Some("dev") | None => Ok(VerificationMode::Dev),
        Some("merge") => Ok(VerificationMode::Merge),
        Some("release") => Ok(VerificationMode::Release),
        Some(other) => anyhow::bail!(
            "Unsupported VRT policy.default_mode `{other}` in {}",
            path.display()
        ),
    }
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
    if has_dep(&package_json, "drizzle-kit") || has_file(root, "drizzle.config") {
        tools.insert(Detection::Drizzle);
    }
    if has_dep(&package_json, "eslint") || has_script(&package_json, "lint") {
        tools.insert(Detection::Eslint);
    }
    if has_dep(&package_json, "@biomejs/biome") || has_script(&package_json, "format") {
        tools.insert(Detection::Biome);
    }

    let workspace_kind = if !root.join("package.json").exists() {
        "unknown".to_string()
    } else if tools.contains(&Detection::Nx) {
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
    let ci = discover_ci_workflows(root);
    let mut weak_spots = Vec::new();
    if !root.join("package.json").exists() {
        weak_spots.push(WeakSpot {
            id: "no-package-json".to_string(),
            message: "No package.json detected; JS/TS verification capabilities are unavailable."
                .to_string(),
            suggestion:
                "Add a package.json with project-owned scripts so VRT can discover verification capabilities."
                    .to_string(),
        });
    }
    if !package_json.scripts.contains_key("typecheck") {
        weak_spots.push(WeakSpot {
            id: "no-typecheck-script".to_string(),
            message: "No typecheck script detected; TypeScript consistency is not locally proven."
                .to_string(),
            suggestion:
                "Add a package.json typecheck script, for example `tsc --noEmit` or your workspace typecheck command."
                    .to_string(),
        });
    }
    if !package_json.scripts.contains_key("test") {
        weak_spots.push(WeakSpot {
            id: "no-test-script".to_string(),
            message: "No test script detected; package behavior is not locally proven.".to_string(),
            suggestion:
                "Add a package.json test script that runs the project's unit or integration test runner."
                    .to_string(),
        });
    }
    if !has_browser_smoke_script(&package_json) {
        weak_spots.push(WeakSpot {
            id: "no-playwright-smoke".to_string(),
            message: "No Playwright smoke test detected; browser behavior is not locally proven."
                .to_string(),
            suggestion:
                "Add a smoke, test:smoke, e2e, test:e2e, playwright, or test:playwright script when browser behavior needs local proof."
                    .to_string(),
        });
    }
    if !package_json.scripts.contains_key("build") {
        weak_spots.push(WeakSpot {
            id: "no-build-script".to_string(),
            message: "No build script detected; release confidence requires external proof."
                .to_string(),
            suggestion:
                "Add a package.json build script that runs the production bundler or framework build."
                    .to_string(),
        });
    }
    let destructive_scripts = destructive_package_scripts(&package_json);
    if !destructive_scripts.is_empty() {
        weak_spots.push(WeakSpot {
            id: "destructive-script".to_string(),
            message: format!(
                "Destructive package scripts detected: {}. VRT will not run them automatically.",
                destructive_scripts.join(", ")
            ),
            suggestion:
                "Run destructive scripts manually after reviewing the command, target environment, and rollback plan."
                    .to_string(),
        });
    }
    if !has_env_validation_script(&package_json) {
        weak_spots.push(WeakSpot {
            id: "no-env-validation".to_string(),
            message:
                "No explicit environment validation detected; release confidence requires external environment proof."
                    .to_string(),
            suggestion:
                "Add an env:check, check:env, validate-env, dotenvx, @t3-oss/env, t3-env, or zod-env script."
                    .to_string(),
        });
    }
    if tools.contains(&Detection::Prisma) && !has_migration_safety_script(&package_json) {
        weak_spots.push(WeakSpot {
            id: "no-migration-safety-check".to_string(),
            message:
                "No migration safety check detected; database release confidence requires external migration proof."
                    .to_string(),
            suggestion:
                "Add a migration:check, migrations:check, check:migration, check:migrations, or prisma migrate diff/status script."
                    .to_string(),
        });
    }
    if ci.is_empty() {
        weak_spots.push(WeakSpot {
            id: "no-ci-config".to_string(),
            message:
                "No CI workflow detected; merge and release confidence require external proof."
                    .to_string(),
            suggestion:
                "Add a .github/workflows/*.yml or *.yaml workflow that runs the project's verification pipeline."
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
        "ci": ci,
    });
    let profile_hash = hash_string(&serde_json::to_string(&fingerprint)?);

    Ok(ProjectProfile {
        schema_version: VRT_SCHEMA_VERSION,
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
        ci,
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
    add_test_capabilities(profile, &mut capabilities, &cwd);
    add_script_capability(
        profile,
        &mut capabilities,
        "build",
        "build",
        "expensive",
        "high",
        &cwd,
    );
    add_format_check_capability(profile, &mut capabilities, &cwd);
    add_env_validation_capability(profile, &mut capabilities, &cwd);
    add_migration_safety_capability(profile, &mut capabilities, &cwd);
    add_browser_smoke_capability(profile, &mut capabilities, &cwd);
    if profile.tools.contains(&Detection::Prisma) {
        let command = package_runner(profile, "prisma validate");
        capabilities.push(VerificationCapability {
            id: "workspace-prisma-validate".to_string(),
            kind: "schema_validate".to_string(),
            safety_level: command_safety_level("schema_validate", &command, "cheap").to_string(),
            command,
            cwd: cwd.clone(),
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
        add_prisma_generate_capability(profile, &mut capabilities, &cwd);
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
            RiskTag::PackageBoundary
                | RiskTag::BuildConfig
                | RiskTag::Env
                | RiskTag::Infra
                | RiskTag::Ci
        )
    });
    let policy_requires_escalation =
        strict_policy_matches(root, &risk_tags)? && !relaxed_policy_matches(root, &risk_tags)?;
    let requires_escalation = global_boundary_changed
        || policy_requires_escalation
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
    let release_policy = release_policy(Path::new(&profile.root))?;
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
                safety_level: cap.safety_level.clone(),
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
        add(
            "schema_generate",
            "Database schema or migration changed; regenerate Prisma client when configured.",
        );
    }
    if matches!(mode, VerificationMode::Release)
        || change
            .risk_tags
            .iter()
            .any(|tag| matches!(tag, RiskTag::DatabaseSchema | RiskTag::Migration))
    {
        add(
            "migration_safety",
            "Database schema, migration, or release mode needs migration safety proof when configured.",
        );
    }
    if matches!(mode, VerificationMode::Release) || change.risk_tags.contains(&RiskTag::Env) {
        add(
            "env_validate",
            "Environment-sensitive change or release mode needs environment contract validation when configured.",
        );
    }
    if matches!(mode, VerificationMode::Merge | VerificationMode::Release)
        || change.requires_escalation
    {
        add(
            "lint",
            "Merge/release or high-risk change needs static hygiene proof.",
        );
        add(
            "format_check",
            "Merge/release or high-risk change needs format and style consistency proof when configured.",
        );
        add(
            "browser_smoke",
            "Merge/release or high-risk change needs browser smoke proof when configured.",
        );
    }
    let release_requires_build =
        matches!(mode, VerificationMode::Release) && release_policy.require_full_build;
    if release_requires_build || change.global_boundary_changed || change.requires_escalation {
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
    if matches!(mode, VerificationMode::Release) && release_policy.require_ci {
        skipped.push(SkippedCheck {
            capability_id: "external-ci".to_string(),
            reason: if profile.ci.is_empty() {
                "Release policy requires CI proof, but no CI workflow was detected.".to_string()
            } else {
                "Release policy requires external CI proof; local VRT did not execute hosted CI."
                    .to_string()
            },
            residual_risk: "External CI evidence not collected by this local run.".to_string(),
        });
    }
    let residual_risks = skipped
        .iter()
        .map(|skip| skip.residual_risk.clone())
        .collect::<Vec<_>>();
    let expected_confidence = confidence_for(
        mode,
        &ordered,
        &skipped,
        change.requires_escalation,
        release_policy.require_ci,
    );
    let plan_fingerprint = serde_json::json!({
        "mode": mode,
        "change": change.change_id,
        "profile": profile.profile_hash,
        "release_policy": release_policy,
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
    if matches!(mode, VerificationMode::Release) && release_policy.require_ci {
        escalations.push(EscalationRequirement {
            level: "ci".to_string(),
            reason: if profile.ci.is_empty() {
                "Release policy requires external CI evidence, but no CI workflow was detected."
                    .to_string()
            } else {
                "Release policy requires external CI evidence from the configured workflow."
                    .to_string()
            },
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
    run_verification_internal(
        root,
        profile,
        change,
        plan,
        VerificationRunOptions::default(),
    )
}

pub fn run_verification_brokered(
    root: &Path,
    profile: &ProjectProfile,
    change: &ChangeSet,
    plan: &VerificationPlan,
) -> Result<EvidenceRecord> {
    let job_id = format!("job_{}", Uuid::new_v4().simple());
    write_broker_job(root, &job_id, plan, "queued", None)?;
    let queue_start = Instant::now();
    let pool_slot = match BrokerRunnerPoolSlot::acquire(root, &job_id, plan) {
        Ok(slot) => slot,
        Err(error) => {
            let _ = write_broker_job_error(root, &job_id, plan, error.to_string());
            return Err(error);
        }
    };
    let queue_wait_ms = queue_start.elapsed().as_millis();
    write_broker_job(root, &job_id, plan, "running", None)?;
    let lock_start = Instant::now();
    let held_locks = match BrokerResourceLocks::acquire(root, &job_id, plan) {
        Ok(locks) => locks,
        Err(error) => {
            drop(pool_slot);
            let _ = write_broker_job_error(root, &job_id, plan, error.to_string());
            return Err(error);
        }
    };
    let lock_wait_ms = lock_start.elapsed().as_millis();
    let evidence = run_verification_internal(
        root,
        profile,
        change,
        plan,
        VerificationRunOptions {
            context: RunContext {
                broker_job_id: Some(job_id.clone()),
                queue_wait_ms,
                lock_wait_ms,
            },
            ..VerificationRunOptions::default()
        },
    );
    drop(held_locks);
    drop(pool_slot);
    match &evidence {
        Ok(evidence) => {
            let status = if evidence.validity == "valid" {
                "passed"
            } else {
                "failed"
            };
            write_broker_job(root, &job_id, plan, status, Some(evidence))?;
        }
        Err(error) => {
            write_broker_job_error(root, &job_id, plan, error.to_string())?;
        }
    }
    evidence
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
        && previous.config_hash == config_hash(root)
        && previous.toolchain_version == toolchain_version()
        && previous.env_assumptions == env_assumptions()
        && previous.relevant_inputs_hash == relevant_inputs_hash(root, profile, change, plan)
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
        if previous.config_hash != config_hash(root) {
            stale_reasons.push("config hash changed; previous checks were not reused".to_string());
        }
        if previous.toolchain_version != toolchain_version() {
            stale_reasons
                .push("toolchain version changed; previous checks were not reused".to_string());
        }
        if previous.env_assumptions != env_assumptions() {
            stale_reasons.push(
                "environment assumptions changed; previous checks were not reused".to_string(),
            );
        }
        if previous.relevant_inputs_hash != relevant_inputs_hash(root, profile, change, plan) {
            stale_reasons
                .push("relevant inputs changed; previous checks were not reused".to_string());
        }
    }
    run_verification_internal(
        root,
        profile,
        change,
        &continued_plan,
        VerificationRunOptions {
            continued_from: Some(previous.evidence_id),
            reused_checks,
            stale_reasons,
            ..VerificationRunOptions::default()
        },
    )
}

#[derive(Debug, Clone, Default)]
struct RunContext {
    broker_job_id: Option<String>,
    queue_wait_ms: u128,
    lock_wait_ms: u128,
}

#[derive(Debug, Clone, Default)]
struct VerificationRunOptions {
    continued_from: Option<String>,
    reused_checks: Vec<CheckEvidence>,
    stale_reasons: Vec<String>,
    context: RunContext,
}

fn run_verification_internal(
    root: &Path,
    profile: &ProjectProfile,
    change: &ChangeSet,
    plan: &VerificationPlan,
    options: VerificationRunOptions,
) -> Result<EvidenceRecord> {
    let VerificationRunOptions {
        continued_from,
        reused_checks,
        stale_reasons,
        context: run_context,
    } = options;
    let _lock = match VerificationRunLock::acquire_or_join(root, profile, change, plan)? {
        VerificationRunLockState::Acquired(lock) => lock,
        VerificationRunLockState::Joined(evidence) => {
            return write_singleflight_follower_evidence(
                root,
                profile,
                change,
                plan,
                &evidence,
                &run_context,
            )
        }
    };
    if continued_from.is_none() && reused_checks.is_empty() && stale_reasons.is_empty() {
        if let Some(evidence) = read_cached_evidence(root, profile, change, plan)? {
            return write_cached_evidence_hit(root, profile, change, plan, &evidence, &run_context);
        }
    }
    let evidence_id = format!("ev_{}", Uuid::new_v4().simple());
    let raw_log_dir = PathBuf::from(".vrt").join("evidence").join(&evidence_id);
    fs::create_dir_all(root.join(&raw_log_dir))?;
    let started_at = Utc::now();
    let started = Instant::now();
    let mut checks = Vec::new();
    let mut failed = false;
    for step in &plan.steps {
        let step_started = Instant::now();
        let step_cwd = if step.cwd.is_empty() {
            root
        } else {
            Path::new(&step.cwd)
        };
        let output = if is_destructive_command(&step.command) {
            refused_destructive_output(&step.command)
        } else {
            run_shell_step(&step.command, step_cwd, step.timeout_ms)
                .with_context(|| format!("run {}", step.command))?
        };
        let duration_ms = step_started.elapsed().as_millis();
        let mut log = String::new();
        log.push_str("$ ");
        log.push_str(&step.command);
        log.push('\n');
        log.push_str(&output.stdout);
        log.push_str(&output.stderr);
        if output.timed_out {
            log.push_str(&format!(
                "[vrt] timed out after {}ms\n",
                step.timeout_ms.unwrap_or_default()
            ));
        }
        let raw_log = raw_log_dir.join(format!("{}.raw.log", sanitize(&step.id)));
        fs::write(root.join(&raw_log), &log)?;
        let status = if output.success {
            "passed"
        } else {
            failed = true;
            "failed"
        };
        checks.push(CheckEvidence {
            name: step.capability_id.clone(),
            command: step.command.clone(),
            safety_level: step.safety_level.clone(),
            status: status.to_string(),
            exit_code: output.exit_code,
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
        schema_version: VRT_SCHEMA_VERSION,
        evidence_id,
        continued_from,
        session_id: plan.session_id.clone(),
        plan_id: plan.plan_id.clone(),
        change_id: change.change_id.clone(),
        base_commit: change.base_commit.clone(),
        diff_hash: change.diff_hash.clone(),
        profile_hash: profile.profile_hash.clone(),
        lockfile_hash: lockfile_hash(root),
        config_hash: config_hash(root),
        toolchain_version: toolchain_version(),
        relevant_inputs_hash: relevant_inputs_hash(root, profile, change, plan),
        env_assumptions: env_assumptions(),
        dirty_state: dirty_state(root),
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
        broker_job_id: run_context.broker_job_id,
        queue_wait_ms: run_context.queue_wait_ms,
        lock_wait_ms: run_context.lock_wait_ms,
        singleflight: SingleflightEvidence::default(),
        resource_locks: resource_locks_for_plan_with_wait(plan, run_context.lock_wait_ms),
        runner_pool: runner_pool_for_plan(plan),
    };
    write_json(root.join(&evidence.report_path), &evidence)?;
    write_json(root.join(".vrt/latest.json"), &evidence)?;
    register_session_context(root, change, plan, &evidence)?;
    write_evidence_cache_entry(root, profile, change, plan, &evidence)?;
    Ok(evidence)
}

fn read_cached_evidence(
    root: &Path,
    profile: &ProjectProfile,
    change: &ChangeSet,
    plan: &VerificationPlan,
) -> Result<Option<EvidenceRecord>> {
    let path = evidence_cache_entry_path(root, profile, change, plan);
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(None);
    };
    let entry: serde_json::Value =
        serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    let Some(report_path) = entry.get("report_path").and_then(|value| value.as_str()) else {
        return Ok(None);
    };
    let Ok(report_text) = fs::read_to_string(root.join(report_path)) else {
        return Ok(None);
    };
    let Ok(evidence) = serde_json::from_str::<EvidenceRecord>(&report_text) else {
        return Ok(None);
    };
    if evidence_is_reusable_cache_source(root, &evidence, profile, change, plan)? {
        Ok(Some(evidence))
    } else {
        Ok(None)
    }
}

fn write_cached_evidence_hit(
    root: &Path,
    profile: &ProjectProfile,
    change: &ChangeSet,
    plan: &VerificationPlan,
    cached: &EvidenceRecord,
    run_context: &RunContext,
) -> Result<EvidenceRecord> {
    let evidence_id = format!("ev_{}", Uuid::new_v4().simple());
    let raw_log_dir = PathBuf::from(".vrt").join("evidence").join(&evidence_id);
    fs::create_dir_all(root.join(&raw_log_dir))?;
    let started_at = Utc::now();
    let log_path = raw_log_dir.join("cache-hit.raw.log");
    fs::write(
        root.join(&log_path),
        format!(
            "[vrt] reused cached evidence {}\n[vrt] exact match: plan_id={} diff_hash={} profile_hash={}\n",
            cached.evidence_id, plan.plan_id, change.diff_hash, profile.profile_hash
        ),
    )?;
    let mut reused_checks = cached.reused_checks.clone();
    reused_checks.extend(cached.checks.clone());
    let report_path = raw_log_dir.join("evidence.json");
    let evidence = EvidenceRecord {
        schema_version: VRT_SCHEMA_VERSION,
        evidence_id,
        continued_from: Some(cached.evidence_id.clone()),
        session_id: plan.session_id.clone(),
        plan_id: plan.plan_id.clone(),
        change_id: change.change_id.clone(),
        base_commit: change.base_commit.clone(),
        diff_hash: change.diff_hash.clone(),
        profile_hash: profile.profile_hash.clone(),
        lockfile_hash: lockfile_hash(Path::new(&profile.root)),
        config_hash: config_hash(Path::new(&profile.root)),
        toolchain_version: toolchain_version(),
        relevant_inputs_hash: relevant_inputs_hash(Path::new(&profile.root), profile, change, plan),
        env_assumptions: env_assumptions(),
        dirty_state: dirty_state(Path::new(&profile.root)),
        started_at,
        finished_at: Utc::now(),
        duration_ms: 0,
        checks: vec![],
        reused_checks,
        skipped: plan.skipped.clone(),
        confidence: plan.expected_confidence.clone(),
        raw_log_dir: raw_log_dir.to_string_lossy().to_string(),
        report_path: report_path.to_string_lossy().to_string(),
        validity: "valid".to_string(),
        stale_reasons: vec![],
        broker_job_id: run_context.broker_job_id.clone(),
        queue_wait_ms: run_context.queue_wait_ms,
        lock_wait_ms: run_context.lock_wait_ms,
        singleflight: SingleflightEvidence::default(),
        resource_locks: resource_locks_for_plan_with_wait(plan, run_context.lock_wait_ms),
        runner_pool: runner_pool_for_plan(plan),
    };
    write_json(root.join(&evidence.report_path), &evidence)?;
    write_json(root.join(".vrt/latest.json"), &evidence)?;
    register_session_context(root, change, plan, &evidence)?;
    write_evidence_cache_entry(root, profile, change, plan, &evidence)?;
    Ok(evidence)
}

fn write_singleflight_follower_evidence(
    root: &Path,
    profile: &ProjectProfile,
    change: &ChangeSet,
    plan: &VerificationPlan,
    leader: &EvidenceRecord,
    run_context: &RunContext,
) -> Result<EvidenceRecord> {
    let evidence_id = format!("ev_{}", Uuid::new_v4().simple());
    let raw_log_dir = PathBuf::from(".vrt").join("evidence").join(&evidence_id);
    fs::create_dir_all(root.join(&raw_log_dir))?;
    let started_at = Utc::now();
    let log_path = raw_log_dir.join("singleflight-follower.raw.log");
    fs::write(
        root.join(&log_path),
        format!(
            "[vrt] joined singleflight evidence {}\n[vrt] exact match: plan_id={} diff_hash={} profile_hash={}\n",
            leader.evidence_id, plan.plan_id, change.diff_hash, profile.profile_hash
        ),
    )?;
    let mut reused_checks = leader.reused_checks.clone();
    reused_checks.extend(leader.checks.clone());
    for check in &mut reused_checks {
        if check.status == "passed" {
            check.status = "reused".to_string();
        }
        check.summary = format!("Shared from singleflight evidence {}", leader.evidence_id);
    }
    let report_path = raw_log_dir.join("evidence.json");
    let evidence = EvidenceRecord {
        schema_version: VRT_SCHEMA_VERSION,
        evidence_id,
        continued_from: Some(leader.evidence_id.clone()),
        session_id: plan.session_id.clone(),
        plan_id: plan.plan_id.clone(),
        change_id: change.change_id.clone(),
        base_commit: change.base_commit.clone(),
        diff_hash: change.diff_hash.clone(),
        profile_hash: profile.profile_hash.clone(),
        lockfile_hash: lockfile_hash(root),
        config_hash: config_hash(root),
        toolchain_version: toolchain_version(),
        relevant_inputs_hash: relevant_inputs_hash(root, profile, change, plan),
        env_assumptions: env_assumptions(),
        dirty_state: dirty_state(root),
        started_at,
        finished_at: Utc::now(),
        duration_ms: 0,
        checks: vec![],
        reused_checks,
        skipped: plan.skipped.clone(),
        confidence: leader.confidence.clone(),
        raw_log_dir: raw_log_dir.to_string_lossy().to_string(),
        report_path: report_path.to_string_lossy().to_string(),
        validity: leader.validity.clone(),
        stale_reasons: vec![],
        broker_job_id: run_context.broker_job_id.clone(),
        queue_wait_ms: run_context.queue_wait_ms,
        lock_wait_ms: run_context.lock_wait_ms,
        singleflight: SingleflightEvidence {
            role: "follower".to_string(),
            key: Some(singleflight_key(root, profile, change, plan)?),
            shared_from_evidence_id: Some(leader.evidence_id.clone()),
        },
        resource_locks: resource_locks_for_plan_with_wait(plan, run_context.lock_wait_ms),
        runner_pool: runner_pool_for_plan(plan),
    };
    write_json(root.join(&evidence.report_path), &evidence)?;
    write_json(root.join(".vrt/latest.json"), &evidence)?;
    register_session_context(root, change, plan, &evidence)?;
    write_evidence_cache_entry(root, profile, change, plan, &evidence)?;
    Ok(evidence)
}

fn write_evidence_cache_entry(
    root: &Path,
    profile: &ProjectProfile,
    change: &ChangeSet,
    plan: &VerificationPlan,
    evidence: &EvidenceRecord,
) -> Result<()> {
    if !evidence_is_reusable_cache_source(root, evidence, profile, change, plan)? {
        return Ok(());
    }
    let path = evidence_cache_entry_path(root, profile, change, plan);
    write_json(
        path,
        &serde_json::json!({
            "schema_version": VRT_SCHEMA_VERSION,
            "cache_key": evidence_cache_key(root, profile, change, plan),
            "evidence_id": evidence.evidence_id,
            "report_path": evidence.report_path,
            "written_at": Utc::now(),
        }),
    )
}

fn evidence_is_reusable_cache_source(
    root: &Path,
    evidence: &EvidenceRecord,
    profile: &ProjectProfile,
    change: &ChangeSet,
    plan: &VerificationPlan,
) -> Result<bool> {
    if evidence.validity != "valid"
        || !evidence.stale_reasons.is_empty()
        || !evidence_matches_plan(evidence, profile, change, plan)
        || evidence.lockfile_hash != lockfile_hash(root)
        || evidence.config_hash != config_hash(root)
        || evidence.toolchain_version != toolchain_version()
        || evidence.relevant_inputs_hash != relevant_inputs_hash(root, profile, change, plan)
        || evidence.env_assumptions != env_assumptions()
    {
        return Ok(false);
    }
    if serde_json::to_value(&evidence.skipped)? != serde_json::to_value(&plan.skipped)? {
        return Ok(false);
    }
    let proven = evidence
        .checks
        .iter()
        .chain(evidence.reused_checks.iter())
        .map(|check| {
            (
                check.name.as_str(),
                check.command.as_str(),
                check.status.as_str(),
            )
        })
        .collect::<BTreeSet<_>>();
    Ok(plan.steps.iter().all(|step| {
        proven.contains(&(step.capability_id.as_str(), step.command.as_str(), "passed"))
    }))
}

fn evidence_cache_entry_path(
    root: &Path,
    profile: &ProjectProfile,
    change: &ChangeSet,
    plan: &VerificationPlan,
) -> PathBuf {
    root.join(".vrt")
        .join("cache")
        .join("evidence")
        .join(format!(
            "{}.json",
            evidence_cache_key(root, profile, change, plan)
        ))
}

fn evidence_cache_key(
    root: &Path,
    profile: &ProjectProfile,
    change: &ChangeSet,
    plan: &VerificationPlan,
) -> String {
    let fingerprint = serde_json::json!({
        "schema_version": VRT_SCHEMA_VERSION,
        "plan_id": plan.plan_id,
        "base_commit": change.base_commit,
        "diff_hash": change.diff_hash,
        "profile_hash": profile.profile_hash,
        "lockfile_hash": lockfile_hash(root),
        "config_hash": config_hash(root),
        "toolchain_version": toolchain_version(),
    });
    hash_string(&fingerprint.to_string())[..24].to_string()
}

struct ShellStepOutput {
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    success: bool,
    timed_out: bool,
}

fn run_shell_step(command: &str, cwd: &Path, timeout_ms: Option<u64>) -> Result<ShellStepOutput> {
    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(command)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_process_group(&mut cmd);
    let mut child = cmd.spawn()?;
    if let Some(timeout_ms) = timeout_ms {
        let timeout = Duration::from_millis(timeout_ms);
        let started = Instant::now();
        loop {
            if child.try_wait()?.is_some() {
                let output = child.wait_with_output()?;
                return Ok(shell_step_output(output, false));
            }
            if started.elapsed() >= timeout {
                terminate_child_tree(&mut child);
                let output = child.wait_with_output()?;
                return Ok(ShellStepOutput {
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                    exit_code: None,
                    success: false,
                    timed_out: true,
                });
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    let output = child.wait_with_output()?;
    Ok(shell_step_output(output, false))
}

fn shell_step_output(output: std::process::Output, timed_out: bool) -> ShellStepOutput {
    ShellStepOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code(),
        success: output.status.success() && !timed_out,
        timed_out,
    }
}

fn refused_destructive_output(command: &str) -> ShellStepOutput {
    ShellStepOutput {
        stdout: String::new(),
        stderr: format!(
            "[vrt] refused destructive command: {command}\n[vrt] refused destructive command; explicit user execution is required outside VRT.\n"
        ),
        exit_code: None,
        success: false,
        timed_out: false,
    }
}

fn is_destructive_command(command: &str) -> bool {
    let normalized = command
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    normalized.contains("rm -rf")
        || normalized.contains("rm -fr")
        || normalized.contains("git reset --hard")
        || normalized.contains("prisma migrate deploy")
        || normalized.contains("db push")
        || normalized.contains("docker compose down -v")
        || normalized.contains("docker-compose down -v")
}

fn command_safety_level(kind: &str, command: &str, cost: &str) -> &'static str {
    if is_destructive_command(command) {
        return "destructive";
    }
    let normalized = command
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if normalized.contains("curl ")
        || normalized.contains("wget ")
        || normalized.contains("npm publish")
        || normalized.contains("pnpm publish")
        || normalized.contains("yarn npm publish")
    {
        return "networked";
    }
    if cost == "expensive" || matches!(kind, "build" | "full_test" | "browser_smoke") {
        return "expensive";
    }
    if matches!(
        kind,
        "typecheck" | "lint" | "format_check" | "schema_validate" | "unit_test" | "related_test"
    ) || normalized.contains("tsc --noemit")
        || normalized.contains("tsc --no-emit")
        || normalized.contains("eslint")
        || normalized.contains("biome check")
        || normalized.contains("vitest")
        || normalized.contains("jest")
        || normalized.contains("prisma validate")
    {
        return "safe";
    }
    if normalized.starts_with("npm ")
        || normalized.starts_with("pnpm ")
        || normalized.starts_with("yarn ")
        || normalized.starts_with("bun ")
        || normalized.starts_with("npx ")
    {
        return "project";
    }
    "unknown"
}

#[cfg(unix)]
fn configure_process_group(cmd: &mut Command) {
    unsafe {
        cmd.pre_exec(|| {
            if libc::setpgid(0, 0) == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        });
    }
}

#[cfg(not(unix))]
fn configure_process_group(_cmd: &mut Command) {}

#[cfg(unix)]
fn terminate_child_tree(child: &mut Child) {
    unsafe {
        libc::killpg(child.id() as i32, libc::SIGKILL);
    }
    let _ = child.kill();
}

#[cfg(not(unix))]
fn terminate_child_tree(child: &mut Child) {
    let _ = child.kill();
}

struct VerificationRunLock {
    path: PathBuf,
}

enum VerificationRunLockState {
    Acquired(VerificationRunLock),
    Joined(Box<EvidenceRecord>),
}

impl VerificationRunLock {
    fn acquire_or_join(
        root: &Path,
        profile: &ProjectProfile,
        change: &ChangeSet,
        plan: &VerificationPlan,
    ) -> Result<VerificationRunLockState> {
        let vrt_dir = root.join(".vrt");
        fs::create_dir_all(&vrt_dir)?;
        let path = vrt_dir.join("run.lock");
        match fs::create_dir(&path) {
            Ok(()) => {
                let lock = serde_json::json!({
                    "session_id": plan.session_id,
                    "plan_id": plan.plan_id,
                    "started_at": Utc::now(),
                    "pid": std::process::id(),
                });
                write_json(path.join("lock.json"), &lock)?;
                Ok(VerificationRunLockState::Acquired(Self { path }))
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                let detail = fs::read_to_string(path.join("lock.json"))
                    .ok()
                    .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
                let session = detail
                    .as_ref()
                    .and_then(|value| value.get("session_id"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown-session");
                let plan_id = detail
                    .as_ref()
                    .and_then(|value| value.get("plan_id"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown-plan");
                if plan_id == plan.plan_id {
                    if let Some(evidence) = wait_for_singleflight_evidence(
                        root,
                        profile,
                        change,
                        plan,
                        singleflight_wait_timeout(),
                    )? {
                        return Ok(VerificationRunLockState::Joined(Box::new(evidence)));
                    }
                }
                anyhow::bail!(
                    "verification is already running for this worktree: session_id={session} plan_id={plan_id}. If this is stale, remove {}",
                    path.display()
                );
            }
            Err(error) => Err(error).with_context(|| format!("create {}", path.display())),
        }
    }
}

fn singleflight_wait_timeout() -> Duration {
    std::env::var("VRT_SINGLEFLIGHT_WAIT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or_else(|| Duration::from_secs(30))
}

fn wait_for_singleflight_evidence(
    root: &Path,
    profile: &ProjectProfile,
    change: &ChangeSet,
    plan: &VerificationPlan,
    timeout: Duration,
) -> Result<Option<EvidenceRecord>> {
    let started = Instant::now();
    loop {
        if let Ok(evidence) = read_latest_evidence(root) {
            if evidence_matches_plan(&evidence, profile, change, plan) {
                return Ok(Some(evidence));
            }
        }
        if !root.join(".vrt/run.lock").exists() {
            return Ok(None);
        }
        if started.elapsed() >= timeout {
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn singleflight_key(
    root: &Path,
    profile: &ProjectProfile,
    change: &ChangeSet,
    plan: &VerificationPlan,
) -> Result<String> {
    let commands = plan
        .steps
        .iter()
        .map(|step| {
            serde_json::json!({
                "capability_id": step.capability_id,
                "command": step.command,
                "cwd": step.cwd,
                "safety_level": step.safety_level
            })
        })
        .collect::<Vec<_>>();
    let value = serde_json::json!({
        "commands": commands,
        "profile_hash": profile.profile_hash,
        "diff_hash": change.diff_hash,
        "base_commit": change.base_commit,
        "lockfile_hash": lockfile_hash(root),
        "config_hash": config_hash(root),
        "toolchain_version": toolchain_version(),
        "relevant_inputs_hash": relevant_inputs_hash(root, profile, change, plan),
        "env_assumptions": env_assumptions()
    });
    let label = plan
        .steps
        .first()
        .map(|step| sanitize(&step.capability_id))
        .unwrap_or_else(|| "empty-plan".to_string());
    Ok(format!(
        "sf_{}_{}",
        label,
        &hash_string(&serde_json::to_string(&value)?)[..24]
    ))
}

fn evidence_matches_plan(
    evidence: &EvidenceRecord,
    profile: &ProjectProfile,
    change: &ChangeSet,
    plan: &VerificationPlan,
) -> bool {
    evidence.plan_id == plan.plan_id
        && evidence.profile_hash == profile.profile_hash
        && evidence.base_commit == change.base_commit
        && evidence.diff_hash == change.diff_hash
}

impl Drop for VerificationRunLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
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

pub fn render_agent_report(root: &Path, evidence: &EvidenceRecord) -> serde_json::Value {
    let explanation = explain_evidence(evidence, root);
    serde_json::json!({
        "schema_version": VRT_SCHEMA_VERSION,
        "status": explanation.status,
        "validity": evidence.validity,
        "evidence_id": evidence.evidence_id,
        "plan_id": evidence.plan_id,
        "session_id": evidence.session_id,
        "base_commit": evidence.base_commit,
        "diff_hash": evidence.diff_hash,
        "profile_hash": evidence.profile_hash,
        "checks_run": evidence.checks.len(),
        "checks_reused": evidence.reused_checks.len(),
        "checks_skipped": evidence.skipped.len(),
        "failure_kind": explanation.failure_kind,
        "root_cause_candidates": explanation.root_cause_candidates,
        "downstream_noise_hidden": explanation.downstream_noise_hidden,
        "recommended_next_action": explanation.recommended_next_action,
        "do_not_run": explanation.do_not_run,
        "raw_log": explanation.raw_log,
        "confidence": evidence.confidence,
        "residual_risks": evidence.confidence.residual_risks,
        "evidence": evidence,
    })
}

pub fn render_token_report(evidence: &EvidenceRecord, profile: TokenProfile) -> String {
    match profile {
        TokenProfile::Standard => human_report(evidence),
        TokenProfile::Rtk => render_rtk_report(evidence),
        TokenProfile::Headroom => render_headroom_report(evidence),
    }
}

fn render_rtk_report(evidence: &EvidenceRecord) -> String {
    let failed = evidence
        .checks
        .iter()
        .filter(|check| check.status == "failed")
        .collect::<Vec<_>>();
    let passed = evidence
        .checks
        .iter()
        .filter(|check| check.status == "passed")
        .count()
        + evidence.reused_checks.len();
    let raw_refs = failed
        .iter()
        .map(|check| check.raw_log.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let failure_summary = failed
        .first()
        .map(|check| check.summary.as_str())
        .unwrap_or("none");
    format!(
        "VRT|status={} evidence={} report={} raw={}\nchecks pass={} fail={} reused={} skipped={}\nconfidence local={} merge={} release={}\nfirst_failure={}\n",
        evidence.validity,
        evidence.evidence_id,
        evidence.report_path,
        if raw_refs.is_empty() { "none" } else { raw_refs.as_str() },
        passed,
        failed.len(),
        evidence.reused_checks.len(),
        evidence.skipped.len(),
        evidence.confidence.local,
        evidence.confidence.merge,
        evidence.confidence.release,
        failure_summary
    )
}

fn render_headroom_report(evidence: &EvidenceRecord) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "tool": "vrt",
        "profile": "headroom",
        "status": evidence.validity,
        "confidence": evidence.confidence,
        "refs": {
            "evidence_id": evidence.evidence_id,
            "evidence": evidence.report_path,
            "raw_log_dir": evidence.raw_log_dir,
            "continued_from": evidence.continued_from
        },
        "checks": evidence.checks.iter().map(|check| {
            serde_json::json!({
                "name": check.name,
                "status": check.status,
                "summary": check.summary,
                "raw_log": check.raw_log,
                "exit_code": check.exit_code
            })
        }).collect::<Vec<_>>(),
        "reused_checks": evidence.reused_checks.iter().map(|check| {
            serde_json::json!({
                "name": check.name,
                "status": check.status,
                "raw_log": check.raw_log
            })
        }).collect::<Vec<_>>(),
        "skipped": evidence.skipped.iter().map(|skip| {
            serde_json::json!({
                "capability_id": skip.capability_id,
                "residual_risk": skip.residual_risk
            })
        }).collect::<Vec<_>>()
    }))
    .unwrap_or_else(|_| "{}".to_string())
}

pub fn token_compatibility_manifest() -> serde_json::Value {
    serde_json::json!({
        "schema_version": VRT_SCHEMA_VERSION,
        "name": "vrt-token-compatibility",
        "purpose": "Keep VRT verification evidence reversible when agent token-saving tools compact visible output.",
        "tools": {
            "rtk": {
                "mode": "cli-proxy",
                "recommended": "rtk vrt verify --token-profile rtk",
                "profile": "rtk",
                "agent_setup": {
                    "codex": "rtk init -g --codex",
                    "claude": "rtk init -g",
                    "cursor": "rtk init -g --agent cursor",
                    "windsurf": "rtk init -g --agent windsurf"
                },
                "entrypoints": [
                    "rtk vrt verify --token-profile rtk",
                    "rtk git status",
                    "rtk cargo test",
                    "rtk npm test"
                ],
                "contract": "Line-oriented output keeps evidence=, report=, raw=, and confidence fields stable for shell output rewriting."
            },
            "headroom": {
                "mode": "structured-context",
                "recommended": "vrt verify --token-profile headroom",
                "profile": "headroom",
                "agent_setup": {
                    "codex": "headroom wrap codex",
                    "claude": "headroom wrap claude",
                    "mcp": "headroom mcp install"
                },
                "entrypoints": [
                    "vrt verify --token-profile headroom",
                    "headroom wrap codex",
                    "headroom proxy --port 8787",
                    "headroom mcp install"
                ],
                "mcp_tools": [
                    "headroom_compress",
                    "headroom_retrieve",
                    "headroom_stats"
                ],
                "contract": "Structured JSON keeps refs.evidence, refs.raw_log_dir, checks[].raw_log, confidence, and skipped residual risk retrievable after compression."
            }
        },
        "commands": {
            "rtk_verify": "rtk vrt verify --token-profile rtk",
            "headroom_verify": "vrt verify --token-profile headroom",
            "doctor": "vrt token doctor --json",
            "install_rules": "vrt token install-rules",
            "manifest": "vrt token manifest --json"
        },
        "preserve": [
            "evidence=",
            "report=",
            "raw=",
            "raw_log",
            "raw_log_dir",
            "refs.evidence",
            "refs.raw_log_dir",
            "checks[].raw_log",
            ".vrt/latest.json",
            ".vrt/evidence"
        ],
        "retrieval": {
            "files": [".vrt/latest.json", ".vrt/evidence/<evidence-id>/evidence.json", ".vrt/evidence/<evidence-id>/*.raw.log"],
            "mcp_resources": ["vrt://latest-evidence", "vrt://token-rules", "vrt://token-compatibility"],
            "broker_ops": ["status", "get_evidence", "explain_failure"]
        },
        "invariants": [
            "Skipped checks are residual risk, never passed checks.",
            "Compact output must preserve evidence identifiers and raw log references.",
            "Agents should retrieve raw logs instead of rerunning broad checks when compact summaries are insufficient."
        ]
    })
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
- If VRT reports `.vrt/run.lock`, another verification is active for this worktree; do not start competing build/test/lint commands.
- If VRT broker state is available, inspect `vrt broker status --json`, `vrt queue status --json`, and `vrt lock list --json` instead of bypassing VRT with competing expensive commands.
- If VRT reports a queued job, do not treat it as failed; wait for the result or report the queue status.
- If VRT reports a resource lock, do not manually run a command that writes to the same resource.
- If evidence is shared or reused, preserve its scope and do not treat local shared evidence as release proof.

## Reporting

When reporting to a user, include checks run, checks skipped, confidence level, residual risk, and the next recommended verification step.
"#
}

pub fn token_rules_markdown() -> &'static str {
    r#"# VRT Token-Saving Compatibility

This repository supports RTK and Headroom alongside VRT.

## RTK

- Install RTK for Codex with `rtk init -g --codex` when using hook-based command rewriting.
- Running `rtk vrt verify --token-profile rtk` is safe and preferred for agent-visible human output.
- RTK may compact stdout, but VRT evidence remains recoverable through `.vrt/latest.json` and `.vrt/evidence/**`.
- Do not compress away `evidence=`, `report=`, `raw=`, `raw_log`, or `.vrt/evidence` references.

## Headroom

- Wrap Codex with `headroom wrap codex`, use `headroom proxy --port 8787` for OpenAI-compatible clients, or install the Headroom MCP bridge with `headroom mcp install`.
- Running `vrt verify --token-profile headroom` emits structured JSON designed for compression and later retrieval.
- If Headroom compression hides detail, retrieve CCR/original context through `headroom_retrieve` or VRT raw log paths instead of rerunning broad checks.
- Headroom may compress summaries, but original logs must remain available at `raw_log` paths.
- Preserve `refs.evidence`, `refs.raw_log_dir`, `checks[].raw_log`, and `confidence`.

## Agent Rule

Do not compress away evidence references. Skipped checks are residual risk, not passed checks. If more detail is needed, retrieve the raw log path rather than rerunning expensive commands.
"#
}

pub fn install_token_rules(root: &Path) -> Result<()> {
    let token_dir = root.join(".vrt/token-saving");
    fs::create_dir_all(&token_dir)?;
    fs::write(token_dir.join("RTK_HEADROOM.md"), token_rules_markdown())?;
    let cursor_dir = root.join(".cursor/rules");
    fs::create_dir_all(&cursor_dir)?;
    fs::write(
        cursor_dir.join("vrt-token-saving.md"),
        token_rules_markdown(),
    )?;
    let windsurf_dir = root.join(".windsurf/rules");
    fs::create_dir_all(&windsurf_dir)?;
    fs::write(
        windsurf_dir.join("vrt-token-saving.md"),
        token_rules_markdown(),
    )?;
    let codex_skill_dir = root.join(".codex/skills/vrt-token-saving");
    fs::create_dir_all(&codex_skill_dir)?;
    fs::write(codex_skill_dir.join("SKILL.md"), token_rules_markdown())?;
    let include = "\n\n# VRT Token-Saving Tools\n\nSee `.vrt/token-saving/RTK_HEADROOM.md` for RTK and Headroom compatibility rules.\n";
    install_token_agent_doc(root.join("AGENTS.md"), include)?;
    install_token_agent_doc(root.join("CLAUDE.md"), include)?;
    install_token_agent_doc(root.join("GEMINI.md"), include)?;
    Ok(())
}

fn install_token_agent_doc(path: PathBuf, include: &str) -> Result<()> {
    if path.exists() {
        let existing = fs::read_to_string(&path)?;
        if !existing.contains(".vrt/token-saving/RTK_HEADROOM.md") {
            fs::write(&path, format!("{existing}{include}"))?;
        }
    } else {
        fs::write(&path, format!("# Agent Instructions{include}"))?;
    }
    Ok(())
}

pub fn install_skill(root: &Path) -> Result<()> {
    let skill_dir = root.join(".vrt/skill");
    fs::create_dir_all(&skill_dir)?;
    fs::write(skill_dir.join("vrt.md"), skill_markdown())?;
    fs::write(skill_dir.join("VRT.md"), skill_markdown())?;
    let cursor_dir = root.join(".cursor/rules");
    fs::create_dir_all(&cursor_dir)?;
    fs::write(cursor_dir.join("vrt.md"), skill_markdown())?;
    let windsurf_dir = root.join(".windsurf/rules");
    fs::create_dir_all(&windsurf_dir)?;
    fs::write(windsurf_dir.join("vrt.md"), skill_markdown())?;
    let codex_skill_dir = root.join(".codex/skills/vrt");
    fs::create_dir_all(&codex_skill_dir)?;
    fs::write(codex_skill_dir.join("SKILL.md"), skill_markdown())?;
    install_agent_doc(root.join("AGENTS.md"))?;
    install_agent_doc(root.join("CLAUDE.md"))?;
    install_agent_doc(root.join("GEMINI.md"))?;
    Ok(())
}

fn install_agent_doc(path: PathBuf) -> Result<()> {
    let section = format!(
        "\n\n# VRT\n\nSee `.vrt/skill/vrt.md` for local verification rules.\n\n{}",
        skill_markdown()
    );
    if path.exists() {
        let existing = fs::read_to_string(&path)?;
        if !existing.contains("VRT Verification Skill") {
            fs::write(&path, format!("{existing}{section}"))?;
        }
    } else {
        fs::write(&path, format!("# Agent Instructions{section}"))?;
    }
    Ok(())
}

pub fn bench_summary(root: &Path) -> Result<serde_json::Value> {
    let latest_path = root.join(".vrt/latest.json");
    let latest: EvidenceRecord = match fs::read_to_string(&latest_path) {
        Ok(text) => serde_json::from_str(&text)?,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(empty_bench_summary(root));
        }
        Err(error) => return Err(error).with_context(|| format!("read {}", latest_path.display())),
    };
    let records = read_all_evidence_records(root)?;
    let skipped_builds = latest
        .skipped
        .iter()
        .filter(|skip| skip.capability_id.contains("build"))
        .count();
    let false_confidence_cases = list_false_confidence_cases(root)?.len();
    let evidence_records = records.len();
    let false_confidence_rate = if evidence_records == 0 {
        0.0
    } else {
        false_confidence_cases as f64 / evidence_records as f64
    };
    let cache_hits = records
        .iter()
        .filter(|record| record.continued_from.is_some())
        .count();
    let reused_checks = records
        .iter()
        .map(|record| record.reused_checks.len())
        .sum::<usize>();
    let evidence_reuse_records = records
        .iter()
        .filter(|record| !record.reused_checks.is_empty())
        .count();
    let evidence_reuse_rate = rate(evidence_reuse_records, evidence_records);
    let cache_hit_rate = rate(cache_hits, evidence_records);
    let reruns_avoided = reused_checks;
    let skipped_expensive_checks_ms = latest
        .skipped
        .iter()
        .map(estimated_saved_time_for_skip)
        .sum::<u64>();
    let evidence_reuse_ms = latest
        .reused_checks
        .iter()
        .map(|check| u64::try_from(check.duration_ms).unwrap_or(u64::MAX).max(1))
        .sum::<u64>();
    let early_failures = records
        .iter()
        .filter(|record| {
            record.validity == "partial"
                && record.checks.iter().any(|check| check.status == "failed")
                && !record.skipped.is_empty()
        })
        .count();
    let ci_failures_shifted_left = early_failures;
    let stale_evidence_detected = records
        .iter()
        .filter(|record| !record.stale_reasons.is_empty())
        .count();
    let log_lines_compressed = records
        .iter()
        .flat_map(|record| record.checks.iter().chain(record.reused_checks.iter()))
        .filter_map(|check| fs::read_to_string(root.join(&check.raw_log)).ok())
        .map(|raw| raw.lines().count() as u64)
        .sum::<u64>();
    let agent_tokens_saved_estimate =
        log_lines_compressed.saturating_mul(8).saturating_mul(60) / 100;
    let queue_wait_time_ms = records
        .iter()
        .map(|record| u64::try_from(record.queue_wait_ms).unwrap_or(u64::MAX))
        .sum::<u64>();
    let lock_wait_time_ms = records
        .iter()
        .map(|record| u64::try_from(record.lock_wait_ms).unwrap_or(u64::MAX))
        .sum::<u64>();
    let singleflight_hits = records
        .iter()
        .filter(|record| record.singleflight.role == "follower")
        .count();
    let shared_evidence_count = records
        .iter()
        .filter(|record| record.singleflight.shared_from_evidence_id.is_some())
        .count();
    let singleflight_saved_time_ms = records
        .iter()
        .filter(|record| record.singleflight.role == "follower")
        .map(|record| u64::try_from(record.duration_ms).unwrap_or(u64::MAX))
        .sum::<u64>();
    let resource_conflicts_avoided = records
        .iter()
        .flat_map(|record| record.resource_locks.iter())
        .filter(|lock| lock.mode == "exclusive" && lock.waited_ms > 0)
        .count();
    let duplicate_commands_avoided = singleflight_hits;
    let session_count = list_session_contexts(root)
        .map(|sessions| sessions.len())
        .unwrap_or(0);
    let runner_pool_utilization = runner_pool_utilization(&records);
    Ok(serde_json::json!({
        "evidence_id": latest.evidence_id,
        "verification_time_ms": latest.duration_ms,
        "full_builds_avoided": skipped_builds,
        "evidence_records": evidence_records,
        "cache_hits": cache_hits,
        "cache_hit_rate": cache_hit_rate,
        "evidence_reuse_rate": evidence_reuse_rate,
        "reused_checks": reused_checks,
        "reruns_avoided": reruns_avoided,
        "early_failures": early_failures,
        "ci_failures_shifted_left": ci_failures_shifted_left,
        "stale_evidence_detected": stale_evidence_detected,
        "log_lines_compressed": log_lines_compressed,
        "agent_tokens_saved_estimate": agent_tokens_saved_estimate,
        "queue_wait_time_ms": queue_wait_time_ms,
        "lock_wait_time_ms": lock_wait_time_ms,
        "singleflight_hits": singleflight_hits,
        "singleflight_saved_time_ms": singleflight_saved_time_ms,
        "resource_conflicts_avoided": resource_conflicts_avoided,
        "duplicate_commands_avoided": duplicate_commands_avoided,
        "runner_pool_utilization": runner_pool_utilization,
        "session_count": session_count,
        "shared_evidence_count": shared_evidence_count,
        "estimated_saved_time_ms": skipped_expensive_checks_ms.saturating_add(evidence_reuse_ms),
        "saved_by": {
            "skipped_expensive_checks_ms": skipped_expensive_checks_ms,
            "evidence_reuse_ms": evidence_reuse_ms,
            "early_failure_ms": 0
        },
        "false_confidence_cases": false_confidence_cases,
        "false_confidence_rate": false_confidence_rate,
        "confidence": latest.confidence,
        "note": "Saved time is estimated conservatively from skipped expensive checks and exact evidence reuse; skipped is not passed."
    }))
}

fn empty_bench_summary(root: &Path) -> serde_json::Value {
    let false_confidence_cases = list_false_confidence_cases(root)
        .map(|cases| cases.len())
        .unwrap_or(0);
    let session_count = list_session_contexts(root)
        .map(|sessions| sessions.len())
        .unwrap_or(0);
    serde_json::json!({
        "evidence_id": serde_json::Value::Null,
        "verification_time_ms": 0,
        "full_builds_avoided": 0,
        "evidence_records": 0,
        "cache_hits": 0,
        "cache_hit_rate": 0.0,
        "evidence_reuse_rate": 0.0,
        "reused_checks": 0,
        "reruns_avoided": 0,
        "early_failures": 0,
        "ci_failures_shifted_left": 0,
        "stale_evidence_detected": 0,
        "log_lines_compressed": 0,
        "agent_tokens_saved_estimate": 0,
        "queue_wait_time_ms": 0,
        "lock_wait_time_ms": 0,
        "singleflight_hits": 0,
        "singleflight_saved_time_ms": 0,
        "resource_conflicts_avoided": 0,
        "duplicate_commands_avoided": 0,
        "runner_pool_utilization": runner_pool_utilization(&[]),
        "session_count": session_count,
        "shared_evidence_count": 0,
        "estimated_saved_time_ms": 0,
        "saved_by": {
            "skipped_expensive_checks_ms": 0,
            "evidence_reuse_ms": 0,
            "early_failure_ms": 0
        },
        "false_confidence_cases": false_confidence_cases,
        "false_confidence_rate": 0.0,
        "confidence": serde_json::json!({}),
        "note": "No VRT evidence has been recorded yet."
    })
}

fn runner_pool_utilization(records: &[EvidenceRecord]) -> serde_json::Value {
    let mut counts = BTreeMap::new();
    for pool in ["cheap", "medium", "expensive", "exclusive"] {
        counts.insert(pool, 0_u64);
    }
    for record in records {
        if let Some(count) = counts.get_mut(record.runner_pool.as_str()) {
            *count += 1;
        }
    }
    let total = records.len() as f64;
    let rate_for = |count: u64| {
        if total == 0.0 {
            0.0
        } else {
            count as f64 / total
        }
    };
    serde_json::json!({
        "cheap": rate_for(*counts.get("cheap").unwrap_or(&0)),
        "medium": rate_for(*counts.get("medium").unwrap_or(&0)),
        "expensive": rate_for(*counts.get("expensive").unwrap_or(&0)),
        "exclusive": rate_for(*counts.get("exclusive").unwrap_or(&0))
    })
}

fn rate(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn estimated_saved_time_for_skip(skip: &SkippedCheck) -> u64 {
    let id = skip.capability_id.as_str();
    if id.contains("e2e") || id.contains("playwright") || id.contains("browser-smoke") {
        180_000
    } else if id.contains("build") {
        120_000
    } else if id.contains("test") {
        30_000
    } else if id.contains("lint") || id.contains("typecheck") {
        10_000
    } else {
        1_000
    }
}

fn runner_pool_for_plan(plan: &VerificationPlan) -> String {
    if plan.steps.iter().any(|step| {
        step.safety_level == "destructive"
            || step.capability_id.contains("e2e")
            || step.capability_id.contains("playwright")
            || step.capability_id.contains("browser-smoke")
    }) {
        "exclusive".to_string()
    } else if plan.steps.iter().any(|step| {
        step.safety_level == "expensive"
            || step.capability_id.contains("build")
            || step.command.contains(" build")
    }) {
        "expensive".to_string()
    } else if plan.steps.len() > 1 {
        "medium".to_string()
    } else {
        "cheap".to_string()
    }
}

fn runner_pool_limit(pool: &str) -> usize {
    match pool {
        "cheap" => 4,
        "medium" => 2,
        "expensive" => 1,
        "exclusive" => 1,
        _ => 1,
    }
}

struct BrokerRunnerPoolSlot {
    path: PathBuf,
}

impl BrokerRunnerPoolSlot {
    fn acquire(root: &Path, job_id: &str, plan: &VerificationPlan) -> Result<Self> {
        let pool = runner_pool_for_plan(plan);
        let limit = runner_pool_limit(&pool);
        let pool_dir = root.join(".vrt/broker/pools").join(&pool);
        fs::create_dir_all(&pool_dir)?;
        let deadline = Instant::now() + broker_lock_wait_timeout();
        loop {
            for slot in 0..limit {
                let path = pool_dir.join(format!("slot-{slot}.lock"));
                match fs::create_dir(&path) {
                    Ok(()) => {
                        write_json(
                            path.join("slot.json"),
                            &serde_json::json!({
                                "schema_version": VRT_SCHEMA_VERSION,
                                "pool": pool,
                                "slot": slot,
                                "job_id": job_id,
                                "plan_id": plan.plan_id,
                                "session_id": plan.session_id,
                                "created_at": Utc::now()
                            }),
                        )?;
                        return Ok(Self { path });
                    }
                    Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                        if broker_pool_slot_is_stale(&path) {
                            let _ = fs::remove_dir_all(&path);
                        }
                    }
                    Err(error) => {
                        return Err(error)
                            .with_context(|| format!("create broker pool slot {}", path.display()))
                    }
                }
            }
            if Instant::now() >= deadline {
                anyhow::bail!("timed out waiting for runner pool {pool}");
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for BrokerRunnerPoolSlot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn broker_pool_slot_is_stale(path: &Path) -> bool {
    let Ok(text) = fs::read_to_string(path.join("slot.json")) else {
        return true;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return true;
    };
    let Some(created_at) = value
        .get("created_at")
        .and_then(|value| value.as_str())
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|time| time.with_timezone(&Utc))
    else {
        return true;
    };
    Utc::now()
        .signed_duration_since(created_at)
        .to_std()
        .map(|age| age > broker_lock_lease())
        .unwrap_or(true)
}

fn resource_locks_for_plan(plan: &VerificationPlan) -> Vec<ResourceLockRecord> {
    resource_locks_for_plan_with_wait(plan, 0)
}

fn resource_locks_for_plan_with_wait(
    plan: &VerificationPlan,
    exclusive_wait_ms: u128,
) -> Vec<ResourceLockRecord> {
    let mut locks = BTreeMap::new();
    locks.insert(
        "source-tree".to_string(),
        ResourceLockRecord {
            resource_id: "source-tree".to_string(),
            kind: "filesystem".to_string(),
            mode: "shared".to_string(),
            reason: "Verification reads the source tree for the current diff scope.".to_string(),
            waited_ms: 0,
        },
    );
    for step in &plan.steps {
        let id = step.capability_id.as_str();
        let command = step.command.as_str();
        if id.contains("build") || command.contains(" next build") || command.ends_with(" build") {
            locks.insert(
                ".next".to_string(),
                ResourceLockRecord {
                    resource_id: ".next".to_string(),
                    kind: "filesystem".to_string(),
                    mode: "exclusive".to_string(),
                    reason: "Production builds write framework build output.".to_string(),
                    waited_ms: exclusive_wait_ms,
                },
            );
        }
        if id.contains("schema-generate")
            || id.contains("schema_generate")
            || command.contains("prisma generate")
        {
            locks.insert(
                "prisma-client".to_string(),
                ResourceLockRecord {
                    resource_id: "prisma-client".to_string(),
                    kind: "generated_artifact".to_string(),
                    mode: "exclusive".to_string(),
                    reason: "Prisma client generation writes shared generated files.".to_string(),
                    waited_ms: exclusive_wait_ms,
                },
            );
        }
        if id.contains("e2e") || id.contains("browser-smoke") || command.contains("playwright") {
            locks.insert(
                "port-3000".to_string(),
                ResourceLockRecord {
                    resource_id: "port-3000".to_string(),
                    kind: "port".to_string(),
                    mode: "exclusive".to_string(),
                    reason: "Browser and e2e checks commonly bind the local app port.".to_string(),
                    waited_ms: exclusive_wait_ms,
                },
            );
            locks.insert(
                "playwright-browser".to_string(),
                ResourceLockRecord {
                    resource_id: "playwright-browser".to_string(),
                    kind: "browser".to_string(),
                    mode: "exclusive".to_string(),
                    reason: "Browser automation should be pooled to avoid local contention."
                        .to_string(),
                    waited_ms: exclusive_wait_ms,
                },
            );
        }
    }
    locks.into_values().collect()
}

struct BrokerResourceLocks {
    paths: Vec<PathBuf>,
}

impl BrokerResourceLocks {
    fn acquire(root: &Path, job_id: &str, plan: &VerificationPlan) -> Result<Self> {
        let mut paths = Vec::new();
        let deadline = Instant::now() + broker_lock_wait_timeout();
        for lock in resource_locks_for_plan(plan)
            .into_iter()
            .filter(|lock| lock.mode == "exclusive")
        {
            let path = broker_lock_path(root, &lock.resource_id);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            loop {
                match fs::create_dir(&path) {
                    Ok(()) => {
                        write_json(
                            path.join("lock.json"),
                            &serde_json::json!({
                                "schema_version": VRT_SCHEMA_VERSION,
                                "resource_id": lock.resource_id,
                                "kind": lock.kind,
                                "mode": lock.mode,
                                "reason": lock.reason,
                                "job_id": job_id,
                                "plan_id": plan.plan_id,
                                "session_id": plan.session_id,
                                "created_at": Utc::now()
                            }),
                        )?;
                        paths.push(path);
                        break;
                    }
                    Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                        if broker_lock_is_stale(&path) {
                            let _ = fs::remove_dir_all(&path);
                            continue;
                        }
                        if Instant::now() >= deadline {
                            anyhow::bail!(
                                "timed out waiting for resource lock {} held at {}",
                                lock.resource_id,
                                path.display()
                            );
                        }
                        thread::sleep(Duration::from_millis(50));
                    }
                    Err(error) => {
                        return Err(error)
                            .with_context(|| format!("create broker lock {}", path.display()))
                    }
                }
            }
        }
        Ok(Self { paths })
    }
}

impl Drop for BrokerResourceLocks {
    fn drop(&mut self) {
        for path in &self.paths {
            let _ = fs::remove_dir_all(path);
        }
    }
}

fn broker_lock_wait_timeout() -> Duration {
    std::env::var("VRT_BROKER_LOCK_WAIT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or_else(|| Duration::from_secs(30))
}

fn broker_lock_lease() -> Duration {
    std::env::var("VRT_BROKER_LOCK_LEASE_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or_else(|| Duration::from_secs(15 * 60))
}

fn broker_lock_is_stale(path: &Path) -> bool {
    let Ok(text) = fs::read_to_string(path.join("lock.json")) else {
        return true;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return true;
    };
    let Some(created_at) = value
        .get("created_at")
        .and_then(|value| value.as_str())
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|time| time.with_timezone(&Utc))
    else {
        return true;
    };
    Utc::now()
        .signed_duration_since(created_at)
        .to_std()
        .map(|age| age > broker_lock_lease())
        .unwrap_or(true)
}

fn broker_lock_path(root: &Path, resource_id: &str) -> PathBuf {
    root.join(".vrt/broker/locks")
        .join(format!("{}.lock", broker_lock_filename(resource_id)))
}

fn broker_lock_filename(resource_id: &str) -> String {
    resource_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

pub fn record_false_confidence_case(
    root: &Path,
    evidence_id: Option<&str>,
    stricter_check: &str,
    failure_summary: &str,
) -> Result<FalseConfidenceCase> {
    let evidence = if let Some(evidence_id) = evidence_id {
        read_evidence(root, evidence_id)?
    } else {
        read_latest_evidence(root)?
    };
    let case = FalseConfidenceCase {
        schema_version: VRT_SCHEMA_VERSION,
        case_id: format!("fc_{}", Uuid::new_v4().simple()),
        recorded_at: Utc::now(),
        evidence_id: evidence.evidence_id.clone(),
        stricter_check: stricter_check.trim().to_string(),
        failure_summary: failure_summary.trim().to_string(),
        previous_confidence: evidence.confidence.clone(),
        previous_validity: evidence.validity.clone(),
        diff_hash: evidence.diff_hash.clone(),
        profile_hash: evidence.profile_hash.clone(),
        notes: vec![
            "False confidence means a stricter verification later failed for a reason the earlier confidence should have covered.".to_string(),
            "This record is corrective evidence; do not delete the original evidence record.".to_string(),
        ],
    };
    if case.stricter_check.is_empty() {
        anyhow::bail!("stricter check must not be empty");
    }
    if case.failure_summary.is_empty() {
        anyhow::bail!("failure summary must not be empty");
    }
    let ledger = root.join(".vrt/false-confidence.jsonl");
    if let Some(parent) = ledger.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(&ledger)?;
    writeln!(file, "{}", serde_json::to_string(&case)?)?;
    Ok(case)
}

pub fn list_false_confidence_cases(root: &Path) -> Result<Vec<FalseConfidenceCase>> {
    let path = root.join(".vrt/false-confidence.jsonl");
    if !path.exists() {
        return Ok(vec![]);
    }
    let mut cases = Vec::new();
    for (index, line) in fs::read_to_string(&path)?.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let case = serde_json::from_str(line)
            .with_context(|| format!("parse false confidence case at line {}", index + 1))?;
        cases.push(case);
    }
    Ok(cases)
}

pub fn start_worktree_session(
    root: &Path,
    worktree_path: &Path,
    branch: Option<&str>,
) -> Result<WorktreeSession> {
    let session_id = format!("session_{}", Uuid::new_v4().simple());
    let branch = branch
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("vrt/{}", &session_id["session_".len()..]));
    if branch.contains(' ') || branch.contains("..") || branch.starts_with('-') {
        anyhow::bail!("invalid worktree branch name");
    }
    let base_commit = git_output(root, ["rev-parse", "HEAD"]).unwrap_or_else(|_| "unknown".into());
    let output = Command::new("git")
        .args(["worktree", "add", "-b", &branch])
        .arg(worktree_path)
        .arg("HEAD")
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "git worktree add failed: {}{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
    }
    let worktree_path = worktree_path
        .canonicalize()
        .unwrap_or_else(|_| worktree_path.to_path_buf());
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let session = WorktreeSession {
        schema_version: VRT_SCHEMA_VERSION,
        session_id: session_id.clone(),
        created_at: Utc::now(),
        root: root.to_string_lossy().to_string(),
        worktree_path: worktree_path.to_string_lossy().to_string(),
        branch,
        base_commit: base_commit.trim().to_string(),
        status: "active".to_string(),
        instructions: vec![
            format!("cd {}", worktree_path.display()),
            format!("export VRT_SESSION_ID={session_id}"),
            "run vrt verify --json from this worktree".to_string(),
        ],
    };
    write_session_metadata(&root, &session)?;
    write_session_metadata(&worktree_path, &session)?;
    Ok(session)
}

pub fn list_worktree_sessions(root: &Path) -> Result<Vec<WorktreeSession>> {
    let sessions_dir = root.join(".vrt/sessions");
    if !sessions_dir.exists() {
        return Ok(vec![]);
    }
    let mut sessions = Vec::new();
    for entry in fs::read_dir(sessions_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext == "json")
        {
            let session: WorktreeSession = serde_json::from_str(&fs::read_to_string(entry.path())?)
                .with_context(|| format!("parse {}", entry.path().display()))?;
            sessions.push(session);
        }
    }
    sessions.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok(sessions)
}

pub fn current_worktree_session(root: &Path) -> Result<WorktreeSession> {
    let session_path = root.join(".vrt/session.json");
    if session_path.exists() {
        return serde_json::from_str(&fs::read_to_string(&session_path)?)
            .with_context(|| format!("parse {}", session_path.display()));
    }
    let root_path = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let base_commit = git_output(root, ["rev-parse", "HEAD"]).unwrap_or_else(|_| "unknown".into());
    let branch = git_output(root, ["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_else(|_| "unknown".into());
    Ok(WorktreeSession {
        schema_version: VRT_SCHEMA_VERSION,
        session_id: std::env::var("VRT_SESSION_ID").unwrap_or_else(|_| "unmanaged".to_string()),
        created_at: Utc::now(),
        root: root_path.to_string_lossy().to_string(),
        worktree_path: root_path.to_string_lossy().to_string(),
        branch: branch.trim().to_string(),
        base_commit: base_commit.trim().to_string(),
        status: "unmanaged".to_string(),
        instructions: vec![
            "run vrt session start --worktree <path> to create an isolated Agent worktree"
                .to_string(),
        ],
    })
}

pub fn multi_agent_session_view(root: &Path) -> Result<MultiAgentSessionView> {
    let root_path = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut sessions = Vec::new();
    for session in list_worktree_sessions(root)? {
        let worktree = PathBuf::from(&session.worktree_path);
        let latest_evidence =
            read_latest_evidence(&worktree)
                .ok()
                .map(|evidence| SessionEvidenceSummary {
                    evidence_id: evidence.evidence_id,
                    validity: evidence.validity,
                    finished_at: evidence.finished_at,
                    checks_failed: evidence
                        .checks
                        .iter()
                        .filter(|check| check.status == "failed")
                        .count(),
                    checks_run: evidence.checks.len(),
                    checks_reused: evidence.reused_checks.len(),
                    checks_skipped: evidence.skipped.len(),
                    confidence: evidence.confidence,
                    diff_hash: evidence.diff_hash,
                    profile_hash: evidence.profile_hash,
                    report_path: evidence.report_path,
                });
        let active_lock = read_run_lock(&worktree);
        let false_confidence_cases = list_false_confidence_cases(&worktree)
            .map(|cases| cases.len())
            .unwrap_or(0);
        sessions.push(WorktreeSessionView {
            session,
            latest_evidence,
            active_lock,
            false_confidence_cases,
        });
    }
    Ok(MultiAgentSessionView {
        schema_version: VRT_SCHEMA_VERSION,
        generated_at: Utc::now(),
        root: root_path.to_string_lossy().to_string(),
        sessions,
    })
}

pub fn list_session_contexts(root: &Path) -> Result<Vec<SessionContext>> {
    let sessions_dir = session_registry_dir(root);
    if !sessions_dir.exists() {
        return Ok(vec![]);
    }
    let mut sessions = Vec::new();
    for entry in fs::read_dir(sessions_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext == "json")
        {
            let session: SessionContext = serde_json::from_str(&fs::read_to_string(entry.path())?)
                .with_context(|| format!("parse {}", entry.path().display()))?;
            sessions.push(session);
        }
    }
    sessions.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok(sessions)
}

pub fn show_session_context(root: &Path, session_id: &str) -> Result<SessionContext> {
    let path = session_context_path(root, session_id);
    serde_json::from_str(&fs::read_to_string(&path)?)
        .with_context(|| format!("parse {}", path.display()))
}

pub fn close_session_context(root: &Path, session_id: &str) -> Result<SessionContext> {
    let mut session = show_session_context(root, session_id)?;
    session.status = "closed".to_string();
    session.last_seen_at = Utc::now();
    write_json(session_context_path(root, session_id), &session)?;
    Ok(session)
}

fn register_session_context(
    root: &Path,
    change: &ChangeSet,
    plan: &VerificationPlan,
    evidence: &EvidenceRecord,
) -> Result<()> {
    fs::create_dir_all(session_registry_dir(root))?;
    let now = Utc::now();
    let path = session_context_path(root, &plan.session_id);
    let existing = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<SessionContext>(&text).ok());
    let root_path = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let current_head = git_output(root, ["rev-parse", "HEAD"]).unwrap_or_else(|_| "unknown".into());
    let branch = git_output(root, ["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_else(|_| "unknown".into());
    let worktree = session_worktree_context(root, branch.trim());
    let status = existing
        .as_ref()
        .filter(|session| session.status == "closed")
        .map(|session| session.status.clone())
        .unwrap_or_else(|| "active".to_string());
    let session = SessionContext {
        schema_version: VRT_SCHEMA_VERSION,
        session_id: plan.session_id.clone(),
        name: existing.and_then(|session| session.name),
        agent_kind: std::env::var("VRT_AGENT_KIND").unwrap_or_else(|_| "unknown".to_string()),
        repo_root: root_path.to_string_lossy().to_string(),
        working_dir: root_path.to_string_lossy().to_string(),
        worktree,
        base_commit: change.base_commit.clone(),
        current_head: current_head.trim().to_string(),
        diff_hash: change.diff_hash.clone(),
        dirty_state: if dirty_state(root).is_dirty {
            "dirty".to_string()
        } else {
            "clean".to_string()
        },
        created_at: show_session_context(root, &plan.session_id)
            .map(|session| session.created_at)
            .unwrap_or(now),
        last_seen_at: now,
        status,
        last_evidence_id: Some(evidence.evidence_id.clone()),
    };
    write_json(path, &session)
}

fn session_worktree_context(root: &Path, branch: &str) -> SessionWorktreeContext {
    let root_path = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if let Ok(session) = current_worktree_session(root) {
        let worktree_path = PathBuf::from(&session.worktree_path);
        let worktree_path = worktree_path
            .canonicalize()
            .unwrap_or_else(|_| worktree_path.to_path_buf());
        let managed_by_vrt = root.join(".vrt/session.json").exists();
        let enabled = managed_by_vrt && worktree_path == root_path;
        return SessionWorktreeContext {
            enabled,
            path: enabled.then(|| worktree_path.to_string_lossy().to_string()),
            branch: Some(session.branch),
            managed_by_vrt: enabled,
        };
    }
    SessionWorktreeContext {
        enabled: false,
        path: None,
        branch: Some(branch.to_string()),
        managed_by_vrt: false,
    }
}

fn session_registry_dir(root: &Path) -> PathBuf {
    root.join(".vrt/session-registry")
}

fn session_context_path(root: &Path, session_id: &str) -> PathBuf {
    session_registry_dir(root).join(format!("{}.json", sanitize(session_id)))
}

fn write_session_metadata(root: &Path, session: &WorktreeSession) -> Result<()> {
    fs::create_dir_all(root.join(".vrt/sessions"))?;
    write_json(root.join(".vrt/session.json"), session)?;
    write_json(
        root.join(".vrt/sessions")
            .join(format!("{}.json", session.session_id)),
        session,
    )?;
    Ok(())
}

fn read_run_lock(root: &Path) -> Option<serde_json::Value> {
    fs::read_to_string(root.join(".vrt/run.lock/lock.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
}

pub fn export_report(root: &Path, format: ReportFormat, output: &Path) -> Result<()> {
    let evidence = read_latest_evidence(root)?;
    let rendered = match format {
        ReportFormat::Markdown => render_markdown_report(&evidence),
        ReportFormat::Sarif => serde_json::to_string_pretty(&render_sarif(&evidence))?,
        ReportFormat::Junit => render_junit(&evidence),
        ReportFormat::Otel => serde_json::to_string_pretty(&render_otel_trace(&evidence))?,
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
    Markdown,
    Sarif,
    Junit,
    Otel,
}

pub fn render_markdown_report(evidence: &EvidenceRecord) -> String {
    let mut out = String::new();
    out.push_str("## VRT Verification Report\n\n");
    out.push_str(&format!("- Evidence: `{}`\n", evidence.evidence_id));
    out.push_str(&format!("- Status: `{}`\n", evidence.validity));
    out.push_str(&format!("- Session: `{}`\n", evidence.session_id));
    out.push_str(&format!("- Plan: `{}`\n", evidence.plan_id));
    out.push_str(&format!("- Diff: `{}`\n", evidence.diff_hash));
    out.push_str(&format!("- Profile: `{}`\n", evidence.profile_hash));
    out.push_str(&format!(
        "- Report: `{}`\n- Raw logs: `{}`\n\n",
        evidence.report_path, evidence.raw_log_dir
    ));

    out.push_str("### Confidence\n\n");
    out.push_str(&format!(
        "- local: {}\n- merge: {}\n- release: {}\n\n",
        evidence.confidence.local, evidence.confidence.merge, evidence.confidence.release
    ));

    out.push_str("### Checks\n\n");
    if evidence.reused_checks.is_empty() && evidence.checks.is_empty() {
        out.push_str("- No checks ran.\n");
    }
    for check in &evidence.reused_checks {
        out.push_str(&format!(
            "- `{}`: reused\n  - safety: {}\n  - raw log: `{}`\n",
            check.name, check.safety_level, check.raw_log
        ));
    }
    for check in &evidence.checks {
        out.push_str(&format!(
            "- `{}`: {}\n  - safety: {}\n  - command: `{}`\n  - summary: {}\n  - raw log: `{}`\n",
            check.name,
            check.status,
            check.safety_level,
            markdown_inline_code(&check.command),
            check.summary,
            check.raw_log
        ));
    }
    out.push('\n');

    out.push_str("### Skipped Checks\n\n");
    if evidence.skipped.is_empty() {
        out.push_str("- None.\n");
    } else {
        for skipped in &evidence.skipped {
            out.push_str(&format!(
                "- `{}`: {}\n  - residual risk: {}\n",
                skipped.capability_id, skipped.reason, skipped.residual_risk
            ));
        }
    }

    if !evidence.stale_reasons.is_empty() {
        out.push_str("\n### Stale Evidence Notes\n\n");
        for reason in &evidence.stale_reasons {
            out.push_str(&format!("- {}\n", reason));
        }
    }

    out
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

pub fn render_otel_trace(evidence: &EvidenceRecord) -> serde_json::Value {
    let trace_id = trace_id(evidence);
    let root_span_id = span_id(&format!("{}:root", evidence.evidence_id));
    let mut spans = Vec::new();
    spans.push(serde_json::json!({
        "traceId": trace_id,
        "spanId": root_span_id,
        "name": "vrt.verify",
        "kind": 1,
        "startTimeUnixNano": unix_nanos(evidence.started_at),
        "endTimeUnixNano": unix_nanos(evidence.finished_at),
        "status": otel_status(&evidence.validity),
        "attributes": [
            otel_attr("vrt.schema_version", VRT_SCHEMA_VERSION),
            otel_attr("vrt.evidence_id", &evidence.evidence_id),
            otel_attr("vrt.plan_id", &evidence.plan_id),
            otel_attr("vrt.session_id", &evidence.session_id),
            otel_attr("vrt.change_id", &evidence.change_id),
            otel_attr("vrt.diff_hash", &evidence.diff_hash),
            otel_attr("vrt.profile_hash", &evidence.profile_hash),
            otel_attr("vrt.validity", &evidence.validity),
            otel_attr("vrt.confidence.local", &evidence.confidence.local),
            otel_attr("vrt.confidence.merge", &evidence.confidence.merge),
            otel_attr("vrt.confidence.release", &evidence.confidence.release),
            otel_attr("vrt.checks.run", evidence.checks.len() as u64),
            otel_attr("vrt.checks.reused", evidence.reused_checks.len() as u64),
            otel_attr("vrt.checks.skipped", evidence.skipped.len() as u64),
            otel_attr("vrt.raw_log_dir", &evidence.raw_log_dir),
            otel_attr("vrt.report_path", &evidence.report_path),
        ]
    }));

    let mut child_index = 0_u64;
    for check in evidence.reused_checks.iter().chain(evidence.checks.iter()) {
        child_index += 1;
        spans.push(serde_json::json!({
            "traceId": trace_id,
            "spanId": span_id(&format!("{}:check:{child_index}:{}", evidence.evidence_id, check.name)),
            "parentSpanId": root_span_id,
            "name": format!("vrt.check.{}", check.name),
            "kind": 1,
            "startTimeUnixNano": unix_nanos(evidence.started_at),
            "endTimeUnixNano": unix_nanos(evidence.finished_at),
            "status": otel_status(&check.status),
            "attributes": [
                otel_attr("vrt.check.name", &check.name),
                otel_attr("vrt.check.command", &check.command),
                otel_attr("vrt.check.status", &check.status),
                otel_attr("vrt.check.exit_code", check.exit_code.map(i64::from)),
                otel_attr("vrt.check.duration_ms", check.duration_ms as u64),
                otel_attr("vrt.check.raw_log", &check.raw_log),
                otel_attr("vrt.check.summary", &check.summary),
            ]
        }));
    }

    for skipped in &evidence.skipped {
        child_index += 1;
        spans.push(serde_json::json!({
            "traceId": trace_id,
            "spanId": span_id(&format!("{}:skipped:{child_index}:{}", evidence.evidence_id, skipped.capability_id)),
            "parentSpanId": root_span_id,
            "name": format!("vrt.skipped.{}", skipped.capability_id),
            "kind": 1,
            "startTimeUnixNano": unix_nanos(evidence.finished_at),
            "endTimeUnixNano": unix_nanos(evidence.finished_at),
            "status": {
                "code": 1,
                "message": "skipped is residual risk, not passed"
            },
            "attributes": [
                otel_attr("vrt.skipped.capability_id", &skipped.capability_id),
                otel_attr("vrt.skipped.reason", &skipped.reason),
                otel_attr("vrt.skipped.residual_risk", &skipped.residual_risk),
            ]
        }));
    }

    serde_json::json!({
        "resourceSpans": [
            {
                "resource": {
                    "attributes": [
                        otel_attr("service.name", "vrt"),
                        otel_attr("service.version", env!("CARGO_PKG_VERSION")),
                        otel_attr("telemetry.sdk.language", "rust"),
                        otel_attr("vrt.schema_version", VRT_SCHEMA_VERSION),
                    ]
                },
                "scopeSpans": [
                    {
                        "scope": {
                            "name": "vrt",
                            "version": env!("CARGO_PKG_VERSION")
                        },
                        "spans": spans
                    }
                ]
            }
        ]
    })
}

enum OtelAttrValue {
    String(String),
    Int(i64),
}

fn otel_attr(key: &str, value: impl Into<OtelAttrValue>) -> serde_json::Value {
    let value = match value.into() {
        OtelAttrValue::String(value) => serde_json::json!({ "stringValue": value }),
        OtelAttrValue::Int(value) => serde_json::json!({ "intValue": value }),
    };
    serde_json::json!({
        "key": key,
        "value": value
    })
}

impl From<&str> for OtelAttrValue {
    fn from(value: &str) -> Self {
        OtelAttrValue::String(value.to_string())
    }
}

impl From<&String> for OtelAttrValue {
    fn from(value: &String) -> Self {
        OtelAttrValue::String(value.clone())
    }
}

impl From<String> for OtelAttrValue {
    fn from(value: String) -> Self {
        OtelAttrValue::String(value)
    }
}

impl From<u32> for OtelAttrValue {
    fn from(value: u32) -> Self {
        OtelAttrValue::Int(i64::from(value))
    }
}

impl From<u64> for OtelAttrValue {
    fn from(value: u64) -> Self {
        OtelAttrValue::Int(value as i64)
    }
}

impl From<i64> for OtelAttrValue {
    fn from(value: i64) -> Self {
        OtelAttrValue::Int(value)
    }
}

impl From<Option<i64>> for OtelAttrValue {
    fn from(value: Option<i64>) -> Self {
        value
            .map(OtelAttrValue::Int)
            .unwrap_or_else(|| OtelAttrValue::String("null".to_string()))
    }
}

fn trace_id(evidence: &EvidenceRecord) -> String {
    hash_string(&format!(
        "{}:{}:{}",
        evidence.evidence_id, evidence.diff_hash, evidence.profile_hash
    ))[..32]
        .to_string()
}

fn span_id(seed: &str) -> String {
    hash_string(seed)[..16].to_string()
}

fn unix_nanos(time: DateTime<Utc>) -> String {
    time.timestamp_nanos_opt()
        .unwrap_or_else(|| time.timestamp_micros() * 1_000)
        .to_string()
}

fn otel_status(status: &str) -> serde_json::Value {
    match status {
        "passed" | "valid" | "reused" => serde_json::json!({
            "code": 1,
            "message": status
        }),
        "partial" | "failed" | "invalid" => serde_json::json!({
            "code": 2,
            "message": status
        }),
        other => serde_json::json!({
            "code": 0,
            "message": other
        }),
    }
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
                    },
                    "resources": {
                        "subscribe": false,
                        "listChanged": false
                    },
                    "prompts": {
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
        "resources/list" => jsonrpc_result(
            id,
            serde_json::json!({
                "resources": mcp_resources()
            }),
        ),
        "resources/read" => {
            let params = request
                .get("params")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let uri = params
                .get("uri")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            match read_mcp_resource(root, uri) {
                Ok(result) => jsonrpc_result(id, result),
                Err(error) => jsonrpc_error(id, -32602, error.to_string()),
            }
        }
        "prompts/list" => jsonrpc_result(
            id,
            serde_json::json!({
                "prompts": mcp_prompts()
            }),
        ),
        "prompts/get" => {
            let params = request
                .get("params")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let name = params
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            match get_mcp_prompt(name, &arguments) {
                Ok(result) => jsonrpc_result(id, result),
                Err(error) => jsonrpc_error(id, -32602, error.to_string()),
            }
        }
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

pub fn broker_status(root: &Path) -> serde_json::Value {
    let latest_evidence = read_latest_evidence(root).ok().map(|evidence| {
        serde_json::json!({
            "evidence_id": evidence.evidence_id,
            "validity": evidence.validity,
            "finished_at": evidence.finished_at,
            "confidence": evidence.confidence,
            "checks_run": evidence.checks.len(),
            "checks_failed": evidence.checks.iter().filter(|check| check.status == "failed").count(),
            "checks_skipped": evidence.skipped.len(),
            "diff_hash": evidence.diff_hash,
            "profile_hash": evidence.profile_hash,
        })
    });
    serde_json::json!({
        "schema_version": VRT_SCHEMA_VERSION,
        "protocol": "vrt-broker/1",
        "root": root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
        "pid": std::process::id(),
        "generated_at": Utc::now(),
        "broker_state": broker_runtime_state(root),
        "active_lock": read_run_lock(root),
        "latest_evidence": latest_evidence,
        "session_view": multi_agent_session_view(root).ok(),
        "tools": [
            "status",
            "analyze_change",
            "plan_verification",
            "run_verification",
            "explain_failure",
            "get_evidence",
            "escalate_verification",
            "session_view"
        ],
        "forbidden": ["run_any_shell_command"]
    })
}

pub fn start_broker(root: &Path) -> Result<serde_json::Value> {
    fs::create_dir_all(root.join(".vrt/broker"))?;
    let state = broker_state_value(root, true);
    write_json(root.join(".vrt/broker/state.json"), &state)?;
    Ok(state)
}

pub fn stop_broker(root: &Path) -> Result<serde_json::Value> {
    fs::create_dir_all(root.join(".vrt/broker"))?;
    let mut state = broker_state_value(root, false);
    state["stopped_at"] = serde_json::json!(Utc::now());
    write_json(root.join(".vrt/broker/state.json"), &state)?;
    Ok(state)
}

pub fn queue_status(root: &Path) -> serde_json::Value {
    let state = broker_runtime_state(root);
    let jobs = broker_job_entries(root);
    let queued_jobs = jobs
        .iter()
        .filter(|(_, job)| job_status(job) == Some("queued"))
        .count() as u64;
    let running_jobs = jobs
        .iter()
        .filter(|(_, job)| job_status(job) == Some("running"))
        .count() as u64;
    let cancelled_jobs = jobs
        .iter()
        .filter(|(_, job)| job_status(job) == Some("cancelled"))
        .count() as u64;
    let waiting = jobs
        .iter()
        .filter(|(_, job)| job_status(job) == Some("queued"))
        .map(|(_, job)| broker_queue_item(job))
        .collect::<Vec<_>>();
    let running = jobs
        .iter()
        .filter(|(_, job)| job_status(job) == Some("running"))
        .map(|(_, job)| broker_queue_item(job))
        .collect::<Vec<_>>();
    serde_json::json!({
        "schema_version": VRT_SCHEMA_VERSION,
        "queued_jobs": if jobs.is_empty() { state["queue"]["queued_jobs"].as_u64().unwrap_or(0) } else { queued_jobs },
        "running_jobs": if jobs.is_empty() { state["queue"]["running_jobs"].as_u64().unwrap_or(0) } else { running_jobs },
        "cancelled_jobs": if jobs.is_empty() { state["queue"]["cancelled_jobs"].as_u64().unwrap_or(0) } else { cancelled_jobs },
        "waiting": waiting,
        "running": running,
        "runner_pool": state["runner_pool"].clone(),
        "note": "Queue status is repo-local; direct verification falls back when no broker is running."
    })
}

pub fn cancel_queue_job(root: &Path, job_id: &str) -> Result<serde_json::Value> {
    if job_id.trim().is_empty() {
        anyhow::bail!("job id must not be empty");
    }
    for (path, mut job) in broker_job_entries(root) {
        if job["job_id"] != job_id {
            continue;
        }
        let status = job_status(&job).unwrap_or("unknown");
        if status != "queued" {
            return Ok(serde_json::json!({
                "schema_version": VRT_SCHEMA_VERSION,
                "job_id": job_id,
                "status": "not_cancelled",
                "job_status": status,
                "queue": queue_status(root),
                "message": "Only queued broker jobs can be cancelled."
            }));
        }
        job["status"] = serde_json::json!("cancelled");
        job["cancelled_at"] = serde_json::json!(Utc::now());
        job["updated_at"] = serde_json::json!(Utc::now());
        write_json(path, &job)?;
        return Ok(serde_json::json!({
            "schema_version": VRT_SCHEMA_VERSION,
            "job_id": job_id,
            "status": "cancelled",
            "queue": queue_status(root)
        }));
    }
    Ok(serde_json::json!({
        "schema_version": VRT_SCHEMA_VERSION,
        "job_id": job_id,
        "status": "not_found",
        "queue": queue_status(root),
        "message": "No queued broker job with this id was found in the repo-local queue state."
    }))
}

fn broker_job_entries(root: &Path) -> Vec<(PathBuf, serde_json::Value)> {
    let jobs_dir = root.join(".vrt/broker/jobs");
    let Ok(entries) = fs::read_dir(&jobs_dir) else {
        return vec![];
    };
    let mut jobs = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
        })
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("json"))
        .filter_map(|entry| {
            let path = entry.path();
            let job = fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())?;
            Some((path, job))
        })
        .collect::<Vec<_>>();
    jobs.sort_by(|(_, left), (_, right)| {
        let left_key = format!(
            "{}:{}",
            left.get("created_at")
                .and_then(|value| value.as_str())
                .unwrap_or(""),
            left.get("job_id")
                .and_then(|value| value.as_str())
                .unwrap_or("")
        );
        let right_key = format!(
            "{}:{}",
            right
                .get("created_at")
                .and_then(|value| value.as_str())
                .unwrap_or(""),
            right
                .get("job_id")
                .and_then(|value| value.as_str())
                .unwrap_or("")
        );
        left_key.cmp(&right_key)
    });
    jobs
}

fn job_status(job: &serde_json::Value) -> Option<&str> {
    job.get("status").and_then(|value| value.as_str())
}

fn broker_queue_item(job: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "job_id": job.get("job_id").cloned().unwrap_or(serde_json::Value::Null),
        "session_id": job.get("session_id").cloned().unwrap_or(serde_json::Value::Null),
        "plan_id": job.get("plan_id").cloned().unwrap_or(serde_json::Value::Null),
        "status": job.get("status").cloned().unwrap_or(serde_json::Value::Null),
        "cost": job.get("cost").cloned().unwrap_or(serde_json::Value::Null),
        "required_resources": job.get("required_resources").cloned().unwrap_or_else(|| serde_json::json!([])),
        "created_at": job.get("created_at").cloned().unwrap_or(serde_json::Value::Null),
        "updated_at": job.get("updated_at").cloned().unwrap_or(serde_json::Value::Null)
    })
}

pub fn lock_list(root: &Path) -> serde_json::Value {
    let mut locks = active_broker_locks(root);
    let observed = read_latest_evidence(root)
        .ok()
        .map(|evidence| {
            evidence
                .resource_locks
                .into_iter()
                .map(|lock| {
                    serde_json::json!({
                        "resource_id": lock.resource_id,
                        "kind": lock.kind,
                        "mode": lock.mode,
                        "reason": lock.reason,
                        "waited_ms": lock.waited_ms,
                        "status": "observed"
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    locks.extend(observed);
    let held = locks.iter().filter(|lock| lock["status"] == "held").count();
    serde_json::json!({
        "schema_version": VRT_SCHEMA_VERSION,
        "held": held,
        "waiting": 0,
        "locks": locks
    })
}

fn active_broker_locks(root: &Path) -> Vec<serde_json::Value> {
    let locks_dir = root.join(".vrt/broker/locks");
    let Ok(entries) = fs::read_dir(&locks_dir) else {
        return vec![];
    };
    entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
        .filter_map(|entry| {
            let path = entry.path();
            if broker_lock_is_stale(&path) {
                let _ = fs::remove_dir_all(&path);
                return None;
            }
            let value = fs::read_to_string(path.join("lock.json"))
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())?;
            Some(serde_json::json!({
                "resource_id": value.get("resource_id").cloned().unwrap_or_else(|| serde_json::json!("unknown")),
                "kind": value.get("kind").cloned().unwrap_or_else(|| serde_json::json!("unknown")),
                "mode": value.get("mode").cloned().unwrap_or_else(|| serde_json::json!("exclusive")),
                "reason": value.get("reason").cloned().unwrap_or_else(|| serde_json::json!("broker resource lock")),
                "waited_ms": 0,
                "status": "held",
                "job_id": value.get("job_id").cloned().unwrap_or(serde_json::Value::Null),
                "created_at": value.get("created_at").cloned().unwrap_or(serde_json::Value::Null)
            }))
        })
        .collect()
}

pub fn lock_show(root: &Path, lock_id: &str) -> Result<serde_json::Value> {
    let locks = lock_list(root);
    let Some(lock) = locks["locks"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|lock| lock["resource_id"] == lock_id)
    else {
        anyhow::bail!("lock not found: {lock_id}");
    };
    Ok(lock.clone())
}

fn broker_runtime_state(root: &Path) -> serde_json::Value {
    let path = root.join(".vrt/broker/state.json");
    let mut state = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| broker_state_value(root, false));
    state["runner_pool"] = runner_pool_status(root);
    state
}

fn broker_state_value(root: &Path, running: bool) -> serde_json::Value {
    let session_count = list_session_contexts(root)
        .map(|sessions| sessions.len())
        .unwrap_or(0);
    serde_json::json!({
        "schema_version": VRT_SCHEMA_VERSION,
        "running": running,
        "mode": "repo-local",
        "root": root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
        "socket_path": root.join(".vrt/broker/vrt.sock"),
        "started_at": Utc::now(),
        "capabilities": [
            "session_registry",
            "verification_queue",
            "resource_locks",
            "singleflight",
            "runner_pool",
            "evidence_ledger"
        ],
        "sessions": {
            "active": session_count
        },
        "queue": {
            "queued_jobs": 0,
            "running_jobs": 0,
            "cancelled_jobs": 0
        },
        "locks": {
            "held": 0,
            "waiting": 0,
            "stale": 0
        },
        "runner_pool": runner_pool_status(root)
    })
}

fn runner_pool_status(root: &Path) -> serde_json::Value {
    let mut pools = serde_json::Map::new();
    for pool in ["cheap", "medium", "expensive", "exclusive"] {
        pools.insert(
            pool.to_string(),
            serde_json::json!({
                "limit": runner_pool_limit(pool),
                "running": active_runner_pool_slot_count(root, pool)
            }),
        );
    }
    serde_json::Value::Object(pools)
}

fn active_runner_pool_slot_count(root: &Path, pool: &str) -> usize {
    let pool_dir = root.join(".vrt/broker/pools").join(pool);
    let Ok(entries) = fs::read_dir(&pool_dir) else {
        return 0;
    };
    entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
        .filter(|entry| {
            let path = entry.path();
            if broker_pool_slot_is_stale(&path) {
                let _ = fs::remove_dir_all(&path);
                false
            } else {
                true
            }
        })
        .count()
}

fn write_broker_job(
    root: &Path,
    job_id: &str,
    plan: &VerificationPlan,
    status: &str,
    evidence: Option<&EvidenceRecord>,
) -> Result<()> {
    let jobs_dir = root.join(".vrt/broker/jobs");
    fs::create_dir_all(&jobs_dir)?;
    let value = serde_json::json!({
        "schema_version": VRT_SCHEMA_VERSION,
        "job_id": job_id,
        "session_id": plan.session_id,
        "plan_id": plan.plan_id,
        "mode": plan.mode,
        "priority": "normal",
        "cost": runner_pool_for_plan(plan),
        "status": status,
        "required_resources": resource_locks_for_plan(plan),
        "created_at": Utc::now(),
        "updated_at": Utc::now(),
        "evidence_id": evidence.map(|record| record.evidence_id.clone()),
        "queue_wait_ms": evidence.map(|record| record.queue_wait_ms).unwrap_or(0),
        "lock_wait_ms": evidence.map(|record| record.lock_wait_ms).unwrap_or(0),
        "singleflight": evidence.map(|record| record.singleflight.clone())
    });
    write_json(jobs_dir.join(format!("{job_id}.json")), &value)
}

fn write_broker_job_error(
    root: &Path,
    job_id: &str,
    plan: &VerificationPlan,
    error: String,
) -> Result<()> {
    let jobs_dir = root.join(".vrt/broker/jobs");
    fs::create_dir_all(&jobs_dir)?;
    write_json(
        jobs_dir.join(format!("{job_id}.json")),
        &serde_json::json!({
            "schema_version": VRT_SCHEMA_VERSION,
            "job_id": job_id,
            "session_id": plan.session_id,
            "plan_id": plan.plan_id,
            "mode": plan.mode,
            "priority": "normal",
            "cost": runner_pool_for_plan(plan),
            "status": "failed",
            "required_resources": resource_locks_for_plan(plan),
            "created_at": Utc::now(),
            "updated_at": Utc::now(),
            "error": error
        }),
    )
}

pub fn handle_broker_message(root: &Path, line: &str) -> Result<Option<String>> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let request: serde_json::Value = serde_json::from_str(trimmed).context("parse broker json")?;
    let id = request
        .get("id")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let op = request
        .get("op")
        .or_else(|| request.get("method"))
        .and_then(|value| value.as_str())
        .unwrap_or("status");
    let mut arguments = request
        .get("arguments")
        .or_else(|| request.get("params"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if op == "run_verification" {
        arguments["brokered"] = serde_json::json!(true);
    }
    let result = match op {
        "status" => Ok(broker_status(root)),
        "session_view" => multi_agent_session_view(root).map(|view| {
            serde_json::json!({
                "view": view
            })
        }),
        "analyze_change"
        | "plan_verification"
        | "run_verification"
        | "explain_failure"
        | "get_evidence"
        | "escalate_verification"
        | "get_broker_status"
        | "list_sessions"
        | "show_session"
        | "list_queue"
        | "cancel_job"
        | "list_locks"
        | "start_session"
        | "close_session" => call_mcp_tool(root, op, &arguments),
        "shutdown" => Ok(serde_json::json!({
            "shutdown": true
        })),
        other => Err(anyhow::anyhow!("Unknown broker operation: {other}")),
    };
    let response = match result {
        Ok(result) => serde_json::json!({
            "id": id,
            "ok": true,
            "result": result
        }),
        Err(error) => serde_json::json!({
            "id": id,
            "ok": false,
            "error": error.to_string()
        }),
    };
    Ok(Some(serde_json::to_string(&response)?))
}

fn mcp_tool_names() -> Vec<&'static str> {
    vec![
        "analyze_change",
        "plan_verification",
        "run_verification",
        "explain_failure",
        "get_evidence",
        "escalate_verification",
        "get_broker_status",
        "list_sessions",
        "show_session",
        "list_queue",
        "cancel_job",
        "list_locks",
        "start_session",
        "close_session",
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
                "token_profile": token_profile_property(),
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
            "Explain the latest failed evidence or a specific evidence id and recommend the next agent action.",
            serde_json::json!({
                "evidence_id": {
                    "type": "string",
                    "description": "Optional evidence id. If omitted, .vrt/latest.json is explained."
                }
            }),
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
                },
                "token_profile": token_profile_property()
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
        mcp_tool(
            "get_broker_status",
            "Get Broker Status",
            "Inspect repo-local broker state, queue summary, lock summary, sessions, and latest evidence.",
            serde_json::json!({}),
            true,
        ),
        mcp_tool(
            "list_sessions",
            "List Sessions",
            "List session-aware verification contexts recorded for this repository.",
            serde_json::json!({}),
            true,
        ),
        mcp_tool(
            "show_session",
            "Show Session",
            "Show one recorded session context by session id.",
            serde_json::json!({
                "session_id": {
                    "type": "string",
                    "description": "Session id to inspect."
                }
            }),
            true,
        ),
        mcp_tool(
            "list_queue",
            "List Queue",
            "Inspect the repo-local verification queue and runner pool.",
            serde_json::json!({}),
            true,
        ),
        mcp_tool(
            "cancel_job",
            "Cancel Job",
            "Cancel a queued broker job by id when present.",
            serde_json::json!({
                "job_id": {
                    "type": "string",
                    "description": "Queued job id to cancel."
                }
            }),
            false,
        ),
        mcp_tool(
            "list_locks",
            "List Locks",
            "Inspect resource lock observations for the current repository.",
            serde_json::json!({}),
            true,
        ),
        mcp_tool(
            "start_session",
            "Start Session",
            "Create or inspect a session-aware verification context. Pass worktree_path to create a VRT-managed worktree.",
            serde_json::json!({
                "worktree_path": {
                    "type": "string",
                    "description": "Optional worktree path. If provided, VRT creates a managed git worktree session."
                },
                "branch": {
                    "type": "string",
                    "description": "Optional branch name for a managed worktree session."
                }
            }),
            false,
        ),
        mcp_tool(
            "close_session",
            "Close Session",
            "Mark a session context closed without deleting evidence.",
            serde_json::json!({
                "session_id": {
                    "type": "string",
                    "description": "Session id to close."
                }
            }),
            false,
        ),
    ]
}

fn mcp_resources() -> Vec<serde_json::Value> {
    vec![
        mcp_resource(
            "project-profile",
            "Project Profile",
            "vrt://profile",
            "Current VRT project profile, including detected package manager, frameworks, tools, scripts, nodes, and weak spots.",
            "application/json",
            1.0,
        ),
        mcp_resource(
            "latest-evidence",
            "Latest Evidence",
            "vrt://latest-evidence",
            "Latest VRT verification evidence from .vrt/latest.json, when a verification run has written one.",
            "application/json",
            1.0,
        ),
        mcp_resource(
            "vrt-skill",
            "VRT Skill Rules",
            "vrt://skill",
            "Markdown rules an agent should follow when using VRT for local verification.",
            "text/markdown",
            0.8,
        ),
        mcp_resource(
            "token-saving-rules",
            "RTK and Headroom Rules",
            "vrt://token-rules",
            "Markdown rules for preserving evidence references under RTK and Headroom token-saving profiles.",
            "text/markdown",
            0.8,
        ),
        mcp_resource(
            "token-compatibility",
            "Token Compatibility Manifest",
            "vrt://token-compatibility",
            "Machine-readable RTK and Headroom compatibility contract for preserving reversible VRT evidence references.",
            "application/json",
            0.9,
        ),
    ]
}

fn mcp_resource(
    name: &str,
    title: &str,
    uri: &str,
    description: &str,
    mime_type: &str,
    priority: f64,
) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "title": title,
        "uri": uri,
        "description": description,
        "mimeType": mime_type,
        "annotations": {
            "audience": ["assistant"],
            "priority": priority
        }
    })
}

fn read_mcp_resource(root: &Path, uri: &str) -> Result<serde_json::Value> {
    let (mime_type, text) = match uri {
        "vrt://profile" => {
            let profile = profile_project(root)?;
            (
                "application/json",
                serde_json::to_string_pretty(&profile).context("serialize project profile")?,
            )
        }
        "vrt://latest-evidence" => {
            let evidence = read_latest_evidence(root)?;
            (
                "application/json",
                serde_json::to_string_pretty(&evidence).context("serialize latest evidence")?,
            )
        }
        "vrt://skill" => ("text/markdown", skill_markdown().to_string()),
        "vrt://token-rules" => ("text/markdown", token_rules_markdown().to_string()),
        "vrt://token-compatibility" => (
            "application/json",
            serde_json::to_string_pretty(&token_compatibility_manifest())
                .context("serialize token compatibility manifest")?,
        ),
        "" => anyhow::bail!("Missing resource uri"),
        other => anyhow::bail!("Unknown resource uri: {other}"),
    };
    Ok(resource_read_result(uri, mime_type, text))
}

fn resource_read_result(uri: &str, mime_type: &str, text: String) -> serde_json::Value {
    serde_json::json!({
        "contents": [
            {
                "uri": uri,
                "mimeType": mime_type,
                "text": text
            }
        ]
    })
}

fn mcp_prompts() -> Vec<serde_json::Value> {
    vec![
        mcp_prompt(
            "verify_after_change",
            "Verify After Change",
            "Guide an agent through VRT planning, execution, evidence retrieval, and failure handling after a code change.",
            serde_json::json!([
                {
                    "name": "mode",
                    "description": "Verification mode: dev, merge, or release. Defaults to dev.",
                    "required": false
                },
                {
                    "name": "full",
                    "description": "Use true to include production build proof when available.",
                    "required": false
                },
                {
                    "name": "token_profile",
                    "description": "Optional token-saving profile context: standard, rtk, or headroom.",
                    "required": false
                }
            ]),
        ),
        mcp_prompt(
            "explain_failure",
            "Explain Failure",
            "Guide an agent to inspect failed VRT evidence and choose the next smallest corrective action.",
            serde_json::json!([]),
        ),
        mcp_prompt(
            "write_verification_report",
            "Write Verification Report",
            "Guide an agent to produce a concise user-facing verification summary from VRT evidence.",
            serde_json::json!([]),
        ),
    ]
}

fn mcp_prompt(
    name: &str,
    title: &str,
    description: &str,
    arguments: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "title": title,
        "description": description,
        "arguments": arguments
    })
}

fn get_mcp_prompt(name: &str, arguments: &serde_json::Value) -> Result<serde_json::Value> {
    match name {
        "verify_after_change" => {
            let mode = prompt_string_argument(arguments, "mode").unwrap_or("dev");
            if !["dev", "merge", "release"].contains(&mode) {
                anyhow::bail!("Unsupported verification mode: {mode}");
            }
            let full = prompt_string_argument(arguments, "full").unwrap_or("false");
            let token_profile = prompt_string_argument(arguments, "token_profile").unwrap_or("standard");
            Ok(prompt_result(
                "VRT verification workflow prompt",
                format!(
                    "Use VRT to verify the current change with mode={mode}, full={full}, token_profile={token_profile}.\n\
1. Read MCP context with resources/list, then resources/read for vrt://profile and vrt://token-rules.\n\
2. Call plan_verification with mode={mode} and inspect skipped checks as residual risk.\n\
3. Call run_verification with mode={mode}; set full=true only when release or explicit build proof is needed.\n\
4. If verification fails, call explain_failure before changing code again.\n\
5. Read vrt://latest-evidence after the run and preserve evidence_id, raw_log, confidence, and residual risks in the final response."
                ),
            ))
        }
        "explain_failure" => Ok(prompt_result(
            "VRT failure explanation prompt",
            "Use explain_failure first, then read resources/read for vrt://latest-evidence if more detail is needed. Identify the failed check, raw log path, likely root cause, and one next corrective action. Do not rerun broad commands until the cause has been narrowed.".to_string(),
        )),
        "write_verification_report" => Ok(prompt_result(
            "VRT verification report prompt",
            "Read resources/read for vrt://latest-evidence, then report checks run, checks reused, checks skipped, confidence, residual risks, evidence_id, and raw log references. Never describe skipped checks as passed checks.".to_string(),
        )),
        "" => anyhow::bail!("Missing prompt name"),
        other => anyhow::bail!("Unknown prompt: {other}"),
    }
}

fn prompt_string_argument<'a>(arguments: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    arguments.get(key).and_then(|value| value.as_str())
}

fn prompt_result(description: &str, text: String) -> serde_json::Value {
    serde_json::json!({
        "description": description,
        "messages": [
            {
                "role": "user",
                "content": {
                    "type": "text",
                    "text": text
                }
            }
        ]
    })
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
        "mode": mode_property(),
        "token_profile": token_profile_property()
    })
}

fn mode_property() -> serde_json::Value {
    serde_json::json!({
        "type": "string",
        "enum": ["dev", "merge", "release"],
        "description": "Verification confidence target."
    })
}

fn token_profile_property() -> serde_json::Value {
    serde_json::json!({
        "type": "string",
        "enum": ["standard", "rtk", "headroom"],
        "description": "Optional token-saving output profile for preserving evidence references."
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
            let mode = resolve_verification_mode(root, mcp_requested_mode(arguments, "mode")?)?;
            let token_profile = mcp_token_profile(arguments)?;
            let profile = profile_project(root)?;
            let graph = build_capability_graph(root, &profile)?;
            let change = analyze_change(root, &profile)?;
            let plan = plan_verification(&profile, &graph, &change, mode)?;
            Ok(serde_json::json!({
                "token_profile": token_profile,
                "plan": plan
            }))
        }
        "run_verification" => {
            let mode = resolve_verification_mode(root, mcp_requested_mode(arguments, "mode")?)?;
            let token_profile = mcp_token_profile(arguments)?;
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
            } else if arguments
                .get("brokered")
                .or_else(|| arguments.get("broker"))
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
            {
                run_verification_brokered(root, &profile, &change, &plan)?
            } else {
                run_verification(root, &profile, &change, &plan)?
            };
            Ok(evidence_tool_result(evidence, token_profile))
        }
        "explain_failure" => {
            let explanation = if let Some(evidence_id) = arguments
                .get("evidence_id")
                .and_then(|value| value.as_str())
            {
                let evidence = read_evidence(root, evidence_id)?;
                explain_evidence(&evidence, root)
            } else {
                explain_latest(root)?
            };
            Ok(serde_json::json!({
                "explanation": explanation
            }))
        }
        "get_evidence" => {
            let token_profile = mcp_token_profile(arguments)?;
            let evidence = if let Some(evidence_id) = arguments
                .get("evidence_id")
                .and_then(|value| value.as_str())
            {
                read_evidence(root, evidence_id)?
            } else {
                read_latest_evidence(root)?
            };
            Ok(evidence_tool_result(evidence, token_profile))
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
        "get_broker_status" => Ok(broker_status(root)),
        "list_sessions" => Ok(serde_json::json!({
            "sessions": list_session_contexts(root)?,
            "worktree_sessions": list_worktree_sessions(root)?
        })),
        "show_session" => {
            let session_id = required_string(arguments, "session_id")?;
            Ok(serde_json::json!({
                "session": show_session_context(root, session_id)?
            }))
        }
        "list_queue" => Ok(queue_status(root)),
        "cancel_job" => {
            let job_id = required_string(arguments, "job_id")?;
            cancel_queue_job(root, job_id)
        }
        "list_locks" => Ok(lock_list(root)),
        "start_session" => {
            if let Some(worktree_path) = arguments
                .get("worktree_path")
                .and_then(|value| value.as_str())
            {
                let branch = arguments.get("branch").and_then(|value| value.as_str());
                let session = start_worktree_session(root, Path::new(worktree_path), branch)?;
                Ok(serde_json::json!({
                    "session": session
                }))
            } else {
                Ok(serde_json::json!({
                    "session": current_worktree_session(root)?
                }))
            }
        }
        "close_session" => {
            let session_id = required_string(arguments, "session_id")?;
            Ok(serde_json::json!({
                "session": close_session_context(root, session_id)?
            }))
        }
        other => anyhow::bail!("Unknown tool: {other}"),
    }
}

fn required_string<'a>(arguments: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    arguments
        .get(key)
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("{key} is required"))
}

fn mcp_requested_mode(
    arguments: &serde_json::Value,
    key: &str,
) -> Result<Option<VerificationMode>> {
    match arguments.get(key).and_then(|value| value.as_str()) {
        None => Ok(None),
        Some("dev") => Ok(Some(VerificationMode::Dev)),
        Some("merge") => Ok(Some(VerificationMode::Merge)),
        Some("release") => Ok(Some(VerificationMode::Release)),
        Some(other) => anyhow::bail!("Unsupported verification mode: {other}"),
    }
}

fn mcp_token_profile(arguments: &serde_json::Value) -> Result<TokenProfile> {
    match arguments
        .get("token_profile")
        .and_then(|value| value.as_str())
    {
        Some("standard") | None => Ok(TokenProfile::Standard),
        Some("rtk") => Ok(TokenProfile::Rtk),
        Some("headroom") => Ok(TokenProfile::Headroom),
        Some(other) => anyhow::bail!("Unsupported token profile: {other}"),
    }
}

fn evidence_tool_result(
    evidence: EvidenceRecord,
    token_profile: TokenProfile,
) -> serde_json::Value {
    let token_report = if token_profile == TokenProfile::Standard {
        None
    } else {
        Some(render_token_report(&evidence, token_profile))
    };
    let mut result = serde_json::json!({
        "token_profile": token_profile,
        "evidence": evidence
    });
    if let Some(token_report) = token_report {
        result["token_report"] = serde_json::json!(token_report);
    }
    result
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
            safety_level: cap.safety_level.clone(),
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

fn read_all_evidence_records(root: &Path) -> Result<Vec<EvidenceRecord>> {
    let evidence_dir = root.join(".vrt/evidence");
    if !evidence_dir.exists() {
        return Ok(vec![]);
    }
    let mut records = Vec::new();
    for entry in fs::read_dir(evidence_dir)? {
        let entry = entry?;
        let path = entry.path().join("evidence.json");
        if entry.file_type()?.is_dir() && path.exists() {
            records.push(
                serde_json::from_str(&fs::read_to_string(&path)?)
                    .with_context(|| format!("parse {}", path.display()))?,
            );
        }
    }
    Ok(records)
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
    if !path.exists() {
        return Ok(PackageJson::empty());
    }
    let data = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&data).with_context(|| format!("parse {}", path.display()))
}

fn read_vrt_config(root: &Path) -> Result<Option<VrtConfig>> {
    let path = root.join(".vrt/config.toml");
    if !path.exists() {
        return Ok(None);
    }
    let config: VrtConfig = toml::from_str(&fs::read_to_string(&path)?)
        .with_context(|| format!("parse {}", path.display()))?;
    Ok(Some(config))
}

fn release_policy(root: &Path) -> Result<ReleasePolicy> {
    let Some(config) = read_vrt_config(root)? else {
        return Ok(ReleasePolicy {
            require_full_build: true,
            require_ci: true,
        });
    };
    let Some(release) = config.release else {
        return Ok(ReleasePolicy {
            require_full_build: true,
            require_ci: true,
        });
    };
    Ok(ReleasePolicy {
        require_full_build: release.require_full_build.unwrap_or(true),
        require_ci: release.require_ci.unwrap_or(true),
    })
}

fn strict_policy_matches(root: &Path, risk_tags: &BTreeSet<RiskTag>) -> Result<bool> {
    let Some(config) = read_vrt_config(root)? else {
        return Ok(false);
    };
    let Some(areas) = config
        .policy
        .and_then(|policy| policy.strict)
        .and_then(|strict| strict.areas)
    else {
        return Ok(false);
    };
    let strict_tags = areas
        .iter()
        .flat_map(|area| policy_area_tags(area))
        .collect::<BTreeSet<_>>();
    Ok(risk_tags.iter().any(|tag| strict_tags.contains(tag)))
}

fn relaxed_policy_matches(root: &Path, risk_tags: &BTreeSet<RiskTag>) -> Result<bool> {
    if has_hard_risk(risk_tags) {
        return Ok(false);
    }
    let Some(config) = read_vrt_config(root)? else {
        return Ok(false);
    };
    let Some(areas) = config
        .policy
        .and_then(|policy| policy.relaxed)
        .and_then(|relaxed| relaxed.areas)
    else {
        return Ok(false);
    };
    let relaxed_tags = areas
        .iter()
        .flat_map(|area| policy_area_tags(area))
        .collect::<BTreeSet<_>>();
    Ok(risk_tags.iter().any(|tag| relaxed_tags.contains(tag)))
}

fn has_hard_risk(risk_tags: &BTreeSet<RiskTag>) -> bool {
    risk_tags.iter().any(|tag| {
        matches!(
            tag,
            RiskTag::Auth
                | RiskTag::Billing
                | RiskTag::DatabaseSchema
                | RiskTag::Migration
                | RiskTag::Env
                | RiskTag::Infra
                | RiskTag::Ci
                | RiskTag::PackageBoundary
                | RiskTag::BuildConfig
        )
    })
}

fn policy_area_tags(area: &str) -> Vec<RiskTag> {
    match area.trim().to_ascii_lowercase().as_str() {
        "docs" | "documentation" => vec![RiskTag::Docs],
        "marketing" => vec![RiskTag::Marketing],
        "style" | "css" => vec![RiskTag::Style],
        "ui" | "ui_component" | "ui-component" => vec![RiskTag::UiComponent],
        "api" | "api_route" | "api-route" => vec![RiskTag::ApiRoute],
        "shared" | "shared_package" | "shared-package" => vec![RiskTag::SharedPackage],
        "package" | "package_boundary" | "package-boundary" => vec![RiskTag::PackageBoundary],
        "build" | "build_config" | "build-config" => vec![RiskTag::BuildConfig],
        "database" | "db" => vec![RiskTag::DatabaseSchema, RiskTag::Migration],
        "migration" | "migrations" => vec![RiskTag::Migration],
        "auth" => vec![RiskTag::Auth],
        "billing" | "payments" => vec![RiskTag::Billing],
        "env" | "environment" => vec![RiskTag::Env],
        "infra" | "infrastructure" => vec![RiskTag::Infra],
        "ci" => vec![RiskTag::Ci],
        "unknown" => vec![RiskTag::Unknown],
        _ => vec![],
    }
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

fn destructive_package_scripts(package_json: &PackageJson) -> Vec<String> {
    package_json
        .scripts
        .iter()
        .filter(|(_, command)| is_destructive_command(command))
        .map(|(name, _)| name.clone())
        .collect()
}

fn browser_smoke_script(package_json: &PackageJson) -> Option<&str> {
    [
        "smoke",
        "test:smoke",
        "e2e",
        "test:e2e",
        "playwright",
        "test:playwright",
    ]
    .into_iter()
    .find(|script| package_json.scripts.contains_key(*script))
}

fn has_browser_smoke_script(package_json: &PackageJson) -> bool {
    browser_smoke_script(package_json).is_some()
}

fn has_env_validation_script(package_json: &PackageJson) -> bool {
    env_validation_script(package_json).is_some()
}

fn env_validation_script(package_json: &PackageJson) -> Option<&str> {
    package_json.scripts.iter().find_map(|(name, command)| {
        let text = format!("{name} {command}").to_ascii_lowercase();
        let matches = (text.contains("env") && text.contains("valid"))
            || text.contains("validate-env")
            || text.contains("env:check")
            || text.contains("check:env")
            || text.contains("dotenvx")
            || text.contains("@t3-oss/env")
            || text.contains("t3-env")
            || text.contains("zod-env");
        matches.then_some(name.as_str())
    })
}

fn has_migration_safety_script(package_json: &PackageJson) -> bool {
    migration_safety_script(package_json).is_some()
}

fn migration_safety_script(package_json: &PackageJson) -> Option<&str> {
    package_json.scripts.iter().find_map(|(name, command)| {
        let text = format!("{name} {command}").to_ascii_lowercase();
        let matches = text.contains("migrate diff")
            || text.contains("migrate status")
            || text.contains("migration:check")
            || text.contains("migrations:check")
            || text.contains("check:migration")
            || text.contains("check:migrations")
            || text.contains("migration") && text.contains("safe");
        matches.then_some(name.as_str())
    })
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
    let mut nodes = BTreeMap::new();
    nodes.insert(
        ".".to_string(),
        ProjectNode {
            id: "workspace".to_string(),
            name: package_json
                .name
                .clone()
                .unwrap_or_else(|| "workspace".to_string()),
            path: ".".to_string(),
            kind: "workspace".to_string(),
            framework: frameworks.iter().next().map(|item| format!("{item:?}")),
            package_name: package_json.name.clone(),
            dependencies: package_dependency_names(package_json),
        },
    );
    for segment in ["apps", "packages"] {
        let parent = root.join(segment);
        if !parent.exists() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(parent) {
            for entry in entries.flatten() {
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if !file_type.is_dir() {
                    continue;
                }
                let path = format!("{segment}/{}", entry.file_name().to_string_lossy());
                let kind = if segment == "apps" { "app" } else { "package" };
                nodes.insert(
                    path.clone(),
                    ProjectNode {
                        id: path.replace('/', "-"),
                        name: path.clone(),
                        path,
                        kind: kind.to_string(),
                        framework: None,
                        package_name: None,
                        dependencies: vec![],
                    },
                );
            }
        }
    }
    for entry in WalkDir::new(root)
        .min_depth(2)
        .max_depth(3)
        .into_iter()
        .flatten()
    {
        if entry.file_name() == "package.json" {
            if let Ok(relative) = entry.path().parent().unwrap_or(root).strip_prefix(root) {
                let path = relative.to_string_lossy().to_string();
                let node_package = fs::read_to_string(entry.path())
                    .ok()
                    .and_then(|data| serde_json::from_str::<PackageJson>(&data).ok())
                    .unwrap_or_else(PackageJson::empty);
                let kind = if path.starts_with("apps/") {
                    "app"
                } else {
                    "package"
                };
                nodes.insert(
                    path.clone(),
                    ProjectNode {
                        id: path.replace('/', "-"),
                        name: node_package.name.clone().unwrap_or_else(|| path.clone()),
                        path,
                        kind: kind.to_string(),
                        framework: None,
                        package_name: node_package.name.clone(),
                        dependencies: package_dependency_names(&node_package),
                    },
                );
            }
        }
    }
    nodes.into_values().collect()
}

fn package_dependency_names(package_json: &PackageJson) -> Vec<String> {
    package_json
        .dependencies
        .keys()
        .chain(package_json.dev_dependencies.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn discover_ci_workflows(root: &Path) -> Vec<CiWorkflow> {
    let workflows_dir = root.join(".github/workflows");
    if !workflows_dir.exists() {
        return vec![];
    }

    let mut workflow_paths = fs::read_dir(&workflows_dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| matches!(extension, "yml" | "yaml"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    workflow_paths.sort();

    workflow_paths
        .into_iter()
        .map(|path| parse_github_actions_workflow(root, &path))
        .collect()
}

fn parse_github_actions_workflow(root: &Path, path: &Path) -> CiWorkflow {
    let relative_path = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let source = fs::read_to_string(path).unwrap_or_default();
    let parsed = serde_yaml::from_str::<serde_yaml::Value>(&source).ok();
    let name = parsed
        .as_ref()
        .and_then(|value| yaml_mapping_get(value, "name"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
        .or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "workflow".to_string());
    let jobs = parsed
        .as_ref()
        .and_then(|value| yaml_mapping_get(value, "jobs"))
        .and_then(|value| value.as_mapping())
        .map(|mapping| {
            mapping
                .keys()
                .filter_map(|key| key.as_str().map(ToString::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut commands = Vec::new();
    if let Some(value) = parsed.as_ref() {
        collect_yaml_run_commands(value, &mut commands);
    }
    commands.sort();
    commands.dedup();
    let has_matrix = parsed.as_ref().is_some_and(yaml_has_matrix);
    let runs_typecheck = commands
        .iter()
        .any(|command| ci_command_runs_typecheck(command));
    let runs_test = commands.iter().any(|command| ci_command_runs_test(command));
    let runs_build = commands
        .iter()
        .any(|command| ci_command_runs_build(command));
    let runs_e2e = commands.iter().any(|command| ci_command_runs_e2e(command));

    CiWorkflow {
        provider: "github-actions".to_string(),
        path: relative_path,
        name,
        jobs,
        commands,
        runs_typecheck,
        runs_test,
        runs_build,
        runs_e2e,
        has_matrix,
    }
}

fn yaml_mapping_get<'a>(value: &'a serde_yaml::Value, key: &str) -> Option<&'a serde_yaml::Value> {
    value.as_mapping()?.iter().find_map(|(candidate, value)| {
        if candidate.as_str() == Some(key) {
            Some(value)
        } else {
            None
        }
    })
}

fn collect_yaml_run_commands(value: &serde_yaml::Value, commands: &mut Vec<String>) {
    match value {
        serde_yaml::Value::Mapping(mapping) => {
            for (key, value) in mapping {
                if key.as_str() == Some("run") {
                    if let Some(command) = value.as_str() {
                        commands.push(command.to_string());
                    }
                }
                collect_yaml_run_commands(value, commands);
            }
        }
        serde_yaml::Value::Sequence(items) => {
            for item in items {
                collect_yaml_run_commands(item, commands);
            }
        }
        _ => {}
    }
}

fn yaml_has_matrix(value: &serde_yaml::Value) -> bool {
    match value {
        serde_yaml::Value::Mapping(mapping) => mapping
            .iter()
            .any(|(key, value)| key.as_str() == Some("matrix") || yaml_has_matrix(value)),
        serde_yaml::Value::Sequence(items) => items.iter().any(yaml_has_matrix),
        _ => false,
    }
}

fn ci_command_runs_typecheck(command: &str) -> bool {
    let command = command.to_ascii_lowercase();
    command.contains("typecheck")
        || command.contains("type-check")
        || (command.contains("tsc") && command.contains("--noemit"))
        || (command.contains("tsc") && command.contains("--no-emit"))
}

fn ci_command_runs_test(command: &str) -> bool {
    let command = command.to_ascii_lowercase();
    command.contains(" test")
        || command.starts_with("test")
        || command.contains("npm test")
        || command.contains("pnpm test")
        || command.contains("yarn test")
        || command.contains("bun test")
        || command.contains("vitest")
        || command.contains("jest")
}

fn ci_command_runs_build(command: &str) -> bool {
    let command = command.to_ascii_lowercase();
    command.contains(" build")
        || command.starts_with("build")
        || command.contains("npm run build")
        || command.contains("pnpm run build")
        || command.contains("yarn build")
        || command.contains("bun run build")
        || command.contains("next build")
        || command.contains("vite build")
}

fn ci_command_runs_e2e(command: &str) -> bool {
    let command = command.to_ascii_lowercase();
    command.contains("e2e")
        || command.contains("playwright")
        || command.contains("cypress")
        || command.contains("browser-smoke")
        || command.contains("test:smoke")
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
        let command = package_script(profile, script);
        capabilities.push(VerificationCapability {
            id: format!("workspace-{script}"),
            kind: kind.to_string(),
            command: command.clone(),
            cwd: cwd.to_string(),
            scope: "workspace".to_string(),
            cost: cost.to_string(),
            safety_level: command_safety_level(kind, &command, cost).to_string(),
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

fn add_test_capabilities(
    profile: &ProjectProfile,
    capabilities: &mut Vec<VerificationCapability>,
    cwd: &str,
) {
    let related_script = related_test_script_from_profile(profile);
    if let Some(script) = related_script {
        let command = package_script(profile, script);
        capabilities.push(VerificationCapability {
            id: "workspace-related-test".to_string(),
            kind: "unit_test".to_string(),
            command: command.clone(),
            cwd: cwd.to_string(),
            scope: "workspace".to_string(),
            cost: "medium".to_string(),
            safety_level: command_safety_level("unit_test", &command, "medium").to_string(),
            confidence_contribution: "high".to_string(),
            proves: vec!["Related test behavior for changed files".to_string()],
            cannot_prove: vec![
                "Full test suite behavior".to_string(),
                "Production bundler behavior".to_string(),
                "full browser behavior".to_string(),
            ],
            cacheable: true,
            parallelizable: true,
            side_effects: vec![],
            resource_requirements: vec![],
        });
        if profile.scripts.iter().any(|item| item.name == "test") {
            let command = package_script(profile, "test");
            capabilities.push(VerificationCapability {
                id: "workspace-test".to_string(),
                kind: "full_test".to_string(),
                command: command.clone(),
                cwd: cwd.to_string(),
                scope: "workspace".to_string(),
                cost: "expensive".to_string(),
                safety_level: command_safety_level("full_test", &command, "expensive").to_string(),
                confidence_contribution: "high".to_string(),
                proves: vec!["Full package test suite behavior".to_string()],
                cannot_prove: cannot_prove_for_kind("unit_test"),
                cacheable: true,
                parallelizable: true,
                side_effects: vec![],
                resource_requirements: vec![],
            });
        }
        return;
    }
    add_script_capability(
        profile,
        capabilities,
        "test",
        "unit_test",
        "medium",
        "high",
        cwd,
    );
}

fn add_format_check_capability(
    profile: &ProjectProfile,
    capabilities: &mut Vec<VerificationCapability>,
    cwd: &str,
) {
    let Some(script) = format_check_script_from_profile(profile) else {
        return;
    };
    let command = package_script(profile, script);
    capabilities.push(VerificationCapability {
        id: "workspace-format-check".to_string(),
        kind: "format_check".to_string(),
        command: command.clone(),
        cwd: cwd.to_string(),
        scope: "workspace".to_string(),
        cost: "cheap".to_string(),
        safety_level: command_safety_level("format_check", &command, "cheap").to_string(),
        confidence_contribution: "medium".to_string(),
        proves: proves_for_kind("format_check"),
        cannot_prove: cannot_prove_for_kind("format_check"),
        cacheable: true,
        parallelizable: true,
        side_effects: vec![],
        resource_requirements: vec![],
    });
}

fn add_env_validation_capability(
    profile: &ProjectProfile,
    capabilities: &mut Vec<VerificationCapability>,
    cwd: &str,
) {
    let Some(script) = env_validation_script_from_profile(profile) else {
        return;
    };
    let command = package_script(profile, script);
    capabilities.push(VerificationCapability {
        id: "workspace-env-validate".to_string(),
        kind: "env_validate".to_string(),
        command: command.clone(),
        cwd: cwd.to_string(),
        scope: "workspace".to_string(),
        cost: "cheap".to_string(),
        safety_level: command_safety_level("env_validate", &command, "cheap").to_string(),
        confidence_contribution: "high".to_string(),
        proves: vec!["Environment configuration contract validates locally".to_string()],
        cannot_prove: vec![
            "Hosted environment variables are present".to_string(),
            "External secret values are correct".to_string(),
        ],
        cacheable: true,
        parallelizable: true,
        side_effects: vec![],
        resource_requirements: vec![],
    });
}

fn add_migration_safety_capability(
    profile: &ProjectProfile,
    capabilities: &mut Vec<VerificationCapability>,
    cwd: &str,
) {
    let Some(script) = migration_safety_script_from_profile(profile) else {
        return;
    };
    let command = package_script(profile, script);
    capabilities.push(VerificationCapability {
        id: "workspace-migration-safety".to_string(),
        kind: "migration_safety".to_string(),
        command: command.clone(),
        cwd: cwd.to_string(),
        scope: "workspace".to_string(),
        cost: "medium".to_string(),
        safety_level: command_safety_level("migration_safety", &command, "medium").to_string(),
        confidence_contribution: "high".to_string(),
        proves: proves_for_kind("migration_safety"),
        cannot_prove: cannot_prove_for_kind("migration_safety"),
        cacheable: false,
        parallelizable: false,
        side_effects: vec![],
        resource_requirements: vec!["database-schema".to_string()],
    });
}

fn add_prisma_generate_capability(
    profile: &ProjectProfile,
    capabilities: &mut Vec<VerificationCapability>,
    cwd: &str,
) {
    let Some(script) = prisma_generate_script_from_profile(profile) else {
        return;
    };
    let command = package_script(profile, script);
    capabilities.push(VerificationCapability {
        id: "workspace-prisma-generate".to_string(),
        kind: "schema_generate".to_string(),
        command: command.clone(),
        cwd: cwd.to_string(),
        scope: "workspace".to_string(),
        cost: "cheap".to_string(),
        safety_level: command_safety_level("schema_generate", &command, "cheap").to_string(),
        confidence_contribution: "medium".to_string(),
        proves: proves_for_kind("schema_generate"),
        cannot_prove: cannot_prove_for_kind("schema_generate"),
        cacheable: false,
        parallelizable: false,
        side_effects: vec!["generated-client".to_string()],
        resource_requirements: vec!["database-schema".to_string()],
    });
}

fn add_browser_smoke_capability(
    profile: &ProjectProfile,
    capabilities: &mut Vec<VerificationCapability>,
    cwd: &str,
) {
    if !profile.tools.contains(&Detection::Playwright) {
        return;
    }
    let Some(script) = browser_smoke_script_from_profile(profile) else {
        return;
    };
    let command = package_script(profile, script);
    capabilities.push(VerificationCapability {
        id: "workspace-browser-smoke".to_string(),
        kind: "browser_smoke".to_string(),
        command: command.clone(),
        cwd: cwd.to_string(),
        scope: "workspace".to_string(),
        cost: "expensive".to_string(),
        safety_level: command_safety_level("browser_smoke", &command, "expensive").to_string(),
        confidence_contribution: "medium".to_string(),
        proves: proves_for_kind("browser_smoke"),
        cannot_prove: cannot_prove_for_kind("browser_smoke"),
        cacheable: true,
        parallelizable: false,
        side_effects: vec![],
        resource_requirements: vec!["browser".to_string(), "ports".to_string()],
    });
}

fn format_check_script_from_profile(profile: &ProjectProfile) -> Option<&str> {
    profile.scripts.iter().find_map(|item| {
        let text = format!("{} {}", item.name, item.command).to_ascii_lowercase();
        let looks_like_check = item.name.contains("check")
            || text.contains("biome check")
            || text.contains("biome ci")
            || text.contains("--check");
        let looks_like_format =
            item.name.contains("format") || text.contains("format") || text.contains("biome");
        if looks_like_check && looks_like_format {
            Some(item.name.as_str())
        } else {
            None
        }
    })
}

fn browser_smoke_script_from_profile(profile: &ProjectProfile) -> Option<&str> {
    [
        "smoke",
        "test:smoke",
        "e2e",
        "test:e2e",
        "playwright",
        "test:playwright",
    ]
    .into_iter()
    .find(|script| profile.scripts.iter().any(|item| item.name == *script))
}

fn related_test_script_from_profile(profile: &ProjectProfile) -> Option<&str> {
    [
        "test:related",
        "test:changed",
        "test:affected",
        "related:test",
        "changed:test",
        "affected:test",
    ]
    .into_iter()
    .find(|script| profile.scripts.iter().any(|item| item.name == *script))
}

fn env_validation_script_from_profile(profile: &ProjectProfile) -> Option<&str> {
    profile.scripts.iter().find_map(|item| {
        let text = format!("{} {}", item.name, item.command).to_ascii_lowercase();
        let matches = (text.contains("env") && text.contains("valid"))
            || text.contains("validate-env")
            || text.contains("env:check")
            || text.contains("check:env")
            || text.contains("dotenvx")
            || text.contains("@t3-oss/env")
            || text.contains("t3-env")
            || text.contains("zod-env");
        matches.then_some(item.name.as_str())
    })
}

fn migration_safety_script_from_profile(profile: &ProjectProfile) -> Option<&str> {
    profile.scripts.iter().find_map(|item| {
        let text = format!("{} {}", item.name, item.command).to_ascii_lowercase();
        let matches = text.contains("migrate diff")
            || text.contains("migrate status")
            || text.contains("migration:check")
            || text.contains("migrations:check")
            || text.contains("check:migration")
            || text.contains("check:migrations")
            || text.contains("migration") && text.contains("safe");
        matches.then_some(item.name.as_str())
    })
}

fn prisma_generate_script_from_profile(profile: &ProjectProfile) -> Option<&str> {
    profile.scripts.iter().find_map(|item| {
        let text = format!("{} {}", item.name, item.command).to_ascii_lowercase();
        let matches = text.contains("prisma generate")
            || item.name == "prisma:generate"
            || item.name == "generate:prisma";
        matches.then_some(item.name.as_str())
    })
}

fn package_script(profile: &ProjectProfile, script: &str) -> String {
    if profile.workspace_kind == "nx" {
        return package_binary(profile, &format!("nx affected -t {script}"));
    }
    if profile.workspace_kind == "turbo" {
        return package_binary(profile, &format!("turbo run {script} --affected"));
    }
    match profile.package_manager {
        PackageManager::Pnpm => format!("pnpm {script}"),
        PackageManager::Yarn => format!("yarn {script}"),
        PackageManager::Bun => format!("bun run {script}"),
        PackageManager::Npm | PackageManager::Unknown => format!("npm run {script}"),
    }
}

fn package_binary(profile: &ProjectProfile, command: &str) -> String {
    match profile.package_manager {
        PackageManager::Pnpm => format!("pnpm {command}"),
        PackageManager::Yarn => format!("yarn {command}"),
        PackageManager::Bun => format!("bunx {command}"),
        PackageManager::Npm | PackageManager::Unknown => format!("npx {command}"),
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
        "format_check" => vec!["Source format and style consistency".to_string()],
        "env_validate" => vec!["Environment configuration contract".to_string()],
        "schema_generate" => vec!["Prisma client generation".to_string()],
        "migration_safety" => vec!["Migration safety contract".to_string()],
        "unit_test" => vec!["Package-level test behavior".to_string()],
        "build" => vec!["Production bundler compilation".to_string()],
        "browser_smoke" => vec!["Configured browser smoke behavior".to_string()],
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
        "format_check" => vec!["Type soundness".to_string(), "runtime behavior".to_string()],
        "env_validate" => vec![
            "Hosted environment variables are present".to_string(),
            "External secret values are correct".to_string(),
        ],
        "schema_generate" => vec![
            "Migration safety against a live database".to_string(),
            "Runtime query correctness".to_string(),
        ],
        "migration_safety" => vec![
            "Live database state matches the checked baseline".to_string(),
            "Application data migrations are reversible".to_string(),
        ],
        "unit_test" => vec![
            "Production bundler behavior".to_string(),
            "full browser behavior".to_string(),
        ],
        "build" => vec![
            "Business behavior correctness".to_string(),
            "external service correctness".to_string(),
        ],
        "browser_smoke" => vec![
            "Full browser regression coverage".to_string(),
            "production deployment behavior".to_string(),
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

fn dirty_state(root: &Path) -> DirtyState {
    let output = git_output(root, ["status", "--porcelain"]).unwrap_or_default();
    let changed_files = output
        .lines()
        .filter_map(|line| {
            let path = line.split_whitespace().last()?;
            if path.is_empty() {
                None
            } else {
                Some(path.to_string())
            }
        })
        .collect::<Vec<_>>();
    DirtyState {
        is_dirty: !changed_files.is_empty(),
        changed_files,
    }
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
    if path.contains("marketing") || path.contains("growth") || path.contains("landing") {
        risk_tags.insert(RiskTag::Marketing);
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
        || (path.starts_with("packages/")
            && (path.ends_with("/src/index.ts")
                || path.ends_with("/src/index.tsx")
                || path.ends_with("/index.ts")
                || path.ends_with("/index.tsx")))
    {
        risk_tags.insert(RiskTag::PackageBoundary);
    }
    if path.contains("next.config.")
        || path.contains("vite.config.")
        || path == "turbo.json"
        || path == "nx.json"
    {
        risk_tags.insert(RiskTag::BuildConfig);
    }
    if path.contains("schema.prisma") || path.contains("drizzle.config.") {
        risk_tags.insert(RiskTag::DatabaseSchema);
    }
    if path.contains("migration")
        || path.contains("migrations/")
        || path.starts_with("drizzle/")
        || path.contains("drizzle.config.")
    {
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
    let mut changed_package_names = BTreeSet::new();
    for file in changed_files {
        for node in &profile.nodes {
            if node.path != "." && file.path.starts_with(&node.path) {
                affected.insert(node.id.clone());
                if let Some(package_name) = &node.package_name {
                    changed_package_names.insert(package_name.clone());
                }
            }
        }
    }
    if !changed_package_names.is_empty() {
        for node in &profile.nodes {
            if node
                .dependencies
                .iter()
                .any(|dependency| changed_package_names.contains(dependency))
            {
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
        "format_check" => 120_000,
        "env_validate" => 120_000,
        "schema_generate" => 180_000,
        "migration_safety" => 240_000,
        "typecheck" => 180_000,
        "unit_test" => 240_000,
        "browser_smoke" => 360_000,
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
        "format_check" => (
            "Format/style check is reserved for merge/release or high-risk changes.".to_string(),
            "Format and style consistency not verified.".to_string(),
        ),
        "env_validate" => (
            "No environment-sensitive risk required env validation in this mode.".to_string(),
            "Environment configuration contract not verified.".to_string(),
        ),
        "schema_generate" => (
            "No database schema or migration risk required Prisma client generation in this mode."
                .to_string(),
            "Prisma generated client freshness not verified.".to_string(),
        ),
        "migration_safety" => (
            "No database schema, migration, or release risk required migration safety proof in this mode."
                .to_string(),
            "Migration safety contract not verified.".to_string(),
        ),
        "unit_test" => (
            "No executable JS/TS risk requiring package tests was detected.".to_string(),
            "Behavior covered only by unit tests is not proven.".to_string(),
        ),
        "full_test" => (
            "Related test capability was selected for the local loop.".to_string(),
            "Full test suite behavior not verified.".to_string(),
        ),
        "browser_smoke" => (
            "Browser smoke proof is reserved for merge/release or high-risk changes.".to_string(),
            "Browser smoke behavior not verified.".to_string(),
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
    release_requires_ci: bool,
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
    let release = if matches!(mode, VerificationMode::Release) && has_build && !release_requires_ci
    {
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
    let mut structured = Vec::new();
    let mut generic = Vec::new();
    let mut pending_test_context: Option<String> = None;
    for line in log.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('$') || trimmed.starts_with('>') {
            continue;
        }
        if let Some(candidate) = parse_typescript_diagnostic(trimmed) {
            push_unique(&mut structured, candidate);
        } else if let Some(context) = parse_test_failure_header(trimmed) {
            pending_test_context = Some(context);
        } else if let Some(context) = parse_jest_failure_title(trimmed) {
            pending_test_context = Some(context);
        } else if is_assertion_line(trimmed) {
            if let Some(context) = pending_test_context.take() {
                push_unique(&mut structured, format!("{context}: {}", trimmed.trim()));
            } else {
                push_unique(&mut structured, trimmed.trim().to_string());
            }
        } else if let Some(location) = parse_test_stack_location(trimmed) {
            push_unique(&mut structured, location);
        } else {
            let lower = line.to_lowercase();
            if lower.contains("error")
                || lower.contains("failed")
                || lower.contains("panic")
                || lower.contains("exception")
            {
                push_unique(&mut generic, line.trim().to_string());
            }
        }
        if structured.len() >= 5 {
            break;
        }
    }
    let mut candidates = structured;
    for candidate in generic {
        push_unique(&mut candidates, candidate);
        if candidates.len() >= 5 {
            break;
        }
    }
    candidates
}

fn push_unique(candidates: &mut Vec<String>, candidate: String) {
    if !candidate.is_empty() && !candidates.contains(&candidate) {
        candidates.push(candidate);
    }
}

fn parse_typescript_diagnostic(line: &str) -> Option<String> {
    if let Some(candidate) = parse_colon_typescript_diagnostic(line) {
        return Some(candidate);
    }
    let location_end = line.find("): error ")?;
    let location = &line[..location_end + 1];
    let (path, line_col) = location.rsplit_once('(')?;
    let (line_no, col_no) = line_col.trim_end_matches(')').split_once(',')?;
    if path.is_empty() || line_no.parse::<u64>().is_err() || col_no.parse::<u64>().is_err() {
        return None;
    }
    let diagnostic = &line[location_end + "): error ".len()..];
    let normalized = diagnostic.replacen(": ", " ", 1);
    Some(format!("{path}:{line_no}:{col_no} {normalized}"))
}

fn parse_colon_typescript_diagnostic(line: &str) -> Option<String> {
    let marker = " - error ";
    let location_end = line.find(marker)?;
    let location = &line[..location_end];
    let diagnostic = &line[location_end + marker.len()..];
    let (path_and_line, col_no) = location.rsplit_once(':')?;
    let (path, line_no) = path_and_line.rsplit_once(':')?;
    if path.is_empty() || line_no.parse::<u64>().is_err() || col_no.parse::<u64>().is_err() {
        return None;
    }
    let normalized = diagnostic.replacen(": ", " ", 1);
    Some(format!("{path}:{line_no}:{col_no} {normalized}"))
}

fn parse_test_failure_header(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed
        .strip_prefix("FAIL ")
        .or_else(|| trimmed.strip_prefix("FAIL\t"))?
        .trim();
    if rest.is_empty() {
        return None;
    }
    Some(rest.to_string())
}

fn parse_jest_failure_title(line: &str) -> Option<String> {
    let rest = line.trim().strip_prefix('●')?.trim();
    if rest.is_empty() {
        return None;
    }
    Some(rest.replace('›', ">"))
}

fn is_assertion_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("AssertionError")
        || trimmed.starts_with("Error:")
        || trimmed.starts_with("TypeError:")
        || trimmed.starts_with("ReferenceError:")
        || trimmed.starts_with("Expected ")
        || trimmed.starts_with("Received ")
}

fn parse_test_stack_location(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed
        .strip_prefix('❯')
        .or_else(|| trimmed.strip_prefix("at "))?
        .trim();
    let location = if let Some(open) = rest.rfind('(') {
        let close = rest[open + 1..].find(')')? + open + 1;
        &rest[open + 1..close]
    } else {
        rest.split_whitespace().next()?
    };
    if !location.contains(".ts") && !location.contains(".js") {
        return None;
    }
    let (path_and_line, col) = location.rsplit_once(':')?;
    let (path, line_no) = path_and_line.rsplit_once(':')?;
    if line_no.parse::<u64>().is_err() || col.parse::<u64>().is_err() {
        return None;
    }
    if path.contains("node_modules") {
        return None;
    }
    Some(format!("{path}:{line_no}:{col}"))
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

fn markdown_inline_code(value: &str) -> String {
    value.replace('`', "\\`")
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

fn config_hash(root: &Path) -> String {
    fs::read(root.join(".vrt/config.toml"))
        .map(|bytes| hash_bytes(&bytes))
        .unwrap_or_else(|_| hash_string("missing:.vrt/config.toml"))
}

fn toolchain_version() -> String {
    format!("vrt-core/{}", env!("CARGO_PKG_VERSION"))
}

fn relevant_inputs_hash(
    root: &Path,
    profile: &ProjectProfile,
    change: &ChangeSet,
    plan: &VerificationPlan,
) -> String {
    let fingerprint = serde_json::json!({
        "schema_version": VRT_SCHEMA_VERSION,
        "base_commit": change.base_commit,
        "diff_hash": change.diff_hash,
        "profile_hash": profile.profile_hash,
        "lockfile_hash": lockfile_hash(root),
        "config_hash": config_hash(root),
        "toolchain_version": toolchain_version(),
        "changed_files": change.changed_files,
        "affected_nodes": change.affected_nodes,
        "risk_tags": change.risk_tags,
        "plan_id": plan.plan_id,
        "steps": plan.steps,
        "skipped": plan.skipped,
    });
    hash_string(&fingerprint.to_string())
}

fn env_assumptions() -> Vec<String> {
    vec![
        "local process environment captured by project-owned commands".to_string(),
        "hosted CI and deployment environments remain external proof".to_string(),
    ]
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
