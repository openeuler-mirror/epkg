//! APT-GET alias for APT package manager compatibility shim.
//!
//! Reuses apt module directly - apt-get is just an alias for apt.

use clap::Command;
use color_eyre::Result;

pub fn parse_options(matches: &clap::ArgMatches) -> Result<crate::busybox::apt::AptParams> {
    crate::busybox::apt::parse_options(matches)
}

pub fn command() -> Command {
    crate::busybox::apt::command()
        .name("apt-get")
        .about("Debian/Ubuntu package manager compatibility shim (epkg) - APT-GET alias")
        .visible_alias("apt")
}

pub fn run(params: crate::busybox::apt::AptParams) -> Result<()> {
    crate::busybox::apt::run(params)
}