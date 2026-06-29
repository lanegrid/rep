//! rep - safe, machine-readable rename & token migration for git repositories

use std::process::ExitCode;

use clap::Parser;
use clap::error::ErrorKind;

use rep::cli::{Cli, Commands};
use rep::error::{RepError, Result};
use rep::planner::PlanOpts;
use rep::residual::ResidualOpts;
use rep::scope::ScopeOpts;
use rep::text;
use rep::{applier, output, planner, residual, scanner, status};

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => return handle_parse_error(e),
    };
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
                if let Some(hint) = e.hint() {
                    output::action(hint);
                }
            }
            ExitCode::from(e.exit_code() as u8)
        }
    }
}

/// Turn a clap argument-parsing failure into help, not a dead end.
///
/// `--help`/`--version` are explicit, successful requests — clap renders them as
/// usual. A genuine usage error (unknown command, missing/invalid argument) is
/// the moment a user reads help most, so we follow clap's own message — which
/// already carries its "did you mean" tips — with a runnable next command for
/// whatever subcommand they were reaching for. Under `--json` we instead emit
/// the same machine-readable envelope every other error uses, so agents never
/// hit a non-JSON failure on the "primary interface".
///
/// Usage errors exit with code 10 (`invalid_arguments`), not clap's default 2,
/// because rep publishes 2 as "no matches" — agents must be able to tell a clean
/// empty scan from a mistyped command.
fn handle_parse_error(e: clap::Error) -> ExitCode {
    if matches!(
        e.kind(),
        ErrorKind::DisplayHelp
            | ErrorKind::DisplayVersion
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    ) {
        e.exit();
    }

    if std::env::args().skip(1).any(|a| a == "--json") {
        let rendered = e.to_string();
        let reason = rendered.trim();
        let reason = reason.strip_prefix("error: ").unwrap_or(reason);
        let err = RepError::InvalidArguments(reason.to_string());
        let _ = output::print_json(&err.to_output());
        return ExitCode::from(err.exit_code() as u8);
    }

    let _ = e.print();
    output::action(&format!("Try:  {}", suggested_command()));
    ExitCode::from(RepError::InvalidArguments(String::new()).exit_code() as u8)
}

/// A runnable example for the subcommand the user was reaching for, so the
/// `Try:` line after a usage error is a copy-pasteable next step, not generic
/// boilerplate. Falls back to the first step of the workflow.
fn suggested_command() -> &'static str {
    let sub = std::env::args().skip(1).find(|a| !a.starts_with('-'));
    match sub.as_deref() {
        Some("scan") => "rep scan old_name",
        Some("plan") => "rep plan --map old_name=new_name",
        Some("apply") => "rep apply --plan <plan-id>",
        Some("residual") => "rep residual old_name",
        Some("status") => "rep status",
        _ => "rep scan old_name        (list every command with 'rep --help')",
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
