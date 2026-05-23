use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

mod cli;
mod mcp;
mod output;
mod values;

use cli::{Cli, Command};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    init_logging(cli.verbose);

    match cli.command {
        Command::List(args) => {
            let tools = mcp::list_tools(&args.transport).await?;
            output::print_tools(&tools, args.json, args.schema, args.brief)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Call(args) => {
            let parsed = values::parse_call_arguments(
                &args.target,
                args.args_json.as_deref(),
                &args.arg,
                &args.pairs,
            )?;
            let result =
                mcp::call_tool(&args.transport, parsed.tool_name, parsed.arguments).await?;
            output::print_call_result(&result, args.json)?;
            if result.is_error.unwrap_or(false) && !args.allow_tool_error {
                Ok(ExitCode::from(2))
            } else {
                Ok(ExitCode::SUCCESS)
            }
        }
    }
}

fn init_logging(verbose: u8) {
    if verbose == 0 {
        return;
    }
    let directive = if verbose == 1 { "info" } else { "debug" };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| directive.into()),
        )
        .with_writer(std::io::stderr)
        .try_init();
}
