use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use mcpcall_core::output;

mod cli;

use cli::{Cli, Command, PromptCommand, ResourceCommand};

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
            let options = args.transport.to_options()?;
            let tools = mcpcall_rmcp::list_tools(&options).await?;
            output::print_tools(&tools, args.json, args.schema, args.brief)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Call(args) => {
            let options = args.transport.to_options()?;
            let parsed = mcpcall_core::parse_call_arguments(
                &args.target,
                args.args_json.as_deref(),
                &args.arg,
                &args.pairs,
            )?;
            let result = mcpcall_rmcp::call_tool(&options, parsed.name, parsed.arguments).await?;
            output::print_call_result(&result, args.json)?;
            if result.is_error && !args.allow_tool_error {
                Ok(ExitCode::from(2))
            } else {
                Ok(ExitCode::SUCCESS)
            }
        }
        Command::Resources(args) => {
            let options = args.transport.to_options()?;
            match args.command {
                ResourceCommand::List(list_args) => {
                    let resources = mcpcall_rmcp::list_resources(&options).await?;
                    output::print_resources(&resources, list_args.json, list_args.brief)?;
                }
                ResourceCommand::Templates(list_args) => {
                    let templates = mcpcall_rmcp::list_resource_templates(&options).await?;
                    output::print_resource_templates(&templates, list_args.json, list_args.brief)?;
                }
                ResourceCommand::Read(read_args) => {
                    let result = mcpcall_rmcp::read_resource(&options, read_args.uri).await?;
                    output::print_read_resource(&result, read_args.json)?;
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Prompts(args) => {
            let options = args.transport.to_options()?;
            match args.command {
                PromptCommand::List(list_args) => {
                    let prompts = mcpcall_rmcp::list_prompts(&options).await?;
                    output::print_prompts(&prompts, list_args.json, list_args.brief)?;
                }
                PromptCommand::Get(get_args) => {
                    let parsed = mcpcall_core::parse_named_arguments(
                        &get_args.name,
                        get_args.args_json.as_deref(),
                        &get_args.arg,
                        &get_args.pairs,
                    )?;
                    let result =
                        mcpcall_rmcp::get_prompt(&options, parsed.name, parsed.arguments).await?;
                    output::print_prompt_result(&result, get_args.json)?;
                }
            }
            Ok(ExitCode::SUCCESS)
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
