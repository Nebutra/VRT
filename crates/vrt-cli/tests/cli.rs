use std::fs;
use std::process::Command;

use tempfile::TempDir;

fn fixture() -> TempDir {
    let dir = TempDir::new().expect("temp dir");
    fs::write(
        dir.path().join("package.json"),
        r#"{
  "scripts": {
    "typecheck": "tsc --noEmit",
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
    .expect("package json");
    fs::write(
        dir.path().join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\n",
    )
    .expect("lockfile");
    fs::write(dir.path().join("tsconfig.json"), "{}\n").expect("tsconfig");
    fs::create_dir_all(dir.path().join("apps/web/components")).expect("components dir");
    fs::write(
        dir.path().join("apps/web/components/pricing-card.tsx"),
        "export function PricingCard() { return <div /> }\n",
    )
    .expect("component");
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
    .expect("component change");
    dir
}

#[test]
fn verify_dry_run_json_returns_plan_without_writing_evidence() {
    let dir = fixture();
    let output = Command::new(env!("CARGO_BIN_EXE_vrt"))
        .args([
            "--root",
            dir.path().to_str().unwrap(),
            "verify",
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("vrt verify dry run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("dry-run plan json");
    assert_eq!(value["mode"], "dev");
    assert!(!value["plan_id"].as_str().unwrap().is_empty());
    assert!(value["steps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|step| step["capability_id"] == "workspace-typecheck"));
    assert!(value["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .any(|skip| skip["capability_id"] == "workspace-build"));
    assert!(!dir.path().join(".vrt/evidence").exists());
}

#[test]
fn verify_broker_json_records_broker_job_evidence() {
    let dir = fixture();
    fs::remove_file(dir.path().join("pnpm-lock.yaml")).expect("remove pnpm lock");
    fs::write(dir.path().join("package-lock.json"), "{}\n").expect("npm lock");
    fs::write(
        dir.path().join("package.json"),
        r#"{
  "scripts": {
    "typecheck": "echo typecheck ok",
    "build": "echo build ok"
  },
  "devDependencies": {
    "typescript": "5.9.0"
  }
}"#,
    )
    .expect("package json");

    let output = Command::new(env!("CARGO_BIN_EXE_vrt"))
        .args([
            "--root",
            dir.path().to_str().unwrap(),
            "verify",
            "--broker",
            "--json",
        ])
        .output()
        .expect("vrt verify broker");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("broker verify json");
    let broker_job_id = value["evidence"]["broker_job_id"]
        .as_str()
        .expect("broker job id");
    assert!(broker_job_id.starts_with("job_"));
    assert_eq!(value["evidence"]["singleflight"]["role"], "none");
    assert!(dir
        .path()
        .join(".vrt/broker/jobs")
        .join(format!("{broker_job_id}.json"))
        .exists());
}

#[test]
fn token_manifest_json_reports_rtk_and_headroom_contracts() {
    let dir = fixture();
    let output = Command::new(env!("CARGO_BIN_EXE_vrt"))
        .args([
            "--root",
            dir.path().to_str().unwrap(),
            "token",
            "manifest",
            "--json",
        ])
        .output()
        .expect("vrt token manifest");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("token manifest json");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["tools"]["rtk"]["mode"], "cli-proxy");
    assert_eq!(value["tools"]["headroom"]["mode"], "structured-context");
    assert!(value["preserve"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item == ".vrt/evidence"));
}

#[test]
fn session_show_and_close_manage_session_registry_entries() {
    let dir = fixture();
    let registry = dir.path().join(".vrt/session-registry");
    fs::create_dir_all(&registry).expect("session registry");
    fs::write(
        registry.join("session_cli.json"),
        serde_json::json!({
            "schema_version": 1,
            "session_id": "session_cli",
            "name": null,
            "agent_kind": "codex",
            "repo_root": dir.path().canonicalize().unwrap().display().to_string(),
            "working_dir": dir.path().canonicalize().unwrap().display().to_string(),
            "worktree": {
                "enabled": false,
                "path": null,
                "branch": "main",
                "managed_by_vrt": false
            },
            "base_commit": "abc123",
            "current_head": "abc123",
            "diff_hash": "def456",
            "dirty_state": "dirty",
            "created_at": "2026-06-07T00:00:00Z",
            "last_seen_at": "2026-06-07T00:00:00Z",
            "status": "active",
            "last_evidence_id": "ev_cli"
        })
        .to_string(),
    )
    .expect("write session");

    let show = Command::new(env!("CARGO_BIN_EXE_vrt"))
        .args([
            "--root",
            dir.path().to_str().unwrap(),
            "session",
            "show",
            "session_cli",
            "--json",
        ])
        .output()
        .expect("session show");

    assert!(
        show.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&show.stderr)
    );
    let shown: serde_json::Value = serde_json::from_slice(&show.stdout).expect("show json");
    assert_eq!(shown["session_id"], "session_cli");
    assert_eq!(shown["status"], "active");

    let close = Command::new(env!("CARGO_BIN_EXE_vrt"))
        .args([
            "--root",
            dir.path().to_str().unwrap(),
            "session",
            "close",
            "session_cli",
            "--json",
        ])
        .output()
        .expect("session close");

    assert!(
        close.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&close.stderr)
    );
    let closed: serde_json::Value = serde_json::from_slice(&close.stdout).expect("close json");
    assert_eq!(closed["session_id"], "session_cli");
    assert_eq!(closed["status"], "closed");
}

#[test]
fn broker_start_status_and_stop_manage_repo_local_state() {
    let dir = fixture();
    let start = Command::new(env!("CARGO_BIN_EXE_vrt"))
        .args([
            "--root",
            dir.path().to_str().unwrap(),
            "broker",
            "start",
            "--json",
        ])
        .output()
        .expect("broker start");

    assert!(
        start.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&start.stderr)
    );
    let started: serde_json::Value = serde_json::from_slice(&start.stdout).expect("start json");
    assert_eq!(started["running"], true);
    assert!(started["socket_path"]
        .as_str()
        .expect("socket path")
        .ends_with(".vrt/broker/vrt.sock"));
    assert_eq!(started["runner_pool"]["cheap"]["limit"], 4);
    assert_eq!(started["queue"]["queued_jobs"], 0);
    assert!(dir.path().join(".vrt/broker/state.json").exists());

    let status = Command::new(env!("CARGO_BIN_EXE_vrt"))
        .args([
            "--root",
            dir.path().to_str().unwrap(),
            "broker",
            "status",
            "--json",
        ])
        .output()
        .expect("broker status");
    assert!(status.status.success());
    let status_json: serde_json::Value =
        serde_json::from_slice(&status.stdout).expect("status json");
    assert_eq!(status_json["broker_state"]["running"], true);
    assert_eq!(status_json["broker_state"]["locks"]["held"], 0);

    let stop = Command::new(env!("CARGO_BIN_EXE_vrt"))
        .args([
            "--root",
            dir.path().to_str().unwrap(),
            "broker",
            "stop",
            "--json",
        ])
        .output()
        .expect("broker stop");
    assert!(stop.status.success());
    let stopped: serde_json::Value = serde_json::from_slice(&stop.stdout).expect("stop json");
    assert_eq!(stopped["running"], false);
}

#[test]
fn queue_status_and_lock_list_expose_broker_control_plane() {
    let dir = fixture();
    let queue = Command::new(env!("CARGO_BIN_EXE_vrt"))
        .args([
            "--root",
            dir.path().to_str().unwrap(),
            "queue",
            "status",
            "--json",
        ])
        .output()
        .expect("queue status");
    assert!(
        queue.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&queue.stderr)
    );
    let queue_json: serde_json::Value = serde_json::from_slice(&queue.stdout).expect("queue json");
    assert_eq!(queue_json["queued_jobs"], 0);
    assert_eq!(queue_json["running_jobs"], 0);
    assert_eq!(queue_json["runner_pool"]["expensive"]["limit"], 1);

    let locks = Command::new(env!("CARGO_BIN_EXE_vrt"))
        .args([
            "--root",
            dir.path().to_str().unwrap(),
            "lock",
            "list",
            "--json",
        ])
        .output()
        .expect("lock list");
    assert!(
        locks.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&locks.stderr)
    );
    let locks_json: serde_json::Value = serde_json::from_slice(&locks.stdout).expect("locks json");
    assert_eq!(locks_json["held"], 0);
    assert!(locks_json["locks"]
        .as_array()
        .expect("locks array")
        .is_empty());
}

#[test]
fn queue_cancel_marks_queued_job_cancelled() {
    let dir = fixture();
    let jobs_dir = dir.path().join(".vrt/broker/jobs");
    fs::create_dir_all(&jobs_dir).expect("jobs dir");
    let job_path = jobs_dir.join("job_cli_cancel.json");
    fs::write(
        &job_path,
        serde_json::json!({
            "schema_version": 1,
            "job_id": "job_cli_cancel",
            "session_id": "session_cli",
            "plan_id": "plan_cli",
            "status": "queued",
            "cost": "medium",
            "created_at": "2026-06-08T00:00:00Z",
            "updated_at": "2026-06-08T00:00:00Z"
        })
        .to_string(),
    )
    .expect("job json");

    let cancel = Command::new(env!("CARGO_BIN_EXE_vrt"))
        .args([
            "--root",
            dir.path().to_str().unwrap(),
            "queue",
            "cancel",
            "job_cli_cancel",
            "--json",
        ])
        .output()
        .expect("queue cancel");

    assert!(
        cancel.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&cancel.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&cancel.stdout).expect("cancel json");
    let updated: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(job_path).expect("updated job"))
            .expect("updated job json");
    assert_eq!(result["status"], "cancelled");
    assert_eq!(result["queue"]["cancelled_jobs"], 1);
    assert_eq!(updated["status"], "cancelled");
}

#[test]
fn bench_concurrency_json_includes_broker_control_plane() {
    let dir = fixture();
    let jobs_dir = dir.path().join(".vrt/broker/jobs");
    fs::create_dir_all(&jobs_dir).expect("jobs dir");
    fs::write(
        jobs_dir.join("job_cli_waiting.json"),
        serde_json::json!({
            "schema_version": 1,
            "job_id": "job_cli_waiting",
            "session_id": "session_cli",
            "plan_id": "plan_cli",
            "status": "queued",
            "cost": "medium",
            "created_at": "2026-06-08T00:00:00Z",
            "updated_at": "2026-06-08T00:00:00Z"
        })
        .to_string(),
    )
    .expect("job json");

    let bench = Command::new(env!("CARGO_BIN_EXE_vrt"))
        .args([
            "--root",
            dir.path().to_str().unwrap(),
            "bench",
            "--concurrency",
            "--json",
        ])
        .output()
        .expect("bench concurrency");

    assert!(
        bench.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&bench.stderr)
    );
    let result: serde_json::Value =
        serde_json::from_slice(&bench.stdout).expect("bench concurrency json");
    assert_eq!(result["concurrency"]["queue"]["queued_jobs"], 1);
    assert_eq!(
        result["concurrency"]["queue"]["waiting"][0]["job_id"],
        "job_cli_waiting"
    );
    assert!(result["concurrency"]["locks"]["locks"].is_array());
    assert!(result["concurrency"]["broker"]["broker_state"]["running"].is_boolean());
}
