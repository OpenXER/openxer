use openxer_engine::fixture;
use std::{env, path::PathBuf, process::ExitCode};
fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("oxdiff: {e}");
            ExitCode::FAILURE
        }
    }
}
fn run() -> Result<bool, String> {
    let mut args = env::args_os().skip(1);
    let cmd = args
        .next()
        .ok_or("Usage: oxdiff cal|engine|forward|roundtrip [path]")?;
    if cmd == "--help" || cmd == "-h" {
        println!("oxdiff cal [path]\noxdiff engine|forward|roundtrip [directory]");
        return Ok(true);
    }
    let mode = cmd.to_str().ok_or("Invalid command")?;
    // Fixtures live outside the repository (OPENXER_FIXTURES). A relative
    // path that does not exist as given is read inside the fixtures folder,
    // so `oxdiff engine regressions` works from anywhere.
    let root = fixture::fixtures_root();
    let path = match args.next().map(PathBuf::from) {
        Some(p) if p.is_relative() && !p.exists() => root.join(p),
        Some(p) => p,
        None => root.join(if mode == "cal" { "cal.txt" } else { "models" }),
    };
    if args.next().is_some() {
        return Err("Too many arguments".into());
    }
    if !path.exists() {
        return Err(format!(
            "{} not found (set OPENXER_FIXTURES to the fixtures folder)",
            path.display()
        ));
    }
    let r = match mode {
        "cal" => fixture::run_file(path)?,
        "engine" | "forward" | "roundtrip" => fixture::run_models(path, mode)?,
        _ => return Err(format!("Unknown command {mode}")),
    };
    println!("{}/{} exact", r.exact, r.total);
    for m in r.mismatches {
        eprintln!("{m}");
    }
    Ok(r.total > 0 && r.exact == r.total)
}
