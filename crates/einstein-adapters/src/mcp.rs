//! MCP stdio, tools capability only. No HTTP listener or background work.

use std::io::{self, BufRead, Write};

use serde_json::{Map, Value, json};

use crate::service::{Action, Service};
use crate::{Line, Options, line, write_json};

const VERSIONS: [&str; 2] = ["2025-11-25", "2025-06-18"];
pub const HELP: &str = "Usage: einstein-mcp [--save-dir DIRECTORY] [--no-clock]\nMCP JSON-RPC server over stdio (no network). EOF exits.\n";

#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    AwaitInitialize,
    AwaitNotification,
    Ready,
}

pub fn run(options: Options, mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    let mut mcp = Mcp {
        phase: Phase::AwaitInitialize,
        service: Service::new(options),
    };
    let mut bytes = Vec::new();
    loop {
        let response = match line(&mut input, &mut bytes)? {
            Line::Eof => return Ok(()),
            Line::Oversize => Some(error(Value::Null, -32700, "input line too large")),
            Line::Data => match serde_json::from_slice(&bytes) {
                Ok(value) => mcp.receive(value),
                Err(_) => Some(error(Value::Null, -32700, "invalid JSON")),
            },
        };
        if let Some(response) = response {
            write_json(&mut output, &response)?;
        }
    }
}

struct Mcp {
    phase: Phase,
    service: Service,
}

impl Mcp {
    fn receive(&mut self, envelope: Value) -> Option<Value> {
        let method = envelope.get("method").and_then(Value::as_str);
        if envelope.get("jsonrpc").and_then(Value::as_str) != Some("2.0") || method.is_none() {
            return Some(error(Value::Null, -32600, "invalid JSON-RPC request"));
        }
        let method = method.unwrap();
        let Some(id) = envelope.get("id").cloned() else {
            if method == "notifications/initialized"
                && self.phase == Phase::AwaitNotification
                && envelope.get("params").is_none_or(Value::is_object)
            {
                self.phase = Phase::Ready;
            }
            // Notifications never receive replies.
            return None;
        };
        if !(id.is_string() || id.is_u64() || id.is_i64()) {
            return Some(error(Value::Null, -32600, "invalid request id"));
        }
        let params = match envelope.get("params") {
            None => None,
            Some(v) => match v.as_object() {
                Some(obj) => Some(obj),
                None => return Some(error(id, -32602, "params must be an object")),
            },
        };
        let result = match method {
            "initialize" => match self.initialize(params) {
                Ok(value) => value,
                Err((code, reason)) => return Some(error(id, code, reason)),
            },
            "ping" => json!({}),
            _ if self.phase != Phase::Ready => {
                return Some(error(id, -32002, "server not initialized"));
            }
            "tools/list" => {
                if params.is_some_and(|p| p.contains_key("cursor")) {
                    return Some(error(id, -32602, "invalid cursor"));
                }
                json!({"tools":Action::ALL.map(Action::tool)})
            }
            "tools/call" => match self.call(params) {
                Ok(value) => value,
                Err(reason) => return Some(error(id, -32602, reason)),
            },
            _ => return Some(error(id, -32601, "method not found")),
        };
        Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
    }

    fn initialize(
        &mut self,
        params: Option<&Map<String, Value>>,
    ) -> Result<Value, (i32, &'static str)> {
        if self.phase != Phase::AwaitInitialize {
            return Err((-32600, "already initialized"));
        }
        let invalid = (
            -32602,
            "initialize requires protocolVersion, capabilities and clientInfo name/version",
        );
        let params = params.ok_or(invalid)?;
        let requested = params
            .get("protocolVersion")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or(invalid)?;
        let client = params
            .get("clientInfo")
            .and_then(Value::as_object)
            .ok_or(invalid)?;
        if !params.get("capabilities").is_some_and(Value::is_object)
            || !["name", "version"]
                .into_iter()
                .all(|k| client.get(k).and_then(Value::as_str).is_some())
        {
            return Err(invalid);
        }
        let version = VERSIONS
            .into_iter()
            .find(|&v| v == requested)
            .unwrap_or(VERSIONS[0]);
        self.phase = Phase::AwaitNotification;
        Ok(
            json!({"protocolVersion":version,"capabilities":{"tools":{"listChanged":false}},
            "serverInfo":{"name":"einstein-puzzle","version":env!("CARGO_PKG_VERSION")}}),
        )
    }

    fn call(&mut self, params: Option<&Map<String, Value>>) -> Result<Value, &'static str> {
        let params = params.ok_or("tools/call requires params")?;
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .and_then(|s| s.strip_prefix("einstein_"))
            .ok_or("unknown tool")?;
        let action = Action::ALL
            .into_iter()
            .find(|a| a.name() == name)
            .ok_or("unknown tool")?;
        let empty = Map::new();
        let args = match params.get("arguments") {
            Some(v) => v.as_object().ok_or("arguments must be an object")?,
            None => &empty,
        };
        let result = match action.parse(args) {
            Ok(req) => self.service.run(req),
            Err(reason) => json!({"ok":false,"error":reason}),
        };
        Ok(
            json!({"content":[{"type":"text","text":result.to_string()}],"isError":result["ok"] == false}),
        )
    }
}

fn error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}
