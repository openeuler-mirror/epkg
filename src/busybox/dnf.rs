//! DNF/YUM package manager compatibility shim for Fedora/RHEL.
//!
//! Acts as a simplified CLI parser like main.rs, calling epkg backend functions.

use clap::{Arg, Command};
use color_eyre::Result;

/// Parameters extracted from dnf/yum CLI. Reuses epkg backend.
pub struct DnfParams {
    pub subcmd: String,
    pub packages: Vec<String>,
    pub assume_yes: bool,
}

pub fn parse_options(matches: &clap::ArgMatches) -> Result<DnfParams> {
    let (subcmd, sub_matches) = matches.subcommand().unwrap_or(("help", matches));

    // try_get_many handles subcommands without a "packages" arg (e.g. `list`)
    let packages: Vec<String> = sub_matches
        .try_get_many::<String>("packages")
        .unwrap_or_default()
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();

    let common = super::apt::parse_common_options(matches);

    Ok(DnfParams { subcmd: subcmd.to_string(), packages, assume_yes: common.assume_yes })
}

pub fn command() -> Command {
    super::apt::add_common_args(
        Command::new("dnf")
            .about("Fedora/RHEL package manager compatibility shim (epkg)")
            .visible_alias("yum")
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
            Command::new("info")
                .about("Show package information")
                .arg(Arg::new("packages")
                    .value_name("PACKAGES")
                    .help("Package names to query")
                    .num_args(1..)),
        )
        .subcommand(
            Command::new("list")
                .about("List packages"),
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
                .about("Update repository metadata"),
        )
        .subcommand(
            Command::new("upgrade")
                .about("Upgrade packages")
                .arg(Arg::new("packages")
                    .value_name("PACKAGES")
                    .help("Package names to upgrade (empty = all)")
                    .num_args(0..)),
        )
        .subcommand(
            Command::new("provides")
                .about("Find what package provides a file/capability")
                .arg(Arg::new("packages")
                    .value_name("FILES/CAPABILITIES")
                    .help("Files or capabilities to query")
                    .num_args(1..)
                    .required(true)),
        )
}

pub fn run(params: DnfParams) -> Result<()> {
    crate::init::try_light_init()?;

    super::apt::apply_common_options(&super::apt::PmParams { assume_yes: params.assume_yes });

    match params.subcmd.as_str() {
        "install" => {
            crate::install::install_packages(params.packages)?;
        }
        "remove" => {
            crate::remove::remove_packages(params.packages)?;
        }
        "update" => {
            crate::repo::sync_channel_metadata()?;
            println!("Metadata updated.");
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
            println!("Installed packages:");
            crate::list::list_packages_with_scope(crate::list::ListScope::Installed, "")?;
        }
        "provides" => {
            for item in &params.packages {
                let pkglines = crate::busybox::rpm::select_installed_pkglines_owning_path(item)?;
                if !pkglines.is_empty() {
                    for pkgline in pkglines {
                        let pkgkey = crate::package::pkgline2pkgkey(&pkgline)?;
                        let name = pkgkey.split("__").next().unwrap_or(&pkgkey);
                        println!("{} provides {}", name, item);
                    }
                } else {
                    println!("No package provides '{}'.", item);
                }
            }
        }
        _ => {
            eprintln!("dnf: unknown command '{}'", params.subcmd);
            std::process::exit(1);
        }
    }
    Ok(())
}