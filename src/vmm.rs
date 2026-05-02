//! VMM backend selection and execution.
//!
//! This module provides a cross-platform interface for running commands
//! in virtual machines using different VMM backends (libkrun, qemu).
//!
//! ## Platform Support
//!
//! - Linux: Both libkrun and qemu backends available
//! - macOS (aarch64): libkrun backend available
//! - macOS (x86_64): qemu backend available (libkrun not supported)
//! - Windows: libkrun backend available
//!
//! ## Backend Selection
//!
//! Backends are tried in order specified by `--vmm` option or default order.
//! Default order: libkrun (if available), then qemu.
//!
//! ## Current Usage
//!
//! On Linux, this module is called from `namespace.rs` for VM sandbox mode.
//! On Windows/macOS, `run.rs` calls libkrun directly (TODO: unify paths).

use color_eyre::eyre;
use color_eyre::Result;
use std::path::Path;

#[cfg(target_os = "linux")]
use std::os::fd::OwnedFd;

use crate::run::RunOptions;

/// Try VMM backends in specified order until one succeeds.
///
/// This function never returns on success - the VM takes over execution.
/// Returns an error if all backends fail.
///
/// Note: Currently only called from Linux's namespace.rs.
/// Windows/macOS use direct libkrun calls in run.rs (TODO: unify).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn try_vmm_backends(
    env_root: &Path,
    run_options: &RunOptions,
    guest_command: &Path,
    vm_socket_path: Option<&Path>,
    vmm_order: &[String],
    #[cfg(target_os = "linux")] vm_daemon_ready_fd: Option<&OwnedFd>,
    #[cfg(not(target_os = "linux"))] _vm_daemon_ready_fd: Option<&()>,
) -> Result<()> {
    #[cfg(target_os = "linux")]
    let _ = crate::qemu::ensure_vmm_log_dir();

    let order: Vec<String> = if !vmm_order.is_empty() {
        vmm_order.to_vec()
    } else {
        default_vmm_order()
    };

    let mut last_err: Option<eyre::Report> = None;

    for backend in &order {
        match backend.as_str() {
            "libkrun" => {
                #[cfg(target_os = "linux")]
                {
                    if let Err(e) = try_krun_backend(env_root, run_options, guest_command, vm_daemon_ready_fd) {
                        log::warn!("libkrun backend failed, will try next VMM if any: {}", e);
                        last_err = Some(e);
                        continue;
                    }
                }
                #[cfg(not(target_os = "linux"))]
                {
                    if let Err(e) = try_krun_backend(env_root, run_options, guest_command) {
                        log::warn!("libkrun backend failed, will try next VMM if any: {}", e);
                        last_err = Some(e);
                        continue;
                    }
                }
                return Ok(());
            }
            "qemu" => {
                #[cfg(target_os = "linux")]
                {
                    if let Err(e) = try_qemu_backend(env_root, run_options, guest_command, vm_socket_path, vm_daemon_ready_fd) {
                        log::warn!("qemu backend failed, will try next VMM if any: {}", e);
                        last_err = Some(e);
                        continue;
                    }
                }
                #[cfg(not(target_os = "linux"))]
                {
                    if let Err(e) = try_qemu_backend(env_root, run_options, guest_command, vm_socket_path) {
                        log::warn!("qemu backend failed, will try next VMM if any: {}", e);
                        last_err = Some(e);
                        continue;
                    }
                }
                return Ok(());
            }
            other => {
                log::warn!("Unknown VMM backend '{}' in --vmm list, skipping", other);
            }
        }
    }

    if let Some(e) = last_err {
        return Err(eyre::eyre!(
            "All requested VMM backends failed (order: {:?}); last error: {}",
            order,
            e
        ));
    }

    Err(eyre::eyre!(
        "No usable VMM backend found for order {:?}. \
         Specify --vmm=libkrun,qemu or --vmm=qemu and ensure dependencies are installed.",
        order
    ))
}

/// Get default VMM backend order based on platform and features.
#[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
fn default_vmm_order() -> Vec<String> {
    let mut order = Vec::new();
    #[cfg(feature = "libkrun")]
    {
        order.push("libkrun".to_string());
    }
    #[cfg(target_os = "linux")]
    {
        order.push("qemu".to_string());
    }
    order
}

/// Try libkrun backend.
///
/// Returns Ok(()) when VM execution completes successfully (e.g., session reuse,
/// daemon mode, or no_exit mode). Returns Err if VM fails to start or execute.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn try_krun_backend(
    env_root: &Path,
    run_options: &RunOptions,
    guest_command: &Path,
    #[cfg(target_os = "linux")] vm_daemon_ready_fd: Option<&OwnedFd>,
) -> Result<()> {
    #[cfg(feature = "libkrun")]
    {
        log::debug!("Trying VMM backend: libkrun");
        #[cfg(target_os = "linux")]
        let result = crate::libkrun::run_command_in_krun(env_root, run_options, guest_command, vm_daemon_ready_fd);
        #[cfg(not(target_os = "linux"))]
        let result = crate::libkrun::run_command_in_krun(env_root, run_options, guest_command);
        match result {
            Ok(()) => Ok(()),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("HostAddressNotAvailable") || msg.contains("GuestMemoryMmap") {
                    return Err(eyre::eyre!(
                        "{}. Hint: host address-space layout can cause this; try --memory 4096 or EPKG_VM_MEMORY=4096, or use --vmm=qemu.",
                        msg
                    ));
                }
                Err(e)
            }
        }
    }
    #[cfg(not(feature = "libkrun"))]
    {
        #[cfg(target_os = "linux")]
        let _ = (env_root, run_options, guest_command, vm_daemon_ready_fd);
        #[cfg(not(target_os = "linux"))]
        let _ = (env_root, run_options, guest_command);
        log::debug!("VMM backend 'libkrun' requested but libkrun feature is disabled; skipping");
        Err(eyre::eyre!("libkrun feature disabled"))
    }
}

/// Try qemu backend.
///
/// Returns Ok(()) when VM execution completes successfully (e.g., session reuse
/// completes). Returns Err if VM fails to start or execute.
#[cfg(target_os = "linux")]
pub fn try_qemu_backend(
    env_root: &Path,
    run_options: &RunOptions,
    guest_command: &Path,
    vm_socket_path: Option<&Path>,
    vm_daemon_ready_fd: Option<&OwnedFd>,
) -> Result<()> {
    log::debug!("Trying VMM backend: qemu");
    crate::qemu::run_command_in_qemu(env_root, run_options, guest_command, vm_socket_path, vm_daemon_ready_fd)
}

#[cfg(not(target_os = "linux"))]
#[allow(dead_code)]
pub fn try_qemu_backend(
    _env_root: &Path,
    _run_options: &RunOptions,
    _guest_command: &Path,
    _vm_socket_path: Option<&Path>,
) -> Result<()> {
    log::debug!("VMM backend 'qemu' is only supported on Linux");
    Err(eyre::eyre!("qemu backend is only supported on Linux"))
}