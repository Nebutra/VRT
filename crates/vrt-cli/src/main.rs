use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use vrt_core::{
    analyze_change, bench_summary, broker_status, build_capability_graph, cancel_queue_job,
    close_session_context, current_worktree_session, explain_latest, export_report,
    handle_broker_message, handle_mcp_message, human_report, initialize_project, install_skill,
    install_token_rules, list_false_confidence_cases, list_session_contexts,
    list_worktree_sessions, lock_list, lock_show, multi_agent_session_view, plan_verification,
    profile_project, queue_status, record_false_confidence_case, render_agent_report,
    render_token_report, resolve_verification_mode, run_verification, run_verification_brokered,
    run_verification_continue, show_session_context, start_broker, start_worktree_session,
    stop_broker, token_compatibility_manifest, ReportFormat, TokenProfile, VerificationMode,
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
        #[arg(long, value_enum)]
        mode: Option<CliMode>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        full: bool,
        #[arg(long = "continue")]
        continue_after_failure: bool,
        #[arg(long)]
        broker: bool,
        #[arg(long)]
        no_broker: bool,
        #[arg(long, value_enum, default_value_t = CliTokenProfile::Standard)]
        token_profile: CliTokenProfile,
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
    Broker {
        #[command(subcommand)]
        command: BrokerCommand,
    },
    Queue {
        #[command(subcommand)]
        command: QueueCommand,
    },
    Lock {
        #[command(subcommand)]
        command: LockCommand,
    },
    Bench {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        concurrency: bool,
    },
    Report {
        #[arg(long, value_enum)]
        format: CliReportFormat,
        #[arg(long)]
        output: PathBuf,
    },
    Token {
        #[command(subcommand)]
        command: TokenCommand,
    },
    FalseConfidence {
        #[command(subcommand)]
        command: FalseConfidenceCommand,
    },
    Session {
        #[command(subcommand)]
        command: SessionCommand,
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

#[derive(Debug, Subcommand)]
enum BrokerCommand {
    Start {
        #[arg(long)]
        json: bool,
    },
    Stop {
        #[arg(long)]
        json: bool,
    },
    Status {
        #[arg(long)]
        json: bool,
    },
    Serve,
}

#[derive(Debug, Subcommand)]
enum QueueCommand {
    Status {
        #[arg(long)]
        json: bool,
    },
    Cancel {
        job_id: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum LockCommand {
    List {
        #[arg(long)]
        json: bool,
    },
    Show {
        lock_id: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum TokenCommand {
    Doctor {
        #[arg(long)]
        json: bool,
    },
    Manifest {
        #[arg(long)]
        json: bool,
    },
    InstallRules,
}

#[derive(Debug, Subcommand)]
enum FalseConfidenceCommand {
    Record {
        #[arg(long)]
        evidence_id: Option<String>,
        #[arg(long)]
        stricter_check: String,
        #[arg(long)]
        failure_summary: String,
        #[arg(long)]
        json: bool,
    },
    List {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum SessionCommand {
    Start {
        #[arg(long)]
        worktree: PathBuf,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Status {
        #[arg(long)]
        json: bool,
    },
    List {
        #[arg(long)]
        json: bool,
    },
    Show {
        session_id: String,
        #[arg(long)]
        json: bool,
    },
    Close {
        session_id: String,
        #[arg(long)]
        json: bool,
    },
    View {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliMode {
    Dev,
    Merge,
    Release,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliReportFormat {
    Markdown,
    Sarif,
    Junit,
    Otel,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliTokenProfile {
    Standard,
    Rtk,
    Headroom,
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
                if profile.ci.is_empty() {
                    println!("- CI workflows: none detected");
                } else {
                    for workflow in profile.ci {
                        println!("- CI workflow: {} ({})", workflow.name, workflow.path);
                    }
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
                println!("\nCI workflows:");
                if profile.ci.is_empty() {
                    println!("- none detected");
                } else {
                    for workflow in &profile.ci {
                        println!("- {}: {}", workflow.name, workflow.path);
                    }
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
            dry_run,
            full,
            continue_after_failure,
            broker,
            no_broker,
            token_profile,
        } => {
            if broker && no_broker {
                anyhow::bail!("--broker and --no-broker cannot be used together");
            }
            let profile = initialize_project(&root)?;
            let graph = build_capability_graph(&root, &profile)?;
            let change = analyze_change(&root, &profile)?;
            let mode = resolve_verification_mode(&root, mode.map(Into::into))?;
            let mut plan = plan_verification(&profile, &graph, &change, mode)?;
            if full {
                add_full_build_if_available(&graph, &mut plan);
            }
            if dry_run {
                if json {
                    print_json(&plan)?;
                } else {
                    print!("{}", human_plan(&plan));
                }
                return Ok(());
            }
            let broker_running = broker_status(&root)["broker_state"]["running"]
                .as_bool()
                .unwrap_or(false);
            let use_broker = !continue_after_failure && (broker || (!no_broker && broker_running));
            let evidence = if continue_after_failure {
                run_verification_continue(&root, &profile, &change, &plan)?
            } else if use_broker {
                run_verification_brokered(&root, &profile, &change, &plan)?
            } else {
                run_verification(&root, &profile, &change, &plan)?
            };
            if json {
                print_json(&render_agent_report(&root, &evidence))?;
            } else if !matches!(token_profile, CliTokenProfile::Standard) {
                print!("{}", render_token_report(&evidence, token_profile.into()));
            } else {
                print!("{}", human_report(&evidence));
            }
            if evidence.checks.iter().any(|check| check.status == "failed") {
                io::stdout().flush()?;
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
        Command::Broker { command } => match command {
            BrokerCommand::Start { json } => {
                let state = start_broker(&root)?;
                if json {
                    print_json(&state)?;
                } else {
                    println!("VRT broker started.");
                    println!("Socket:");
                    println!(
                        "{}",
                        state["socket_path"]
                            .as_str()
                            .unwrap_or(".vrt/broker/vrt.sock")
                    );
                    println!("Capabilities:");
                    for cap in state["capabilities"].as_array().into_iter().flatten() {
                        println!("- {}", cap.as_str().unwrap_or("unknown"));
                    }
                }
            }
            BrokerCommand::Stop { json } => {
                let state = stop_broker(&root)?;
                if json {
                    print_json(&state)?;
                } else {
                    println!("VRT broker stopped.");
                    println!("- running: {}", state["running"].as_bool().unwrap_or(false));
                }
            }
            BrokerCommand::Status { json } => {
                let status = broker_status(&root);
                if json {
                    print_json(&status)?;
                } else {
                    println!("VRT broker:");
                    println!(
                        "- running: {}",
                        status["broker_state"]["running"].as_bool().unwrap_or(false)
                    );
                    println!(
                        "- protocol: {}",
                        status["protocol"].as_str().unwrap_or("unknown")
                    );
                    println!("- root: {}", status["root"]);
                    println!(
                        "- active lock: {}",
                        if status["active_lock"].is_null() {
                            "none"
                        } else {
                            "present"
                        }
                    );
                    println!(
                        "- latest evidence: {}",
                        status["latest_evidence"]["evidence_id"]
                            .as_str()
                            .unwrap_or("none")
                    );
                }
            }
            BrokerCommand::Serve => {
                serve_broker(&root)?;
            }
        },
        Command::Queue { command } => match command {
            QueueCommand::Status { json } => {
                let status = queue_status(&root);
                if json {
                    print_json(&status)?;
                } else {
                    println!("Verification queue:");
                    println!("- queued: {}", status["queued_jobs"].as_u64().unwrap_or(0));
                    println!(
                        "- running: {}",
                        status["running_jobs"].as_u64().unwrap_or(0)
                    );
                    println!(
                        "- expensive pool: {}/{}",
                        status["runner_pool"]["expensive"]["running"]
                            .as_u64()
                            .unwrap_or(0),
                        status["runner_pool"]["expensive"]["limit"]
                            .as_u64()
                            .unwrap_or(1)
                    );
                }
            }
            QueueCommand::Cancel { job_id, json } => {
                let result = cancel_queue_job(&root, &job_id)?;
                if json {
                    print_json(&result)?;
                } else {
                    println!("Queue cancel:");
                    println!("- job_id: {}", result["job_id"].as_str().unwrap_or(&job_id));
                    println!(
                        "- status: {}",
                        result["status"].as_str().unwrap_or("unknown")
                    );
                }
            }
        },
        Command::Lock { command } => match command {
            LockCommand::List { json } => {
                let locks = lock_list(&root);
                if json {
                    print_json(&locks)?;
                } else {
                    println!("Locks:");
                    if locks["locks"]
                        .as_array()
                        .is_none_or(|items| items.is_empty())
                    {
                        println!("- none");
                    } else {
                        for lock in locks["locks"].as_array().into_iter().flatten() {
                            println!(
                                "- {} kind={} mode={} status={}",
                                lock["resource_id"].as_str().unwrap_or("unknown"),
                                lock["kind"].as_str().unwrap_or("unknown"),
                                lock["mode"].as_str().unwrap_or("unknown"),
                                lock["status"].as_str().unwrap_or("unknown")
                            );
                        }
                    }
                }
            }
            LockCommand::Show { lock_id, json } => {
                let lock = lock_show(&root, &lock_id)?;
                if json {
                    print_json(&lock)?;
                } else {
                    println!("Lock:");
                    println!(
                        "- resource_id: {}",
                        lock["resource_id"].as_str().unwrap_or(&lock_id)
                    );
                    println!("- kind: {}", lock["kind"].as_str().unwrap_or("unknown"));
                    println!("- mode: {}", lock["mode"].as_str().unwrap_or("unknown"));
                    println!("- reason: {}", lock["reason"].as_str().unwrap_or(""));
                }
            }
        },
        Command::Bench { json, concurrency } => {
            let mut summary = bench_summary(&root)?;
            if concurrency {
                summary["concurrency"] = serde_json::json!({
                    "queue": queue_status(&root),
                    "locks": lock_list(&root),
                    "broker": broker_status(&root)
                });
            }
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
        Command::Token { command } => match command {
            TokenCommand::Doctor { json } => {
                let report = token_doctor();
                if json {
                    print_json(&report)?;
                } else {
                    println!("Token-saving compatibility:");
                    println!(
                        "- rtk: {}",
                        if report["rtk"]["available"].as_bool().unwrap_or(false) {
                            "available"
                        } else {
                            "not found"
                        }
                    );
                    println!(
                        "- headroom: {}",
                        if report["headroom"]["available"].as_bool().unwrap_or(false) {
                            "available"
                        } else {
                            "not found"
                        }
                    );
                    println!("\nRecommended:");
                    println!("- vrt verify --token-profile rtk");
                    println!("- vrt verify --token-profile headroom");
                    println!("- vrt token manifest --json");
                    println!("- vrt token install-rules");
                }
            }
            TokenCommand::Manifest { json } => {
                let manifest = token_compatibility_manifest();
                if json {
                    print_json(&manifest)?;
                } else {
                    println!("VRT token-saving compatibility manifest:");
                    println!("{}", serde_json::to_string_pretty(&manifest)?);
                }
            }
            TokenCommand::InstallRules => {
                install_token_rules(&root)?;
                println!("Installed token-saving compatibility rules:");
                println!("- .vrt/token-saving/RTK_HEADROOM.md");
                println!("- AGENTS.md");
            }
        },
        Command::FalseConfidence { command } => match command {
            FalseConfidenceCommand::Record {
                evidence_id,
                stricter_check,
                failure_summary,
                json,
            } => {
                let case = record_false_confidence_case(
                    &root,
                    evidence_id.as_deref(),
                    &stricter_check,
                    &failure_summary,
                )?;
                if json {
                    print_json(&case)?;
                } else {
                    println!("Recorded false-confidence case:");
                    println!("- case_id: {}", case.case_id);
                    println!("- evidence_id: {}", case.evidence_id);
                    println!("- stricter_check: {}", case.stricter_check);
                    println!("- failure_summary: {}", case.failure_summary);
                }
            }
            FalseConfidenceCommand::List { json } => {
                let cases = list_false_confidence_cases(&root)?;
                if json {
                    print_json(&cases)?;
                } else if cases.is_empty() {
                    println!("No false-confidence cases recorded.");
                } else {
                    println!("False-confidence cases:");
                    for case in cases {
                        println!(
                            "- {} evidence={} check={} failure={}",
                            case.case_id,
                            case.evidence_id,
                            case.stricter_check,
                            case.failure_summary
                        );
                    }
                }
            }
        },
        Command::Session { command } => match command {
            SessionCommand::Start {
                worktree,
                branch,
                json,
            } => {
                let session = start_worktree_session(&root, &worktree, branch.as_deref())?;
                if json {
                    print_json(&session)?;
                } else {
                    println!("Started VRT worktree session:");
                    println!("- session_id: {}", session.session_id);
                    println!("- worktree: {}", session.worktree_path);
                    println!("- branch: {}", session.branch);
                    println!("\nNext:");
                    for instruction in session.instructions {
                        println!("- {instruction}");
                    }
                }
            }
            SessionCommand::Status { json } => {
                let session = current_worktree_session(&root)?;
                if json {
                    print_json(&session)?;
                } else {
                    println!("VRT session:");
                    println!("- session_id: {}", session.session_id);
                    println!("- status: {}", session.status);
                    println!("- branch: {}", session.branch);
                    println!("- worktree: {}", session.worktree_path);
                    println!("- base_commit: {}", session.base_commit);
                }
            }
            SessionCommand::List { json } => {
                let session_contexts = list_session_contexts(&root)?;
                let sessions = list_worktree_sessions(&root)?;
                if json {
                    print_json(&serde_json::json!({
                        "sessions": session_contexts,
                        "worktree_sessions": sessions,
                    }))?;
                } else if session_contexts.is_empty() && sessions.is_empty() {
                    println!("No VRT sessions recorded.");
                } else {
                    println!("VRT sessions:");
                    for session in session_contexts {
                        println!(
                            "- {} agent={} status={} dirty={} evidence={}",
                            session.session_id,
                            session.agent_kind,
                            session.status,
                            session.dirty_state,
                            session.last_evidence_id.as_deref().unwrap_or("none")
                        );
                    }
                    if !sessions.is_empty() {
                        println!("\nVRT worktree sessions:");
                    }
                    for session in sessions {
                        println!(
                            "- {} branch={} worktree={} status={}",
                            session.session_id,
                            session.branch,
                            session.worktree_path,
                            session.status
                        );
                    }
                }
            }
            SessionCommand::Show { session_id, json } => {
                let session = show_session_context(&root, &session_id)?;
                if json {
                    print_json(&session)?;
                } else {
                    println!("VRT session:");
                    println!("- session_id: {}", session.session_id);
                    println!("- status: {}", session.status);
                    println!("- agent: {}", session.agent_kind);
                    println!("- working_dir: {}", session.working_dir);
                    println!("- diff_hash: {}", session.diff_hash);
                    println!(
                        "- last_evidence: {}",
                        session.last_evidence_id.as_deref().unwrap_or("none")
                    );
                }
            }
            SessionCommand::Close { session_id, json } => {
                let session = close_session_context(&root, &session_id)?;
                if json {
                    print_json(&session)?;
                } else {
                    println!("Closed VRT session:");
                    println!("- session_id: {}", session.session_id);
                    println!("- status: {}", session.status);
                }
            }
            SessionCommand::View { json } => {
                let view = multi_agent_session_view(&root)?;
                if json {
                    print_json(&view)?;
                } else if view.sessions.is_empty() {
                    println!("No VRT worktree sessions recorded.");
                } else {
                    println!("VRT multi-Agent session view:");
                    for item in view.sessions {
                        let evidence = item
                            .latest_evidence
                            .as_ref()
                            .map(|evidence| {
                                format!(
                                    "{} validity={} failed={} release={}",
                                    evidence.evidence_id,
                                    evidence.validity,
                                    evidence.checks_failed,
                                    evidence.confidence.release
                                )
                            })
                            .unwrap_or_else(|| "no evidence".to_string());
                        let lock = if item.active_lock.is_some() {
                            "locked"
                        } else {
                            "unlocked"
                        };
                        println!(
                            "- {} branch={} {} evidence={} false_confidence_cases={}",
                            item.session.session_id,
                            item.session.branch,
                            lock,
                            evidence,
                            item.false_confidence_cases
                        );
                    }
                }
            }
        },
    }
    Ok(())
}

fn token_doctor() -> serde_json::Value {
    let rtk = command_available("rtk");
    let headroom = command_available("headroom");
    serde_json::json!({
        "rtk": {
            "available": rtk,
            "recommended_verify": "vrt verify --token-profile rtk",
            "proxy_usage": "rtk vrt verify --token-profile rtk"
        },
        "headroom": {
            "available": headroom,
            "recommended_verify": "vrt verify --token-profile headroom",
            "mcp_safe": true
        },
        "rules": ".vrt/token-saving/RTK_HEADROOM.md",
        "manifest": "vrt token manifest --json",
        "preserve": ["evidence=", "report=", "raw=", "raw_log", ".vrt/evidence"]
    })
}

fn command_available(command: &str) -> bool {
    ProcessCommand::new(command)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
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

fn serve_broker(root: &std::path::Path) -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = handle_broker_message(root, &line)? {
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
            if serde_json::from_str::<serde_json::Value>(&line)
                .ok()
                .and_then(|value| {
                    value
                        .get("op")
                        .or_else(|| value.get("method"))
                        .and_then(|op| op.as_str())
                        .map(|op| op == "shutdown")
                })
                .unwrap_or(false)
            {
                break;
            }
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
            safety_level: cap.safety_level.clone(),
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

fn human_plan(plan: &vrt_core::VerificationPlan) -> String {
    let mut out = String::new();
    out.push_str(&format!("Plan {}\n\n", plan.plan_id));
    out.push_str("Steps:\n");
    if plan.steps.is_empty() {
        out.push_str("- none\n");
    } else {
        for step in &plan.steps {
            out.push_str(&format!(
                "- {}: {}\n  Reason: {}\n",
                step.capability_id, step.command, step.reason
            ));
        }
    }
    out.push_str("\nSkipped:\n");
    if plan.skipped.is_empty() {
        out.push_str("- none\n");
    } else {
        for skipped in &plan.skipped {
            out.push_str(&format!(
                "- {}\n  Reason: {}\n  Residual risk: {}\n",
                skipped.capability_id, skipped.reason, skipped.residual_risk
            ));
        }
    }
    out.push_str("\nExpected confidence:\n");
    out.push_str(&format!("- local: {}\n", plan.expected_confidence.local));
    out.push_str(&format!("- merge: {}\n", plan.expected_confidence.merge));
    out.push_str(&format!(
        "- release: {}\n",
        plan.expected_confidence.release
    ));
    out
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
            CliReportFormat::Markdown => ReportFormat::Markdown,
            CliReportFormat::Sarif => ReportFormat::Sarif,
            CliReportFormat::Junit => ReportFormat::Junit,
            CliReportFormat::Otel => ReportFormat::Otel,
        }
    }
}

impl CliReportFormat {
    fn label(self) -> &'static str {
        match self {
            CliReportFormat::Markdown => "Markdown",
            CliReportFormat::Sarif => "SARIF",
            CliReportFormat::Junit => "JUnit",
            CliReportFormat::Otel => "OpenTelemetry",
        }
    }
}

impl From<CliTokenProfile> for TokenProfile {
    fn from(value: CliTokenProfile) -> Self {
        match value {
            CliTokenProfile::Standard => TokenProfile::Standard,
            CliTokenProfile::Rtk => TokenProfile::Rtk,
            CliTokenProfile::Headroom => TokenProfile::Headroom,
        }
    }
}
