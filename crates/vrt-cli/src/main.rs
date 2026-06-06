use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use vrt_core::{
    analyze_change, bench_summary, build_capability_graph, explain_latest, export_report,
    handle_mcp_message, human_report, initialize_project, install_skill, plan_verification,
    profile_project, run_verification, run_verification_continue, ReportFormat, VerificationMode,
};

#[derive(Debug, Parser)]
#[command(
    name = "vrt",
    version,
    about = "Agent-native local verification runtime"
)]
struct Cli {
    #[arg(long, global = true, default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Init {
        #[arg(long)]
        json: bool,
    },
    Doctor {
        #[arg(long)]
        json: bool,
    },
    Verify {
        #[arg(long, value_enum, default_value_t = CliMode::Dev)]
        mode: CliMode,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        full: bool,
        #[arg(long = "continue")]
        continue_after_failure: bool,
    },
    Explain {
        #[arg(long)]
        json: bool,
    },
    Skill {
        #[command(subcommand)]
        command: SkillCommand,
    },
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
    Bench {
        #[arg(long)]
        json: bool,
    },
    Report {
        #[arg(long, value_enum)]
        format: CliReportFormat,
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum SkillCommand {
    Install,
}

#[derive(Debug, Subcommand)]
enum McpCommand {
    Serve,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliMode {
    Dev,
    Merge,
    Release,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliReportFormat {
    Sarif,
    Junit,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = cli.root.canonicalize().unwrap_or(cli.root);
    match cli.command {
        Command::Init { json } => {
            let profile = initialize_project(&root)?;
            if json {
                print_json(&profile)?;
            } else {
                println!("Project understood.");
                println!("Generated:");
                println!("- .vrt/profile.json");
                println!("- .vrt/config.toml");
                println!("\nDetected:");
                println!("- package manager: {:?}", profile.package_manager);
                println!("- workspace: {}", profile.workspace_kind);
                for item in profile
                    .languages
                    .iter()
                    .chain(profile.frameworks.iter())
                    .chain(profile.tools.iter())
                {
                    println!("- {item:?}");
                }
            }
        }
        Command::Doctor { json } => {
            let profile = profile_project(&root)?;
            let graph = build_capability_graph(&root, &profile)?;
            if json {
                print_json(&serde_json::json!({
                    "profile": profile,
                    "capabilities": graph.capabilities,
                }))?;
            } else {
                println!("Project understood.");
                println!("\nVerification capabilities:");
                for cap in graph.capabilities {
                    println!("- {}: {}", cap.id, cap.command);
                }
                println!("\nWeak spots:");
                if profile.weak_spots.is_empty() {
                    println!("- none detected");
                } else {
                    for weak in profile.weak_spots {
                        println!("- {}", weak.message);
                    }
                }
            }
        }
        Command::Verify {
            mode,
            json,
            full,
            continue_after_failure,
        } => {
            let profile = initialize_project(&root)?;
            let graph = build_capability_graph(&root, &profile)?;
            let change = analyze_change(&root, &profile)?;
            let mut plan = plan_verification(&profile, &graph, &change, mode.into())?;
            if full {
                add_full_build_if_available(&graph, &mut plan);
            }
            let evidence = if continue_after_failure {
                run_verification_continue(&root, &profile, &change, &plan)?
            } else {
                run_verification(&root, &profile, &change, &plan)?
            };
            if json {
                print_json(&evidence)?;
            } else {
                print!("{}", human_report(&evidence));
            }
            if evidence.checks.iter().any(|check| check.status == "failed") {
                std::process::exit(1);
            }
        }
        Command::Explain { json } => {
            let explanation = explain_latest(&root)?;
            if json {
                print_json(&explanation)?;
            } else {
                println!("Status: {}", explanation.status);
                println!("Failure kind: {}", explanation.failure_kind);
                println!("\nRoot cause candidates:");
                for candidate in explanation.root_cause_candidates {
                    println!("- {candidate}");
                }
                println!("\nRecommended:");
                println!("- {}", explanation.recommended_next_action);
                println!("\nDo not run:");
                for skipped in explanation.do_not_run {
                    println!("- {}: {}", skipped.command, skipped.reason);
                }
                if let Some(raw_log) = explanation.raw_log {
                    println!("\nRaw log: {raw_log}");
                }
            }
        }
        Command::Skill { command } => match command {
            SkillCommand::Install => {
                install_skill(&root)?;
                println!("Installed VRT skill files:");
                println!("- .vrt/skill/VRT.md");
                println!("- AGENTS.md");
            }
        },
        Command::Mcp { command } => match command {
            McpCommand::Serve => {
                serve_mcp(&root)?;
            }
        },
        Command::Bench { json } => {
            let summary = bench_summary(&root)?;
            if json {
                print_json(&summary)?;
            } else {
                println!("Verification bench summary:");
                println!("{}", serde_json::to_string_pretty(&summary)?);
            }
        }
        Command::Report { format, output } => {
            export_report(&root, format.into(), &output)?;
            println!("Wrote {} report to {}", format.label(), output.display());
        }
    }
    Ok(())
}

fn serve_mcp(root: &std::path::Path) -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = handle_mcp_message(root, &line)? {
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

fn add_full_build_if_available(
    graph: &vrt_core::CapabilityGraph,
    plan: &mut vrt_core::VerificationPlan,
) {
    if plan
        .steps
        .iter()
        .any(|step| step.capability_id.contains("build"))
    {
        return;
    }
    if let Some(cap) = graph.capabilities.iter().find(|cap| cap.kind == "build") {
        let order = plan.steps.len() as u32 + 1;
        plan.steps.push(vrt_core::PlanStep {
            id: format!("step_{order}"),
            capability_id: cap.id.clone(),
            command: cap.command.clone(),
            cwd: cap.cwd.clone(),
            reason: "--full requested production build proof.".to_string(),
            order,
            stop_on_failure: true,
            timeout_ms: Some(600_000),
        });
        plan.skipped.retain(|skip| skip.capability_id != cap.id);
    }
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

impl From<CliMode> for VerificationMode {
    fn from(value: CliMode) -> Self {
        match value {
            CliMode::Dev => VerificationMode::Dev,
            CliMode::Merge => VerificationMode::Merge,
            CliMode::Release => VerificationMode::Release,
        }
    }
}

impl From<CliReportFormat> for ReportFormat {
    fn from(value: CliReportFormat) -> Self {
        match value {
            CliReportFormat::Sarif => ReportFormat::Sarif,
            CliReportFormat::Junit => ReportFormat::Junit,
        }
    }
}

impl CliReportFormat {
    fn label(self) -> &'static str {
        match self {
            CliReportFormat::Sarif => "SARIF",
            CliReportFormat::Junit => "JUnit",
        }
    }
}
