use std::fs;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;
use vrt_core::handle_mcp_message;

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

#[test]
fn mcp_initializes_with_tools_capability() {
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
