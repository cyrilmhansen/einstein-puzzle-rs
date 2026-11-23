use std::io;
use std::process::ExitCode;

use einstein_adapters::{Invocation, mcp, options};

fn main() -> ExitCode {
    match options(std::env::args_os().skip(1)) {
        Ok(Invocation::Help) => {
            print!("{}", mcp::HELP);
            ExitCode::SUCCESS
        }
        Ok(Invocation::Run(opts)) => {
            match mcp::run(opts, io::stdin().lock(), io::stdout().lock()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(err) if err.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
                Err(err) => {
                    eprintln!("einstein-mcp: {err}");
                    ExitCode::FAILURE
                }
            }
        }
        Err(err) => {
            eprintln!("einstein-mcp: {err}");
            ExitCode::from(2)
        }
    }
}
