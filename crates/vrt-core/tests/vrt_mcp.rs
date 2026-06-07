use std::fs;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;
use vrt_core::{handle_broker_message, handle_mcp_message};

fn fixture() -> TempDir {
    let dir = TempDir::new().expect("temp dir");
    fs::write(
        dir.path().join("package.json"),
        r#"{
  "scripts": {
    "typecheck": "echo typecheck ok",
    "test": "echo test ok",
    "build": "echo build ok"
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
    fs::write(dir.path().join("tsconfig.json"), "{}\n").unwrap();
    fs::create_dir_all(dir.path().join("apps/web/components")).unwrap();
    fs::write(
        dir.path().join("apps/web/components/card.tsx"),
        "export function Card() { return null }\n",
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
        dir.path().join("apps/web/components/card.tsx"),
        "export function Card() { return <section /> }\n",
    )
    .unwrap();
    dir
}

fn request(root: &TempDir, json: Value) -> Value {
    let line = serde_json::to_string(&json).unwrap();
    let response = handle_mcp_message(root.path(), &line)
        .expect("mcp response")
        .expect("non-notification response");
    serde_json::from_str(&response).expect("json response")
}

fn broker_request(root: &TempDir, json: Value) -> Value {
    let line = serde_json::to_string(&json).unwrap();
    let response = handle_broker_message(root.path(), &line)
        .expect("broker response")
        .expect("non-empty broker response");
    serde_json::from_str(&response).expect("json response")
}

#[test]
fn broker_status_exposes_bounded_tools_without_shell_escape() {
    let dir = fixture();

    let response = broker_request(
        &dir,
        serde_json::json!({
            "id": "status",
            "op": "status"
        }),
    );

    assert_eq!(response["ok"], true);
    assert_eq!(response["result"]["protocol"], "vrt-broker/1");
    let tools = response["result"]["tools"].as_array().expect("tools");
    assert!(tools.iter().any(|tool| tool == "run_verification"));
    assert!(response["result"]["forbidden"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool == "run_any_shell_command"));
}

#[test]
fn broker_plans_and_runs_verification_with_jsonl_requests() {
    let dir = fixture();

    let plan = broker_request(
        &dir,
        serde_json::json!({
            "id": "plan",
            "op": "plan_verification",
            "arguments": { "mode": "dev" }
        }),
    );
    assert_eq!(plan["ok"], true);
    assert_eq!(plan["result"]["plan"]["mode"], "dev");

    let run = broker_request(
        &dir,
        serde_json::json!({
            "id": "run",
            "op": "run_verification",
            "arguments": { "mode": "dev", "token_profile": "headroom" }
        }),
    );
    assert_eq!(run["ok"], true);
    assert_eq!(run["result"]["token_profile"], "headroom");
    let token_report: Value = serde_json::from_str(run["result"]["token_report"].as_str().unwrap())
        .expect("headroom token report");
    assert_eq!(token_report["profile"], "headroom");
    assert_eq!(
        token_report["refs"]["evidence_id"],
        run["result"]["evidence"]["evidence_id"]
    );
    assert_eq!(run["result"]["evidence"]["schema_version"], 1);
    assert!(run["result"]["evidence"]["broker_job_id"]
        .as_str()
        .expect("broker job id")
        .starts_with("job_"));
    assert!(run["result"]["evidence"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|check| check["status"] == "passed"));
}

#[test]
fn broker_unknown_operation_is_structured_error() {
    let dir = fixture();

    let response = broker_request(
        &dir,
        serde_json::json!({
            "id": "bad",
            "op": "run_any_shell_command",
            "arguments": { "command": "echo no" }
        }),
    );

    assert_eq!(response["ok"], false);
    assert!(response["error"]
        .as_str()
        .unwrap()
        .contains("Unknown broker operation"));
}

#[test]
fn mcp_initializes_with_agent_context_capabilities() {
    let dir = fixture();

    let response = request(
        &dir,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "protocolVersion": "2025-06-18" }
        }),
    );

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert_eq!(
        response["result"]["capabilities"]["tools"]["listChanged"],
        false
    );
    assert_eq!(
        response["result"]["capabilities"]["resources"]["listChanged"],
        false
    );
    assert_eq!(
        response["result"]["capabilities"]["resources"]["subscribe"],
        false
    );
    assert_eq!(
        response["result"]["capabilities"]["prompts"]["listChanged"],
        false
    );
    assert_eq!(response["result"]["serverInfo"]["name"], "vrt");
}

#[test]
fn mcp_lists_structured_tools_without_free_shell_escape_hatch() {
    let dir = fixture();

    let response = request(
        &dir,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": "tools",
            "method": "tools/list"
        }),
    );

    let tools = response["result"]["tools"].as_array().expect("tools");
    let names = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(names.contains(&"analyze_change"));
    assert!(names.contains(&"plan_verification"));
    assert!(names.contains(&"run_verification"));
    assert!(names.contains(&"explain_failure"));
    assert!(names.contains(&"get_evidence"));
    assert!(names.contains(&"escalate_verification"));
    assert!(names.contains(&"get_broker_status"));
    assert!(names.contains(&"list_sessions"));
    assert!(names.contains(&"show_session"));
    assert!(names.contains(&"list_queue"));
    assert!(names.contains(&"cancel_job"));
    assert!(names.contains(&"list_locks"));
    assert!(names.contains(&"start_session"));
    assert!(names.contains(&"close_session"));
    assert!(!names.contains(&"run_any_shell_command"));
    assert!(tools
        .iter()
        .all(|tool| tool["inputSchema"]["type"] == "object"));
    let run_tool = tools
        .iter()
        .find(|tool| tool["name"] == "run_verification")
        .expect("run_verification tool");
    assert_eq!(
        run_tool["inputSchema"]["properties"]["continue"]["type"],
        "boolean"
    );
}

#[test]
fn mcp_tool_call_returns_text_and_structured_content() {
    let dir = fixture();

    let response = request(
        &dir,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "analyze_change",
                "arguments": {}
            }
        }),
    );

    assert_eq!(response["result"]["isError"], false);
    assert_eq!(response["result"]["content"][0]["type"], "text");
    assert_eq!(
        response["result"]["structuredContent"]["change"]["changed_files"][0]["path"],
        "apps/web/components/card.tsx"
    );
}

#[test]
fn mcp_unknown_tool_is_protocol_error() {
    let dir = fixture();

    let response = request(
        &dir,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "run_any_shell_command",
                "arguments": { "command": "echo no" }
            }
        }),
    );

    assert_eq!(response["error"]["code"], -32601);
    assert!(response["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Unknown tool"));
}

#[test]
fn mcp_lists_and_reads_agent_context_resources() {
    let dir = fixture();

    let list = request(
        &dir,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": "resources",
            "method": "resources/list"
        }),
    );

    let resources = list["result"]["resources"].as_array().expect("resources");
    let uris = resources
        .iter()
        .map(|resource| resource["uri"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(uris.contains(&"vrt://profile"));
    assert!(uris.contains(&"vrt://latest-evidence"));
    assert!(uris.contains(&"vrt://skill"));
    assert!(uris.contains(&"vrt://token-rules"));
    assert!(uris.contains(&"vrt://token-compatibility"));
    assert!(resources
        .iter()
        .all(|resource| resource["mimeType"] == "application/json"
            || resource["mimeType"] == "text/markdown"));

    let profile = request(
        &dir,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": "profile",
            "method": "resources/read",
            "params": { "uri": "vrt://profile" }
        }),
    );
    assert_eq!(profile["result"]["contents"][0]["uri"], "vrt://profile");
    assert_eq!(
        profile["result"]["contents"][0]["mimeType"],
        "application/json"
    );
    assert!(profile["result"]["contents"][0]["text"]
        .as_str()
        .unwrap()
        .contains("\"package_manager\""));

    let skill = request(
        &dir,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": "skill",
            "method": "resources/read",
            "params": { "uri": "vrt://skill" }
        }),
    );
    assert!(skill["result"]["contents"][0]["text"]
        .as_str()
        .unwrap()
        .contains("VRT Verification Skill"));

    let token_rules = request(
        &dir,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": "token-rules",
            "method": "resources/read",
            "params": { "uri": "vrt://token-rules" }
        }),
    );
    let token_text = token_rules["result"]["contents"][0]["text"]
        .as_str()
        .unwrap();
    assert!(token_text.contains("RTK"));
    assert!(token_text.contains("Headroom"));

    let token_compatibility = request(
        &dir,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": "token-compatibility",
            "method": "resources/read",
            "params": { "uri": "vrt://token-compatibility" }
        }),
    );
    assert_eq!(
        token_compatibility["result"]["contents"][0]["mimeType"],
        "application/json"
    );
    let manifest: Value = serde_json::from_str(
        token_compatibility["result"]["contents"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .expect("token compatibility manifest");
    assert_eq!(manifest["tools"]["rtk"]["mode"], "cli-proxy");
    assert_eq!(manifest["tools"]["headroom"]["mode"], "structured-context");
}

#[test]
fn mcp_reads_latest_evidence_resource_after_verification() {
    let dir = fixture();

    request(
        &dir,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": "run",
            "method": "tools/call",
            "params": {
                "name": "run_verification",
                "arguments": { "mode": "dev" }
            }
        }),
    );

    let latest = request(
        &dir,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": "latest",
            "method": "resources/read",
            "params": { "uri": "vrt://latest-evidence" }
        }),
    );

    let text = latest["result"]["contents"][0]["text"].as_str().unwrap();
    assert!(text.contains("\"evidence_id\""));
    assert!(text.contains("\"checks\""));
}

#[test]
fn mcp_explain_failure_can_target_specific_evidence_id() {
    let dir = fixture();
    fs::write(
        dir.path().join("package.json"),
        r#"{
  "scripts": {
    "typecheck": "sh -c 'echo \"apps/web/components/card.tsx(10,3): error TS2322: First failure\" >&2; exit 2'",
    "test": "echo test ok",
    "build": "echo build ok"
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
    .expect("first package json");
    let first_run = request(
        &dir,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": "first-run",
            "method": "tools/call",
            "params": {
                "name": "run_verification",
                "arguments": { "mode": "dev" }
            }
        }),
    );
    let first_evidence_id = first_run["result"]["structuredContent"]["evidence"]["evidence_id"]
        .as_str()
        .expect("first evidence id")
        .to_string();

    fs::write(
        dir.path().join("package.json"),
        r#"{
  "scripts": {
    "typecheck": "sh -c 'echo \"apps/web/components/card.tsx(20,4): error TS2322: Second failure\" >&2; exit 2'",
    "test": "echo test ok",
    "build": "echo build ok"
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
    .expect("second package json");
    request(
        &dir,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": "second-run",
            "method": "tools/call",
            "params": {
                "name": "run_verification",
                "arguments": { "mode": "dev" }
            }
        }),
    );

    let explanation = request(
        &dir,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": "explain-first",
            "method": "tools/call",
            "params": {
                "name": "explain_failure",
                "arguments": { "evidence_id": first_evidence_id }
            }
        }),
    );
    let candidates = explanation["result"]["structuredContent"]["explanation"]
        ["root_cause_candidates"]
        .as_array()
        .expect("root cause candidates");

    assert!(candidates
        .iter()
        .any(|candidate| candidate.as_str().unwrap().contains("First failure")));
    assert!(!candidates
        .iter()
        .any(|candidate| candidate.as_str().unwrap().contains("Second failure")));
}

#[test]
fn mcp_lists_and_gets_verification_prompts() {
    let dir = fixture();

    let list = request(
        &dir,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": "prompts",
            "method": "prompts/list"
        }),
    );

    let prompts = list["result"]["prompts"].as_array().expect("prompts");
    let names = prompts
        .iter()
        .map(|prompt| prompt["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(names.contains(&"verify_after_change"));
    assert!(names.contains(&"explain_failure"));
    assert!(names.contains(&"write_verification_report"));
    let verify_prompt = prompts
        .iter()
        .find(|prompt| prompt["name"] == "verify_after_change")
        .expect("verify prompt");
    assert!(verify_prompt["arguments"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg["name"] == "mode"));

    let prompt = request(
        &dir,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": "verify-prompt",
            "method": "prompts/get",
            "params": {
                "name": "verify_after_change",
                "arguments": { "mode": "merge" }
            }
        }),
    );

    assert_eq!(prompt["result"]["messages"][0]["role"], "user");
    assert_eq!(prompt["result"]["messages"][0]["content"]["type"], "text");
    let text = prompt["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap();
    assert!(text.contains("run_verification"));
    assert!(text.contains("mode=merge"));
    assert!(text.contains("resources/read"));
}

#[test]
fn mcp_unknown_resource_or_prompt_is_invalid_params() {
    let dir = fixture();

    let resource = request(
        &dir,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": "bad-resource",
            "method": "resources/read",
            "params": { "uri": "vrt://missing" }
        }),
    );
    assert_eq!(resource["error"]["code"], -32602);

    let prompt = request(
        &dir,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": "bad-prompt",
            "method": "prompts/get",
            "params": { "name": "missing_prompt" }
        }),
    );
    assert_eq!(prompt["error"]["code"], -32602);
}
