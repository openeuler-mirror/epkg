//! APK package manager compatibility shim for Alpine Linux.
//!
//! Acts as a simplified CLI parser like main.rs, calling epkg backend functions.

use clap::{Arg, Command};
use color_eyre::Result;

/// Parameters extracted from apk CLI. Reuses epkg backend by calling
/// install_packages/remove_packages/upgrade_packages/search/info/list directly.
pub struct ApkParams {
    pub subcmd: String,
    pub packages: Vec<String>,
}

pub fn parse_options(matches: &clap::ArgMatches) -> Result<ApkParams> {
    let (subcmd, sub_matches) = matches.subcommand().unwrap_or(("help", matches));

    let packages: Vec<String> = sub_matches
        .get_many::<String>("packages")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();

    Ok(ApkParams { subcmd: subcmd.to_string(), packages })
}

pub fn command() -> Command {
    Command::new("apk")
        .about("Alpine package manager compatibility shim (epkg)")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("add")
                .about("Install package(s)")
                .arg(Arg::new("packages")
                    .value_name("PACKAGES")
                    .help("Package names to install")
                    .num_args(1..)
                    .required(true)),
        )
        .subcommand(
            Command::new("del")
                .about("Remove package(s)")
                .arg(Arg::new("packages")
                    .value_name("PACKAGES")
                    .help("Package names to remove")
                    .num_args(1..)
                    .required(true)),
        )
        .subcommand(
            Command::new("info")
                .about("Show package information")
                .arg(Arg::new("packages")
                    .value_name("PACKAGES")
                    .help("Package names to query")
                    .num_args(1..)),
        )
        .subcommand(
            Command::new("list")
                .about("List installed packages"),
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
        .subcommand(
            Command::new("update")
                .about("Update repository index"),
        )
        .subcommand(
            Command::new("upgrade")
                .about("Upgrade installed packages")
                .arg(Arg::new("packages")
                    .value_name("PACKAGES")
                    .help("Package names to upgrade (empty = all)")
                    .num_args(0..)),
        )
}

pub fn run(params: ApkParams) -> Result<()> {
    // Initialize like main.rs does for regular epkg commands
    crate::init::try_light_init()?;

    match params.subcmd.as_str() {
        "add" => {
            crate::install::install_packages(params.packages)?;
        }
        "del" => {
            crate::remove::remove_packages(params.packages)?;
        }
        "update" => {
            crate::repo::sync_channel_metadata()?;
            println!("OK: repository index updated");
        }
        "upgrade" => {
            crate::upgrade::upgrade_packages(params.packages)?;
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
        "info" => {
            crate::info::show_package_info(&params.packages, false, false, false)?;
        }
        "list" => {
            crate::list::list_packages_with_scope(crate::list::ListScope::Installed, "")?;
        }
        _ => {
            eprintln!("apk: unknown command '{}'", params.subcmd);
            std::process::exit(1);
        }
    }
    Ok(())
}