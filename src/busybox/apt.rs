//! APT/APT-GET package manager compatibility shim for Debian/Ubuntu.
//!
//! Acts as a simplified CLI parser like main.rs, calling epkg backend functions.

use clap::{Arg, ArgAction, Command};
use color_eyre::Result;

// ---------------------------------------------------------------------------
// Common CLI option helpers shared by package-manager applets (apt, apk, dnf)
// ---------------------------------------------------------------------------

/// Parameters shared across package-manager CLI shims (apt, apk, dnf).
#[derive(Default)]
pub struct PmParams {
    /// `-y` / `--assume-yes`: answer yes to all prompts.
    pub assume_yes: bool,
    /// `-q` / `--quiet`: suppress progress output.
    pub quiet: bool,
    /// `-s` / `--dry-run`: simulate only.
    pub dry_run: bool,
    /// `-d` / `--download-only`: download without installing.
    pub download_only: bool,
    /// `-m` / `--ignore-missing`: continue despite missing packages.
    pub ignore_missing: bool,
}

/// Add common CLI arguments shared by package-manager applets.
///
/// Currently adds:
/// - `-y` / `--assume-yes`: answer yes to all prompts
/// - `-q` / `--quiet`: suppress progress output
pub fn add_common_args(cmd: Command) -> Command {
    cmd.arg(
        Arg::new("assume-yes")
            .short('y')
            .long("assume-yes")
            .help("Answer yes to all prompts")
            .global(true)
            .action(ArgAction::SetTrue),
    )
    .arg(
        Arg::new("quiet")
            .short('q')
            .long("quiet")
            .help("Quiet mode - no progress output")
            .global(true)
            .action(ArgAction::SetTrue),
    )
}

/// Parse common CLI parameters from the top-level arg matches.
pub fn parse_common_options(matches: &clap::ArgMatches) -> PmParams {
    let assume_yes = matches.contains_id("assume-yes") && matches.get_flag("assume-yes");
    let quiet     = matches.contains_id("quiet")       && matches.get_flag("quiet");
    PmParams { assume_yes, quiet, ..Default::default() }
}

/// Apply common CLI parameters to the global epkg config atomically.
///
/// Call this from an applet's `run()` after `try_light_init()`.
pub fn apply_common_options(params: &PmParams) {
    crate::models::apply_config_flags(&crate::models::ConfigFlags {
        assume_yes:     params.assume_yes,
        quiet:          params.quiet,
        dry_run:        params.dry_run,
        download_only:  params.download_only,
        ignore_missing: params.ignore_missing,
    });
}

/// Parameters extracted from apt CLI. Reuses epkg backend.
pub struct AptParams {
    pub subcmd: String,
    pub packages: Vec<String>,
    pub assume_yes: bool,
    pub quiet: bool,
    pub dry_run: bool,
    pub download_only: bool,
    pub ignore_missing: bool,
    #[allow(dead_code)]
    pub fix_broken: bool,
}

pub fn parse_options(matches: &clap::ArgMatches) -> Result<AptParams> {
    let (subcmd, sub_matches) = matches.subcommand().unwrap_or(("help", matches));

    // try_get_many handles subcommands without a "packages" arg (e.g. `update`)
    let packages: Vec<String> = sub_matches
        .try_get_many::<String>("packages")
        .unwrap_or_default()
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();

    let common = parse_common_options(matches);

    Ok(AptParams {
        subcmd:        subcmd.to_string(),
        packages,
        assume_yes:    common.assume_yes,
        quiet:         common.quiet,
        dry_run:       matches.contains_id("dry-run")       && matches.get_flag("dry-run"),
        download_only: matches.contains_id("download-only") && matches.get_flag("download-only"),
        ignore_missing:matches.contains_id("ignore-missing")&& matches.get_flag("ignore-missing"),
        fix_broken:    matches.contains_id("fix-broken")    && matches.get_flag("fix-broken"),
    })
}

// Helper to add args common to install and remove subcommands.
// These are accepted but not wired to backend (epkg may or may not have
// equivalents for apt-specific concepts like recommends/reinstall).
fn add_install_remove_args(cmd: Command) -> Command {
    cmd.arg(
        Arg::new("no-install-recommends")
            .long("no-install-recommends")
            .help("Do not install recommended packages (accepted, epkg ignores recommends)")
            .action(ArgAction::SetTrue),
    )
    .arg(
        Arg::new("reinstall")
            .long("reinstall")
            .help("Reinstall packages (accepted)")
            .action(ArgAction::SetTrue),
    )
}

pub fn command() -> Command {
    add_common_args(
        Command::new("apt")
            .about("Debian/Ubuntu package manager compatibility shim (epkg)")
            .visible_alias("apt-get")
            .subcommand_required(true)
            .arg_required_else_help(true)
            // Global options commonly used in scripts/CI/Docker
            .arg(
                Arg::new("dry-run")
                    .short('s')
                    .long("dry-run")
                    .alias("simulate")
                    .alias("just-print")
                    .help("Simulate - show what would be done")
                    .global(true)
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("download-only")
                    .short('d')
                    .long("download-only")
                    .help("Download only - do not install")
                    .global(true)
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("ignore-missing")
                    .short('m')
                    .long("ignore-missing")
                    .alias("fix-missing")
                    .help("Ignore missing packages")
                    .global(true)
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("fix-broken")
                    .short('f')
                    .long("fix-broken")
                    .help("Fix broken dependencies (accepted)")
                    .global(true)
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("target-release")
                    .short('t')
                    .long("target-release")
                    .alias("default-release")
                    .help("Target release (accepted)")
                    .global(true)
                    .num_args(1),
            ))
        .subcommand(
            add_install_remove_args(
                Command::new("install")
                    .about("Install package(s)")
                    .arg(Arg::new("packages")
                        .value_name("PACKAGES")
                        .help("Package names to install")
                        .num_args(1..)
                        .required(true)),
            )
        )
        .subcommand(
            add_install_remove_args(
                Command::new("remove")
                    .about("Remove package(s)")
                    .arg(Arg::new("packages")
                        .value_name("PACKAGES")
                        .help("Package names to remove")
                        .num_args(1..)
                        .required(true)),
            )
        )
        .subcommand(
            Command::new("purge")
                .about("Remove package(s) including configuration")
                .arg(Arg::new("packages")
                    .value_name("PACKAGES")
                    .help("Package names to purge")
                    .num_args(1..)
                    .required(true)),
        )
        .subcommand(
            Command::new("update")
                .about("Update repository index"),
        )
        .subcommand(
            Command::new("upgrade")
                .about("Upgrade installed packages"),
        )
        .subcommand(
            Command::new("full-upgrade")
                .about("Full upgrade (dist-upgrade equivalent)"),
        )
        .subcommand(
            Command::new("show")
                .about("Show package information")
                .arg(Arg::new("packages")
                    .value_name("PACKAGES")
                    .help("Package names to show")
                    .num_args(1..)),
        )
        .subcommand(
            Command::new("list")
                .about("List packages")
                .arg(Arg::new("packages")
                    .value_name("PATTERNS")
                    .help("Filter patterns (empty = all installed)")
                    .num_args(0..)),
        )
        .subcommand(
            Command::new("search")
                .about("Search for package(s)")
                .arg(Arg::new("packages")
                    .value_name("PATTERNS")
                    .help("Search patterns")
                    .num_args(1..)
                    .required(true)),
        )
}

pub fn run(params: AptParams) -> Result<()> {
    crate::init::try_light_init()?;

    apply_common_options(&PmParams {
        assume_yes:     params.assume_yes,
        quiet:          params.quiet,
        dry_run:        params.dry_run,
        download_only:  params.download_only,
        ignore_missing: params.ignore_missing,
    });
    // fix_broken: accepted but not wired (no epkg backend equivalent)

    match params.subcmd.as_str() {
        "install" => {
            crate::install::install_packages(params.packages)?;
        }
        "remove" | "purge" => {
            // purge is treated same as remove in epkg context
            crate::remove::remove_packages(params.packages)?;
        }
        "update" => {
            crate::repo::sync_channel_metadata()?;
            println!("Get:1 repository metadata updated");
            println!("Fetched metadata in 0s");
        }
        "upgrade" | "full-upgrade" => {
            crate::upgrade::upgrade_packages(vec![])?;
        }
        "search" => {
            for pattern in &params.packages {
                let mut options = crate::search::SearchOptions {
                    origin_pattern: pattern.clone(),
                    ..Default::default()
                };
                crate::search::search_repo_cache(&mut options)?;
            }
        }
        "show" => {
            crate::info::show_package_info(&params.packages, false, false, false)?;
        }
        "list" => {
            crate::list::list_packages_with_scope(crate::list::ListScope::Installed, "")?;
        }
        _ => {
            eprintln!("apt: unknown command '{}'", params.subcmd);
            std::process::exit(1);
        }
    }
    Ok(())
}