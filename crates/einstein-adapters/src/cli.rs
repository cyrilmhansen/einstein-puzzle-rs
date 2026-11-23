use std::io::{self, BufRead, Write};

use serde_json::{Map, Value, json};

use crate::service::{Action, Request, Service};
use crate::{Line, Options, line, write_json};

pub const HELP: &str = "Usage: einstein-cli [--save-dir DIRECTORY] [--no-clock]\n\
Reads one local command per line and writes one JSON result. EOF exits.\n\
Commands: new SEED | state | select CATEGORY COLUMN VALUE | exclude CATEGORY COLUMN VALUE\n\
          toggle-hint ID | switch-hints | pause | resume | restart | end | save SLOT | load SLOT\n\
Categories, columns and values are 1..6; slots are 0..9.\n";

pub fn run(options: Options, mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    let mut service = Service::new(options);
    let mut bytes = Vec::new();
    loop {
        let result = match line(&mut input, &mut bytes)? {
            Line::Eof => return Ok(()),
            Line::Oversize => json!({"ok":false,"error":"input line too large"}),
            Line::Data => match std::str::from_utf8(&bytes)
                .map_err(|_| "input must be UTF-8")
                .and_then(parse)
            {
                Ok(request) => service.run(request),
                Err(error) => json!({"ok":false,"error":error}),
            },
        };
        write_json(&mut output, &result)?;
    }
}

fn parse(s: &str) -> Result<Request, &'static str> {
    let mut tokens = s.split_whitespace();
    let name = tokens.next().ok_or("missing command")?.replace('-', "_");
    let action = Action::ALL
        .into_iter()
        .find(|action| action.name() == name)
        .ok_or("unknown command")?;
    let mut args = Map::new();
    for &key in action.fields() {
        let token = tokens.next().ok_or("unexpected or missing arguments")?;
        let value = if key == "seed" {
            Value::from(token)
        } else {
            if !token.bytes().all(|b| b.is_ascii_digit()) {
                return Err("expected unsigned integer argument");
            }
            Value::from(token.parse::<u64>().map_err(|_| "integer out of range")?)
        };
        args.insert(key.into(), value);
    }
    if tokens.next().is_some() {
        return Err("unexpected or missing arguments");
    }
    action.parse(&args)
}
