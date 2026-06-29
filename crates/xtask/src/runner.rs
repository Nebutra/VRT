//! Clean-room execution: copy a fixture, apply the change under test, and run
//! both the naive baseline command set and `vrt verify --json` with real
//! wall-clock measurement. A missing toolchain yields `not_available`, never a
//! fabricated duration (Canvas §2.1, §7.7).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use tempfile::TempDir;

use crate::model::{CommandRun, RunStatus};

/// One file written into the clean room after the baseline commit — i.e. the
/// diff VRT will analyse.
#[derive(Debug, Clone)]
pub struct Mutation {
    pub path: String,
    pub contents: String,
}

/// A prepared, isolated working copy of a fixture with the mutation applied and
/// a baseline commit in place so VRT can compute a real diff.
pub struct CleanRoom {
    dir: TempDir,
}

impl CleanRoom {
    pub fn path(&self) -> &Path {
        self.dir.path()
    }
}

fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(src) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(src).unwrap();
        if rel.as_os_str().is_empty() {
            continue;
        }
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn git(dir: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .args([
            "-c",
            "user.email=proof@vrt.local",
            "-c",
            "user.name=vrt-proof",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .with_context(|| format!("git {args:?}"))?;
    if !status.status.success() {
        return Err(anyhow!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&status.stderr)
        ));
    }
    Ok(())
}

/// Prepare an isolated clean room: copy the fixture, commit it as the baseline,
/// then apply the mutation on top (uncommitted = the change under test).
pub fn prepare(fixture: &Path, mutations: &[Mutation]) -> Result<CleanRoom> {
    let dir = tempfile::Builder::new().prefix("vrt-proof-").tempdir()?;
    copy_tree(fixture, dir.path())
        .with_context(|| format!("copy fixture {}", fixture.display()))?;
    git(dir.path(), &["init", "-q"])?;
    git(dir.path(), &["add", "-A"])?;
    git(dir.path(), &["commit", "-q", "-m", "baseline"])?;
    for mutation in mutations {
        let target = dir.path().join(&mutation.path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&target, &mutation.contents)
            .with_context(|| format!("apply mutation {}", mutation.path))?;
    }
    Ok(CleanRoom { dir })
}

/// First whitespace token of a command, mapped to the binary that must exist on
/// PATH. `npm run x` / `pnpm test` resolve to their package manager.
fn required_binary(command: &str) -> Option<String> {
    command.split_whitespace().next().map(|s| s.to_string())
}

fn binary_available(bin: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {bin}"))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run one shell command in the clean room, measuring real wall-clock. If the
/// required binary is absent, returns `not_available` without running.
pub fn run_command(dir: &Path, label: &str, command: &str) -> CommandRun {
    if let Some(bin) = required_binary(command) {
        if !binary_available(&bin) {
            return CommandRun {
                label: label.to_string(),
                command: command.to_string(),
                status: RunStatus::NotAvailable,
                exit_code: None,
                duration_ms: 0,
                measured: false,
                output_lines: 0,
            };
        }
    }
    let start = Instant::now();
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(dir)
        .output();
    let duration_ms = start.elapsed().as_millis();
    match output {
        Ok(out) => {
            let lines = String::from_utf8_lossy(&out.stdout).lines().count()
                + String::from_utf8_lossy(&out.stderr).lines().count();
            CommandRun {
                label: label.to_string(),
                command: command.to_string(),
                status: if out.status.success() {
                    RunStatus::Passed
                } else {
                    RunStatus::Failed
                },
                exit_code: out.status.code(),
                duration_ms,
                measured: true,
                output_lines: lines as u64,
            }
        }
        Err(_) => CommandRun {
            label: label.to_string(),
            command: command.to_string(),
            status: RunStatus::NotAvailable,
            exit_code: None,
            duration_ms: 0,
            measured: false,
            output_lines: 0,
        },
    }
}

/// Run the full baseline command set sequentially (the naive agent: build,
/// test, lint, typecheck) in a dedicated clean room.
pub fn run_baseline(dir: &Path, commands: &[(String, String)]) -> Vec<CommandRun> {
    commands
        .iter()
        .map(|(label, cmd)| run_command(dir, label, cmd))
        .collect()
}

/// `vrt verify [--continue] --mode <mode> --no-broker --json`, returning the
/// parsed agent report and measured wall-clock. Does NOT run init.
pub fn run_verify(
    vrt_bin: &Path,
    dir: &Path,
    mode: &str,
    continue_after: bool,
) -> Result<(Value, u128)> {
    let mut args = vec!["verify", "--mode", mode, "--no-broker", "--json"];
    if continue_after {
        args.push("--continue");
    }
    let start = Instant::now();
    let verify = Command::new(vrt_bin)
        .args(&args)
        .current_dir(dir)
        .output()
        .context("vrt verify")?;
    let duration_ms = start.elapsed().as_millis();
    let stdout = String::from_utf8_lossy(&verify.stdout);
    let report: Value = serde_json::from_str(stdout.trim()).with_context(|| {
        format!(
            "parse vrt verify json (stderr: {})",
            String::from_utf8_lossy(&verify.stderr)
        )
    })?;
    Ok((report, duration_ms))
}

/// Run `vrt init` once for a clean room.
pub fn run_init(vrt_bin: &Path, dir: &Path) -> Result<()> {
    let init = Command::new(vrt_bin)
        .args(["init"])
        .current_dir(dir)
        .output()
        .context("vrt init")?;
    if !init.status.success() {
        return Err(anyhow!(
            "vrt init failed: {}",
            String::from_utf8_lossy(&init.stderr)
        ));
    }
    Ok(())
}

/// `vrt init` then a single `vrt verify` (first run of a scenario).
pub fn run_vrt(vrt_bin: &Path, dir: &Path, mode: &str) -> Result<(Value, u128)> {
    run_init(vrt_bin, dir)?;
    run_verify(vrt_bin, dir, mode, false)
}

/// `vrt doctor --json` — profile + capabilities (weak spots).
pub fn run_doctor(vrt_bin: &Path, dir: &Path) -> Value {
    Command::new(vrt_bin)
        .args(["doctor", "--json"])
        .current_dir(dir)
        .output()
        .ok()
        .and_then(|o| serde_json::from_slice(&o.stdout).ok())
        .unwrap_or(Value::Null)
}

/// Apply additional mutations to an existing clean room (the second stage of a
/// multi-step scenario).
pub fn apply_mutations(dir: &Path, mutations: &[Mutation]) -> Result<()> {
    for mutation in mutations {
        let target = dir.join(&mutation.path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&target, &mutation.contents)
            .with_context(|| format!("apply mutation {}", mutation.path))?;
    }
    Ok(())
}

/// Run `vrt explain --json` after a failed verify to capture root-cause /
/// do_not_run semantics.
pub fn run_explain(vrt_bin: &Path, dir: &Path) -> Result<Value> {
    let out = Command::new(vrt_bin)
        .args(["explain", "--json"])
        .current_dir(dir)
        .output()
        .context("vrt explain")?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(serde_json::from_str(stdout.trim()).unwrap_or(Value::Null))
}

fn verify_broker_capture(vrt_bin: &Path, dir: &Path, mode: &str) -> Result<Value> {
    let out = Command::new(vrt_bin)
        .args(["verify", "--mode", mode, "--broker", "--json"])
        .current_dir(dir)
        .output()
        .context("vrt verify --broker")?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(stdout.trim()).with_context(|| {
        format!(
            "parse concurrent verify json (stderr: {})",
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// Run two concurrent `vrt verify --broker --json` against ONE clean room with
/// the SAME diff. The first gets a head start so the singleflight join is
/// deterministic (the second arrives while the leader holds the run lock).
/// Returns (leader_candidate, second). Both must emit parseable JSON.
pub fn run_two_concurrent_verify(
    vrt_bin: &Path,
    dir: &Path,
    mode: &str,
    head_start_ms: u64,
) -> Result<(Value, Value)> {
    let bin = vrt_bin.to_path_buf();
    let d = dir.to_path_buf();
    let m = mode.to_string();
    let first = thread::spawn(move || verify_broker_capture(&bin, &d, &m));
    thread::sleep(Duration::from_millis(head_start_ms));
    let second = verify_broker_capture(vrt_bin, dir, mode);
    let first = first
        .join()
        .map_err(|_| anyhow!("first verify thread panicked"))?;
    Ok((first?, second?))
}

/// Locate (building if needed) the debug `vrt` binary for this workspace.
pub fn ensure_vrt_binary(workspace_root: &Path) -> Result<PathBuf> {
    let status = Command::new("cargo")
        .args(["build", "--quiet", "--package", "vrt-cli"])
        .current_dir(workspace_root)
        .status()
        .context("cargo build vrt-cli")?;
    if !status.success() {
        return Err(anyhow!("failed to build vrt-cli"));
    }
    let bin = workspace_root.join("target/debug/vrt");
    if !bin.exists() {
        return Err(anyhow!("vrt binary not found at {}", bin.display()));
    }
    Ok(bin)
}
