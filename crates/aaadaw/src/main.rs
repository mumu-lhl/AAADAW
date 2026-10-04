mod app;
mod mcp;
mod timeline;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|argument| argument == "mcp") {
        let writable = args.iter().any(|argument| argument == "--write");
        let expected_len = if writable { 5 } else { 4 };
        if args.get(1).is_none_or(|argument| argument != "--stdio")
            || args.get(2).is_none_or(|argument| argument != "--project")
            || args.len() != expected_len
            || (writable && args.get(4).is_none_or(|argument| argument != "--write"))
        {
            return Err("usage: aaadaw mcp --stdio --project <path.aaadaw> [--write]".into());
        }
        return mcp::run(&args[3], writable);
    }
    app::run()?;
    Ok(())
}
