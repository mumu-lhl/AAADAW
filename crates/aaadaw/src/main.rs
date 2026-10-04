mod app;
mod mcp;
mod timeline;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|argument| argument == "mcp") {
        if args.get(1).is_none_or(|argument| argument != "--stdio")
            || args.get(2).is_none_or(|argument| argument != "--project")
            || args.len() != 4
        {
            return Err("usage: aaadaw mcp --stdio --project <path.aaadaw>".into());
        }
        return mcp::run(&args[3]);
    }
    app::run()?;
    Ok(())
}
