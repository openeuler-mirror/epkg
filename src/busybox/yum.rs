//! YUM alias for DNF package manager compatibility shim.
//!
//! Reuses dnf module directly - yum is just an alias for dnf.

use clap::Command;
use color_eyre::Result;

pub fn parse_options(matches: &clap::ArgMatches) -> Result<crate::busybox::dnf::DnfParams> {
    crate::busybox::dnf::parse_options(matches)
}

pub fn command() -> Command {
    crate::busybox::dnf::command()
        .name("yum")
        .about("Fedora/RHEL package manager compatibility shim (epkg) - YUM alias for DNF")
        .visible_alias("dnf")
}

pub fn run(params: crate::busybox::dnf::DnfParams) -> Result<()> {
    crate::busybox::dnf::run(params)
}