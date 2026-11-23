use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};

const CLI: &str = env!("CARGO_BIN_EXE_einstein-cli");
const MCP: &str = env!("CARGO_BIN_EXE_einstein-mcp");

fn process(executable: &str, input: &[u8], dir: Option<&Path>) -> Vec<Value> {
    let mut command = Command::new(executable);
    command.arg("--no-clock");
    if let Some(dir) = dir {
        command.arg("--save-dir").arg(dir);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stderr.is_empty());
    String::from_utf8(result.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn rpc(method: &str, id: usize, params: Value) -> Value {
    json!({"jsonrpc":"2.0","method":method,"id":id,"params":params})
}

fn initialize(version: &str, id: usize) -> Value {
    rpc(
        "initialize",
        id,
        json!({"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"tests","version":"1"}}),
    )
}

fn notice() -> Value {
    json!({"jsonrpc":"2.0","method":"notifications/initialized"})
}
fn call(name: &str, id: usize, arguments: Value) -> Value {
    rpc(
        "tools/call",
        id,
        json!({"name":format!("einstein_{name}"),"arguments":arguments}),
    )
}
fn payload(response: &Value) -> Value {
    serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
}
fn input(msgs: &[Value]) -> String {
    msgs.iter().map(|msg| format!("{msg}\n")).collect()
}

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("adapters-{}-{n}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn cli_and_mcp_change_the_same_state_with_the_same_sequence() {
    let tmp = Temp::new();
    let cli_dir = tmp.0.join("cli");
    let mcp_dir = tmp.0.join("mcp");
    let script = b"state\nnew 42\nselect 9 1 1\ntoggle-hint 0\nswitch-hints\nsave 3\npause\nexclude 2 1 1\nresume\nload 3\nexclude 1 1 5\nrestart\nselect 1 1 5\nend\n";
    let cli = process(CLI, script, Some(&cli_dir));
    let commands = [
        ("state", json!({})),
        ("new", json!({"seed":"42"})),
        ("select", json!({"category":9,"column":1,"value":1})),
        ("toggle_hint", json!({"id":0})),
        ("switch_hints", json!({})),
        ("save", json!({"slot":3})),
        ("pause", json!({})),
        ("exclude", json!({"category":2,"column":1,"value":1})),
        ("resume", json!({})),
        ("load", json!({"slot":3})),
        ("exclude", json!({"category":1,"column":1,"value":5})),
        ("restart", json!({})),
        ("select", json!({"category":1,"column":1,"value":5})),
        ("end", json!({})),
    ];
    let mut msgs = vec![initialize("2025-11-25", 0), notice()];
    msgs.extend(
        commands
            .into_iter()
            .enumerate()
            .map(|(i, (name, args))| call(name, i + 1, args)),
    );
    let mcp = process(MCP, input(&msgs).as_bytes(), Some(&mcp_dir));
    assert_eq!(mcp.len(), cli.len() + 1);
    assert_eq!(mcp[0]["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(cli, mcp[1..].iter().map(payload).collect::<Vec<_>>());
    assert_eq!(cli[2]["ok"], false);
    assert_eq!(cli[5]["effect"], "saved");
    assert_eq!(cli[7]["ok"], false);
    assert_eq!(cli[9]["state"]["hint_view"], "dismissed");
    assert_eq!(cli[10]["state"]["status"], "failed");
    assert_eq!(cli[12]["state"]["candidates"][0][0], json!([5]));
    assert_eq!(cli[13]["state"], Value::Null);
    assert_eq!(
        fs::read(cli_dir.join("slot-3.epz")).unwrap(),
        fs::read(mcp_dir.join("slot-3.epz")).unwrap()
    );
    for row in cli {
        if let Some(state) = row["state"].as_object() {
            for field in ["seed", "solution", "canonical", "save_image"] {
                assert!(!state.contains_key(field));
            }
        }
    }
}

#[test]
fn lifecycle_negotiates_supported_version_and_only_replies_to_requests() {
    let msgs = [
        rpc("tools/list", 1, json!({})),
        initialize("2025-06-18", 2),
        rpc("tools/list", 3, json!({})),
        notice(),
        rpc("tools/list", 4, json!({})),
        rpc("ping", 5, json!({})),
        rpc("tools/nope", 6, json!({})),
        initialize("unknown", 7),
    ];
    let out = process(MCP, input(&msgs).as_bytes(), None);
    assert_eq!(out.len(), 7);
    assert_eq!(out[0]["error"]["code"], -32002);
    assert_eq!(out[1]["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(out[2]["error"]["code"], -32002);
    let tools = out[3]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 12);
    assert!(
        tools
            .iter()
            .all(|tool| tool["inputSchema"]["additionalProperties"] == false)
    );
    let save = tools
        .iter()
        .find(|tool| tool["name"] == "einstein_save")
        .unwrap();
    assert_eq!(save["inputSchema"]["required"], json!(["slot"]));
    assert_eq!(out[4]["result"], json!({}));
    assert_eq!(out[5]["error"]["code"], -32601);
    assert_eq!(out[6]["error"]["code"], -32600);
    let fallback = process(MCP, input(&[initialize("future", 1)]).as_bytes(), None);
    assert_eq!(fallback[0]["result"]["protocolVersion"], "2025-11-25");
}

#[test]
fn oversize_invalid_json_and_slot_paths_are_rejected_without_losing_next_frame() {
    let mut bytes = vec![b'x'; einstein_adapters::MAX_LINE * 2];
    bytes.extend_from_slice(b"\n{!\n");
    let msgs = [
        initialize("2025-11-25", 0),
        notice(),
        call("save", 1, json!({"slot":"../game"})),
        call("load", 2, json!({"slot":1,"path":"escape.epz"})),
        call("load", 3, json!({"slot":1})),
        rpc("tools/call", 4, json!({"name":"einstein_does_not_exist"})),
        call("state", 5, json!({})),
    ];
    bytes.extend_from_slice(input(&msgs).as_bytes());
    let result = process(MCP, &bytes, None);
    assert_eq!(result[0]["error"]["code"], -32700);
    assert_eq!(result[1]["error"]["code"], -32700);
    assert_eq!(result[2]["id"], 0);
    assert!((3..=5).all(|i| result[i]["result"]["isError"] == true));
    assert_eq!(result[6]["error"]["code"], -32602);
    assert_eq!(
        payload(&result[7]),
        json!({"ok":true,"effect":"unchanged","state":null})
    );
}
