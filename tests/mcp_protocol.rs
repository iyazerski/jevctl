use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::{Value, json};

struct McpServer {
    child: Child,
    stdin: ChildStdin,
    responses: Receiver<String>,
    reader: Option<JoinHandle<()>>,
}

impl McpServer {
    fn start() -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_jevctl"))
            .arg("mcp")
            .arg("serve")
            .env("TYPESAFE_API_KEY", "protocol-test")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to start jevctl mcp server");
        Self::from_child(child)
    }

    fn start_live() -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_jevctl"))
            .arg("mcp")
            .arg("serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to start live jevctl mcp server");
        Self::from_child(child)
    }

    fn from_child(mut child: Child) -> Self {
        let stdin = child.stdin.take().expect("missing mcp stdin");
        let stdout = child.stdout.take().expect("missing mcp stdout");
        let (sender, responses) = mpsc::channel();
        let reader = thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stdin,
            responses,
            reader: Some(reader),
        }
    }

    fn initialize(&mut self) -> Value {
        self.send(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "jevctl-test", "version": "0.0.0"}
            }
        }));
        let response = self.read_response();
        assert!(response.get("result").is_some(), "{response}");
        self.send(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }));
        response
    }

    fn send(&mut self, message: Value) {
        serde_json::to_writer(&mut self.stdin, &message).unwrap();
        self.stdin.write_all(b"\n").unwrap();
        self.stdin.flush().unwrap();
    }

    fn read_response(&self) -> Value {
        let line = self
            .responses
            .recv_timeout(Duration::from_secs(30))
            .expect("mcp server did not respond");
        serde_json::from_str(&line).expect("mcp server returned invalid JSON")
    }
}

impl Drop for McpServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[test]
fn rejects_unknown_pre_init_request_and_stays_running() {
    let mut server = McpServer::start();
    server.send(json!({
        "jsonrpc": "2.0", "id": 9, "method": "server/discover", "params": {}
    }));
    let response = server.read_response();
    assert_eq!(response["id"], 9);
    assert_eq!(response["error"]["code"], -32601);
    server.initialize();
}

#[test]
fn ignores_unknown_pre_init_notification_and_stays_running() {
    let mut server = McpServer::start();
    server.send(json!({
        "jsonrpc": "2.0", "method": "notifications/custom", "params": {}
    }));
    server.initialize();
}

#[test]
fn exposes_one_self_explanatory_evaluate_tool() {
    let mut server = McpServer::start();
    let initialize = server.initialize();
    let instructions = initialize["result"]["instructions"].as_str().unwrap();
    for guidance in ["semantic", "boolean", "select", "scale", "Do not"] {
        assert!(
            instructions.contains(guidance),
            "missing {guidance}: {instructions}"
        );
    }

    server.send(json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}
    }));
    let response = server.read_response();
    let tools = response["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    let tool = &tools[0];
    assert_eq!(tool["name"], "evaluate");
    let description = tool["description"].as_str().unwrap();
    for guidance in ["semantic", "boolean", "select", "scale", "Do not"] {
        assert!(
            description.contains(guidance),
            "missing {guidance}: {description}"
        );
    }
    let schema = tool["inputSchema"].to_string();
    for field in [
        "context",
        "questions",
        "detail",
        "boolean",
        "select",
        "scale",
    ] {
        assert!(schema.contains(field), "schema missing {field}: {schema}");
    }
    for hidden in ["jev-latest", "noul", "choice", "score", "model"] {
        assert!(!schema.contains(hidden), "schema leaked {hidden}: {schema}");
    }
}

#[test]
fn rejects_invalid_request_without_calling_service() {
    let mut server = McpServer::start();
    server.initialize();
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "evaluate",
            "arguments": {"context": "anything", "questions": {}}
        }
    }));
    let response = server.read_response();
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("questions must not be empty")
    );
}

#[test]
#[ignore = "requires TYPESAFE_API_KEY and performs a live evaluation"]
fn live_mcp_matches_compact_contract() {
    assert!(std::env::var_os("TYPESAFE_API_KEY").is_some());
    let mut server = McpServer::start_live();
    server.initialize();
    let fixture: Value = serde_json::from_str(include_str!("fixtures/all-kinds.json")).unwrap();
    server.send(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {"name": "evaluate", "arguments": fixture}
    }));
    let response = server.read_response();
    let result = &response["result"];
    assert_eq!(result["isError"], false);
    let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text, result["structuredContent"]);
    assert_eq!(text["answers"].as_object().unwrap().len(), 3);
}
