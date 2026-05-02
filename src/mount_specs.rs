//! Mount spec strings - cross-platform mount policy declarations.
//!
//! This module provides mount spec strings that describe what directories to mount.
//! These strings are interpreted and executed differently per platform:
//! - Linux: namespace.rs parses and executes via bind mounts
//! - macOS/Windows: libkrun/core.rs parses and executes via virtiofs mounts
//!
//! Format: "SOURCE:OPTIONS" where SOURCE is host path, OPTIONS include ro/rw/try
//!
//! Symlink handling: All paths are canonicalized to resolve symlinks before
//! checking for coverage. This prevents redundant mounts when paths are symlinks.

use crate::models::dirs;
#[cfg(not(target_os = "linux"))]
use std::path::{Path, PathBuf};

/// Build VM mount policy as mount spec strings.
///
/// Policy: home_epkg, home_cache, opt_epkg, cwd, user --mounts
/// Platform-specific: /lib/modules (Linux), Windows drives (WSL)
pub fn build_vm_mount_policy(run_options: &crate::run::RunOptions) -> Vec<String> {
    let mut specs = Vec::new();

    // Core epkg directories
    add_epkg_mount_specs(&mut specs);

    // /lib/modules for kernel module loading (Linux only)
    #[cfg(target_os = "linux")]
    if std::path::Path::new("/lib/modules").exists() {
        specs.push("/lib/modules:ro,try".to_string());
    }

    // User-provided mount specs
    specs.extend(run_options.effective_sandbox.mount_specs.iter().cloned());

    // Current working directory
    if !run_options.chdir_to_env_root {
        if let Ok(cwd) = std::env::current_dir() {
            let cwd_str = cwd.to_string_lossy();
            if cwd.is_absolute() && cwd.exists() {
                log::trace!("VM mount policy: adding cwd {}", cwd_str);
                specs.push(format!("{}://{}", cwd_str, cwd_str));
            }
        }
    }

    // Windows drives from /mnt (WSL2)
    #[cfg(target_os = "linux")]
    specs.extend(windows_drive_mount_specs());

    specs
}

/// Resolve symlinks and filter out paths covered by existing mounts.
///
/// This function:
/// 1. Parses each mount spec to get host path
/// 2. Canonicalizes host path to resolve symlinks
/// 3. Filters out paths that are inside already-mounted paths
/// 4. Returns filtered specs with canonical paths
///
/// Returns: Vec<(host_canonical_path, guest_path, read_only, try_only)>
#[cfg(not(target_os = "linux"))]
pub fn resolve_and_filter_mount_specs(
    specs: &[String],
    env_root: &Path,
) -> Vec<(PathBuf, PathBuf, bool, bool)> {
    let mut filtered = Vec::new();
    let mut mounted_canonicals: Vec<PathBuf> = Vec::new();

    for spec_str in specs {
        if let Some((host_path, guest_path, read_only, try_only)) = parse_mount_spec(spec_str, env_root) {
            // Skip if not a directory
            if !host_path.exists() || !host_path.is_dir() {
                log::trace!("Mount spec skipped (not a directory): {}", host_path.display());
                continue;
            }

            // Canonicalize to resolve symlinks
            let canonical = match host_path.canonicalize() {
                Ok(c) => c,
                Err(e) => {
                    log::warn!("Cannot canonicalize {}: {}", host_path.display(), e);
                    continue;
                }
            };

            // Skip if covered by existing mount
            if is_path_covered_by(&canonical, &mounted_canonicals) {
                log::trace!("Mount spec skipped (covered by existing): {}", host_path.display());
                continue;
            }

            filtered.push((canonical.clone(), guest_path, read_only, try_only));
            mounted_canonicals.push(canonical);
        }
    }

    filtered
}

/// Parse mount spec string: "SOURCE:OPTIONS" or "SOURCE://TARGET:OPTIONS"
///
/// Returns: Some((host_path, guest_path, read_only, try_only))
#[cfg(not(target_os = "linux"))]
fn parse_mount_spec(spec_str: &str, env_root: &Path) -> Option<(PathBuf, PathBuf, bool, bool)> {
    let parts: Vec<&str> = spec_str.split(':').collect();

    // Skip pseudo filesystem types (tmpfs, proc, etc.)
    #[cfg(target_os = "linux")]
    if parts.len() >= 2 {
        if crate::mount::PSEUDO_FS_TYPES.contains(&parts[0]) {
            return None;
        }
    }

    let (source, target, options) = if parts.len() == 1 {
        (parts[0], parts[0], "")
    } else if parts.len() == 2 {
        // Check if second part is options or target
        if parts[1].contains(',') || parts[1] == "ro" || parts[1] == "rw" || parts[1].starts_with("ro") || parts[1].starts_with("try") {
            (parts[0], parts[0], parts[1])
        } else {
            (parts[0], parts[1], "")
        }
    } else if parts.len() >= 3 {
        (parts[0], parts[1], parts[2])
    } else {
        return None;
    };

    // Host path: handle @ prefix (env_root substitution)
    let host_path = if source.starts_with('@') {
        env_root.join(&source[1..])
    } else {
        PathBuf::from(source)
    };

    // Guest path: handle @ and // prefixes
    let guest_path = if target.starts_with('@') {
        env_root.join(&target[1..])
    } else if target.starts_with("//") {
        PathBuf::from(&target[2..])
    } else {
        PathBuf::from(target)
    };

    let read_only = options.contains("ro");
    let try_only = options.contains("try");

    Some((host_path, guest_path, read_only, try_only))
}

/// Check if a canonical path is covered by any of the mounted canonical paths.
/// Path is covered if it equals or starts with a mounted path.
#[cfg(not(target_os = "linux"))]
fn is_path_covered_by(path: &Path, mounted: &[PathBuf]) -> bool {
    mounted.iter().any(|m| path == m || path.starts_with(m))
}

/// Add core epkg directory mount specs: home_epkg, home_cache, opt_epkg, epkg_bin_dir.
/// Used by both VM mode and Fs mode (Linux).
pub fn add_epkg_mount_specs(specs: &mut Vec<String>) {
    let dirs = dirs();
    specs.push(format!("{}:try", dirs.home_epkg.display()));
    specs.push(format!("{}:try", dirs.home_cache.display()));
    // opt_epkg: read-only for system directory
    specs.push(format!("{}:ro,try", dirs.opt_epkg.display()));
    add_epkg_bin_dir_mount(specs);
}

/// Add mount spec for epkg binary directory.
/// When epkg is outside self env (e.g. target/debug), mount its dir.
pub fn add_epkg_bin_dir_mount(specs: &mut Vec<String>) {
    let Ok(epkg_exe) = std::env::current_exe() else { return };
    let Some(epkg_bin_dir) = epkg_exe.parent() else { return };

    // Skip if already inside self env
    let dirs = dirs();
    if epkg_bin_dir.starts_with(&dirs.home_epkg) || epkg_bin_dir.starts_with(&dirs.opt_epkg) {
        return;
    }

    specs.push(format!("{}:ro", epkg_bin_dir.display()));
}

/// Windows drives mounted under /mnt (WSL2) - Linux only
#[cfg(target_os = "linux")]
pub fn windows_drive_mount_specs() -> Vec<String> {
    let mut specs = Vec::new();
    let mnt = std::path::Path::new("/mnt");

    if !mnt.exists() {
        return specs;
    }

    let Ok(entries) = std::fs::read_dir(mnt) else { return specs };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Single letter = drive letter (c, d, etc.)
        if name_str.len() == 1 && name_str.chars().next().unwrap().is_ascii_alphabetic() {
            if entry.metadata().map(|m| m.is_dir()).unwrap_or(false) {
                specs.push(format!("/mnt/{}:try", name_str));
            }
        }
    }

    specs
}