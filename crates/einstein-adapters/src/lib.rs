//! Shared native command adapter used by the CLI and MCP stdio binaries.
#![forbid(unsafe_code)]

pub mod cli;
pub mod mcp;
mod service;

use std::ffi::OsString;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use serde_json::Value;

pub const MAX_LINE: usize = 16 * 1024;

#[derive(Default)]
pub struct Options {
    pub save_dir: Option<PathBuf>,
    /// For reproducible command transcripts. Default uses host monotonic time.
    pub no_clock: bool,
}

pub enum Invocation {
    Run(Options),
    Help,
}

pub fn options(args: impl IntoIterator<Item = OsString>) -> Result<Invocation, &'static str> {
    let mut options = Options::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => return Ok(Invocation::Help),
            Some("--save-dir") if options.save_dir.is_none() => {
                let path = args
                    .next()
                    .filter(|s| !s.is_empty())
                    .ok_or("--save-dir requires a path")?;
                options.save_dir = Some(path.into());
            }
            Some("--no-clock") if !options.no_clock => options.no_clock = true,
            _ => return Err("unknown or duplicate option (try --help)"),
        }
    }
    Ok(Invocation::Run(options))
}

pub(crate) enum Line {
    Eof,
    Data,
    Oversize,
}

/// Read one bounded frame, discarding oversize frames through the delimiter.
/// `data` excludes the final newline; EOF can terminate the last frame.
pub(crate) fn line(incoming: &mut impl BufRead, data: &mut Vec<u8>) -> io::Result<Line> {
    data.clear();
    let mut oversize = false;
    loop {
        let buf = incoming.fill_buf()?;
        if buf.is_empty() {
            return Ok(if oversize {
                Line::Oversize
            } else if data.is_empty() {
                Line::Eof
            } else {
                Line::Data
            });
        }
        let newline = buf.iter().position(|&b| b == b'\n');
        let length = newline.unwrap_or(buf.len());
        if length > MAX_LINE.saturating_sub(data.len()) {
            oversize = true;
        }
        if !oversize {
            data.extend_from_slice(&buf[..length]);
        }
        incoming.consume(length + usize::from(newline.is_some()));
        if newline.is_some() {
            return Ok(if oversize { Line::Oversize } else { Line::Data });
        }
    }
}

pub(crate) fn write_json(out: &mut impl Write, msg: &Value) -> io::Result<()> {
    serde_json::to_writer(&mut *out, msg)?;
    out.write_all(b"\n")?;
    out.flush()
}
