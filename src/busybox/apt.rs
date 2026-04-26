//! APT/APT-GET package manager compatibility shim for Debian/Ubuntu.
//!
//! Acts as a simplified CLI parser like main.rs, calling epkg backend functions.

use clap::{Arg, ArgAction, Command};
use color_eyre::Result;

// ---------------------------------------------------------------------------
// Common CLI option helpers shared by package-manager applets (apt, apk, dnf)
// ---------------------------------------------------------------------------

/// Parameters shared across package-manager CLI shims (apt, apk, dnf).
///
/// Extend this struct when adding new shared flags (e.g. `--quiet`, `--dry-run`).
pub struct PmParams {
    /// `-y` / `--assume-yes`: answer yes to all prompts.
    pub assume_yes: bool,
}

/// Add common CLI arguments shared by package-manager applets.
///
/// Currently adds:
/// - `-y` / `--assume-yes`: answer yes to all prompts
pub fn add_common_args(cmd: Command) -> Command {
    cmd.arg(
        Arg::new("assume-yes")
            .short('y')
            .long("assume-yes")
            .help("Answer yes to all prompts")
            .global(true)
            .action(ArgAction::SetTrue),
    )
}

/// Parse common CLI parameters from the top-level arg matches.
pub fn parse_common_options(matches: &clap::ArgMatches) -> PmParams {
    let assume_yes = matches.contains_id("assume-yes") && matches.get_flag("assume-yes");
    PmParams { assume_yes }
}

/// Apply common CLI parameters to the global epkg config.
///
/// Call this from an applet's `run()` after `try_light_init()`.
pub fn apply_common_options(params: &PmParams) {
    if params.assume_yes {
        crate::models::set_assume_yes(true);
    }
}

/// Parameters extracted from apt CLI. Reuses epkg backend.
pub struct AptParams {
    pub subcmd: String,
    pub packages: Vec<String>,
    pub assume_yes: bool,
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

    Ok(AptParams { subcmd: subcmd.to_string(), packages, assume_yes: common.assume_yes })
}

pub fn command() -> Command {
    add_common_args(
        Command::new("apt")
            .about("Debian/Ubuntu package manager compatibility shim (epkg)")
            .visible_alias("apt-get")
            .subcommand_required(true)
            .arg_required_else_help(true))
        .subcommand(
            Command::new("install")
                .about("Install package(s)")
                .arg(Arg::new("packages")
                    .value_name("PACKAGES")
                    .help("Package names to install")
                    .num_args(1..)
                    .required(true)),
        )
        .subcommand(
            Command::new("remove")
                .about("Remove package(s)")
                .arg(Arg::new("packages")
                    .value_name("PACKAGES")
                    .help("Package names to remove")
                    .num_args(1..)
                    .required(true)),
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

    apply_common_options(&PmParams { assume_yes: params.assume_yes });

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