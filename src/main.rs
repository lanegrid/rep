//! rep - safe, machine-readable rename & token migration for git repositories

use std::process::ExitCode;

use clap::Parser;
use clap::error::ErrorKind;

use rep::cli::{Cli, Commands};
use rep::error::{RepError, Result};
use rep::planner::PlanOpts;
use rep::residual::ResidualOpts;
use rep::scope::ScopeOpts;
use rep::show::ShowOpts;
use rep::text;
use rep::{applier, output, planner, residual, scanner, show, status};

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
        Some("apply") => "rep apply --last",
        Some("residual") => "rep residual old_name",
        Some("show") => "rep show",
        Some("status") => "rep status",
        _ => "rep scan old_name        (list every command with 'rep --help')",
    }
}

/// Read every `--map-file` argument into mappings, in flag order.
///
/// `-` (stdin) is allowed at most once so it is unambiguous which mappings
/// came from the pipe. Read failures are usage errors (exit 10), not generic
/// IO errors, because a missing map file is a mistyped argument.
fn read_map_files(paths: &[String]) -> Result<Vec<text::Mapping>> {
    if paths.iter().filter(|p| p.as_str() == "-").count() > 1 {
        return Err(RepError::InvalidArguments(
            "--map-file '-' (stdin) may be given at most once".to_string(),
        ));
    }
    let mut maps = Vec::new();
    for path in paths {
        let (source, content) = if path == "-" {
            let content = std::io::read_to_string(std::io::stdin()).map_err(|e| {
                RepError::InvalidArguments(format!("cannot read --map-file '-' (stdin): {e}"))
            })?;
            ("stdin".to_string(), content)
        } else {
            let content = std::fs::read_to_string(path).map_err(|e| {
                RepError::InvalidArguments(format!("cannot read --map-file '{path}': {e}"))
            })?;
            (path.clone(), content)
        };
        maps.extend(text::parse_map_file(&source, &content)?);
    }
    Ok(maps)
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
            map_file,
            no_content,
            rename_paths,
            include,
            exclude,
            tracked_only: _,
        } => {
            let mut maps = map
                .iter()
                .map(|s| text::parse_mapping(s))
                .collect::<Result<Vec<_>>>()?;
            maps.extend(read_map_files(&map_file)?);
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

        // The clap ArgGroup guarantees exactly one of --plan/--last, so
        // `plan: None` here always means `--last`.
        Commands::Apply { plan, last: _ } => applier::run(plan, json),

        Commands::Residual {
            token,
            plan,
            last,
            case_insensitive,
            include,
            exclude,
            tracked_only: _,
        } => residual::run(
            ResidualOpts {
                token,
                plan,
                last,
                case_insensitive,
                scope: ScopeOpts {
                    include,
                    exclude,
                    tracked_only: true,
                },
            },
            json,
        ),

        // `last` only exists for CLI symmetry; `plan: None` already means the
        // most recent plan.
        Commands::Show {
            plan,
            last: _,
            files,
            skipped,
            preview,
        } => show::run(
            ShowOpts {
                plan,
                files,
                skipped,
                preview,
            },
            json,
        ),

        Commands::Status => status::run(json),
    }
}
