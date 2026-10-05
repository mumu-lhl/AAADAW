mod app;
mod clap_scanner;
mod logging;
mod mcp;
mod timeline;

fn main() -> std::process::ExitCode {
    let helper_mode = std::env::args_os()
        .nth(1)
        .is_some_and(|argument| argument == clap_scanner::SCAN_COMMAND);
    let _log_guard = (!helper_mode).then(logging::initialize);
    match run() {
        Ok(()) => {
            tracing::info!("AAADAW stopped");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            tracing::error!(error = %error, "AAADAW exited with an error");
            eprintln!("AAADAW failed: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args
        .first()
        .is_some_and(|argument| argument == clap_scanner::SCAN_COMMAND)
    {
        if args.len() != 3 {
            return Err("invalid internal CLAP scanner invocation".into());
        }
        return clap_scanner::run_helper(&args[1], &args[2]);
    }
    let args = args
        .iter()
        .map(|argument| {
            argument
                .to_str()
                .ok_or("command-line arguments must be valid Unicode")
        })
        .collect::<Result<Vec<_>, _>>()?;
    if args.first().is_some_and(|argument| *argument == "mcp") {
        let writable = args.contains(&"--write");
        let expected_len = if writable { 5 } else { 4 };
        if args.get(1).is_none_or(|argument| *argument != "--stdio")
            || args.get(2).is_none_or(|argument| *argument != "--project")
            || args.len() != expected_len
            || (writable && args.get(4).is_none_or(|argument| *argument != "--write"))
        {
            return Err("usage: aaadaw mcp --stdio --project <path.aaadaw> [--write]".into());
        }
        tracing::info!(mode = "mcp", writable, "starting AAADAW");
        return mcp::run(args[3], writable);
    }
    tracing::info!(mode = "desktop", "starting AAADAW");
    app::run()?;
    Ok(())
}
