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
