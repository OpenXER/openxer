use openxer_automation::{
    cli::{self, Invocation},
    connect, dispatch, response, AutomationError,
};
use std::{
    io::{self, Write},
    process::ExitCode,
};

fn emit(value: serde_json::Value, exit: u8) -> ExitCode {
    let mut stdout = io::stdout().lock();
    let written = serde_json::to_writer(&mut stdout, &value)
        .map_err(io::Error::other)
        .and_then(|()| writeln!(stdout));
    if let Err(error) = written {
        eprintln!("Cannot write command response: {error}");
        return ExitCode::from(1);
    }
    ExitCode::from(exit)
}

/// The documented exit codes: 0 success, 2 invalid command-line or request
/// arguments (wherever they are found), 1 any other failure.
fn exit_code<T>(result: &openxer_automation::Result<T>) -> u8 {
    match result {
        Ok(_) => 0,
        Err(e) if matches!(e.code.as_str(), "invalid_arguments" | "invalid_changes") => 2,
        Err(_) => 1,
    }
}

fn main() -> ExitCode {
    let arguments = std::env::args_os()
        .skip(1)
        .map(|value| {
            value.into_string().map_err(|_| {
                AutomationError::new("invalid_arguments", "Arguments must be valid Unicode")
            })
        })
        .collect::<openxer_automation::Result<Vec<_>>>();
    match arguments.and_then(cli::parse) {
        Ok(Invocation::Help) => {
            // A reader that closes the pipe early is not an error.
            let _ = io::stdout().lock().write_all(cli::HELP.as_bytes());
            ExitCode::SUCCESS
        }
        Ok(Invocation::Version) => {
            let _ = writeln!(
                io::stdout().lock(),
                "openxer-cli {}",
                openxer_automation::VERSION
            );
            ExitCode::SUCCESS
        }
        Ok(Invocation::Mcp) => match openxer_automation::mcp::run_stdio() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("MCP transport error: {error}");
                ExitCode::from(1)
            }
        },
        Ok(Invocation::ChatMcp(context)) => {
            match openxer_automation::mcp::run_stdio_with(openxer_automation::mcp::Server::chat(
                context,
            )) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("MCP transport error: {error}");
                    ExitCode::from(1)
                }
            }
        }
        Ok(Invocation::Connect(request)) => {
            let exe = std::env::current_exe().ok();
            let env = connect::Env::from_process(exe);
            let result = connect::run(&request, &env);
            let exit = exit_code(&result);
            emit(response(result), exit)
        }
        Ok(Invocation::Run(command)) => {
            let result = dispatch(*command);
            let exit = exit_code(&result);
            emit(response(result), exit)
        }
        Err(error) => emit(response(Err(error)), 2),
    }
}
