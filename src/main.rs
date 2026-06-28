//! rep - safe, machine-readable rename & token migration for git repositories

use std::process::ExitCode;

use clap::Parser;

use rep::cli::{Cli, Commands};
use rep::error::Result;
use rep::planner::PlanOpts;
use rep::residual::ResidualOpts;
use rep::scope::ScopeOpts;
use rep::text;
use rep::{applier, output, planner, residual, scanner, status};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let json = cli.json;

    let result = dispatch(cli);

    match result {
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            if json {
                // Machine-readable error for agents; exit code controls flow,
                // the JSON envelope explains the failure.
                let _ = output::print_json(&e.to_output());
            } else {
                output::error(&e.to_string());
            }
            ExitCode::from(e.exit_code() as u8)
        }
    }
}

fn dispatch(cli: Cli) -> Result<i32> {
    let json = cli.json;
    match cli.command {
        Commands::Scan {
            token,
            case_insensitive,
            include,
            exclude,
            tracked_only: _,
        } => scanner::run(
            token,
            case_insensitive,
            ScopeOpts {
                include,
                exclude,
                tracked_only: true,
            },
            json,
        ),

        Commands::Plan {
            map,
            no_content,
            rename_paths,
            include,
            exclude,
            tracked_only: _,
        } => {
            let maps = map
                .iter()
                .map(|s| text::parse_mapping(s))
                .collect::<Result<Vec<_>>>()?;
            planner::run(
                PlanOpts {
                    maps,
                    content: !no_content,
                    rename_paths,
                    scope: ScopeOpts {
                        include,
                        exclude,
                        tracked_only: true,
                    },
                },
                json,
            )
        }

        Commands::Apply { plan } => applier::run(plan, json),

        Commands::Residual {
            token,
            plan,
            case_insensitive,
            include,
            exclude,
            tracked_only: _,
        } => residual::run(
            ResidualOpts {
                token,
                plan,
                case_insensitive,
                scope: ScopeOpts {
                    include,
                    exclude,
                    tracked_only: true,
                },
            },
            json,
        ),

        Commands::Status => status::run(json),
    }
}
