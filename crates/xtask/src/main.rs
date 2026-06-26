use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use xtask::{proof, report, runner, scenarios};

#[derive(Parser)]
#[command(name = "xtask", about = "VRT workspace tasks (adversarial proof harness)")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the adversarial proof suite and emit .vrt-proof/.
    Proof {
        /// Output directory for the proof package.
        #[arg(long, default_value = ".vrt-proof")]
        out: PathBuf,
    },
}

fn workspace_root() -> PathBuf {
    // crates/xtask -> crates -> <root>
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(PathBuf::from)
        .expect("workspace root")
}

fn git_commit(root: &std::path::Path) -> String {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Proof { out } => run_proof(out),
    }
}

fn run_proof(out: PathBuf) -> Result<()> {
    let root = workspace_root();
    let xtask_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let commit = git_commit(&root);

    eprintln!("building vrt binary…");
    let vrt_bin = runner::ensure_vrt_binary(&root).context("ensure vrt binary")?;

    let scenarios = scenarios::all_scenarios(&xtask_dir, &root);
    eprintln!("running {} adversarial scenarios…", scenarios.len());
    let run = proof::run_all(&vrt_bin, &scenarios, commit)?;

    let out_dir = if out.is_absolute() { out } else { root.join(out) };
    report::emit(&out_dir, &run).context("emit proof package")?;

    println!("{}", report::console_verdict(&run, &run.metrics));
    println!("Wrote proof package to {}", out_dir.display());

    if matches!(run.overall, xtask::model::OverallVerdict::Fail) {
        std::process::exit(1);
    }
    Ok(())
}
