use std::io;
use std::process::ExitCode;

use einstein_adapters::{Invocation, cli, options};

fn main() -> ExitCode {
    match options(std::env::args_os().skip(1)) {
        Ok(Invocation::Help) => {
            print!("{}", cli::HELP);
            ExitCode::SUCCESS
        }
        Ok(Invocation::Run(opts)) => {
            match cli::run(opts, io::stdin().lock(), io::stdout().lock()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(err) if err.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
                Err(err) => {
                    eprintln!("einstein-cli: {err}");
                    ExitCode::FAILURE
                }
            }
        }
        Err(err) => {
            eprintln!("einstein-cli: {err}");
            ExitCode::from(2)
        }
    }
}
