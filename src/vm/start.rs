//! VM start command implementation.
//!
//! Unified for all platforms (Unix, Windows, macOS) using fork_and_execute()
//! with vm_daemon mode. The VM daemon mode sends a dummy command to VM,
//! waits for response (VM ready), then parent registers session.
//!
//! On Linux, uses a pipe to signal VM readiness across namespace boundary.

use std::path::Path;
use color_eyre::{Result, eyre};
use clap::ArgMatches;

use super::session::{VmConfig, discover_vm_session};

/// Parse key=value arguments into VmConfig.
fn parse_kv_args(args: Option<clap::parser::ValuesRef<String>>, vmm: Option<&str>) -> VmConfig {
    let mut config = VmConfig {
        backend: vmm.unwrap_or("libkrun").to_string(),
        ..Default::default()
    };

    if let Some(values) = args {
        for kv in values {
            let parts: Vec<&str> = kv.splitn(2, '=').collect();
            if parts.len() != 2 {
                log::warn!("Invalid key=value format: {}", kv);
                continue;
            }
            let key = parts[0].trim();
            let value = parts[1].trim();

            match key {
                "timeout" => {
                    if let Ok(v) = value.parse() {
                        config.timeout = Some(v);
                    }
                }
                "extend" => {
                    if let Ok(v) = value.parse() {
                        config.extend = v;
                    }
                }
                "cpus" => {
                    if let Ok(v) = value.parse() {
                        config.cpus = v;
                    }
                }
                "memory" => {
                    if let Ok(v) = value.parse() {
                        config.memory_mib = v;
                    }
                }
                _ => {
                    log::warn!("Unknown config key: {}", key);
                }
            }
        }
    }

    config
}

/// Start VM using fork_and_execute with daemon mode.
/// Unified for all platforms (Unix, Windows, macOS).
fn vm_start(env_root: &Path, env_name: &str, config: VmConfig) -> Result<()> {
    use crate::run::{RunOptions, fork_and_execute};
    use crate::models::{SandboxOptions, IsolateMode};

    // Check if VM already running
    if discover_vm_session(env_name)?.is_some() {
        return Err(eyre::eyre!("VM already running for {}", env_name));
    }

    let sandbox = SandboxOptions {
        isolate_mode: Some(IsolateMode::Vm),
        ..Default::default()
    };

    // Daemon mode: command is /bin/true, vm_daemon flag signals special handling
    let run_options = RunOptions {
        env_name: env_name.to_string(),
        command: "/bin/true".to_string(),          // Dummy command for daemon mode
        args: vec![],                              // No args
        sandbox: sandbox.clone(),
        effective_sandbox: sandbox,
        vm_daemon: true,                           // Signal daemon mode to downstream
        vm_keep_timeout: config.timeout,           // Keep VM alive after dummy command
        vm_cpus: Some(config.cpus as u8),
        vm_memory_mib: Some(config.memory_mib),
        vmm_order: vec![config.backend.clone()],
        background: true,                           // Parent returns immediately, child keeps VM alive
        ..Default::default()
    };

    log::info!("vm_start: starting VM daemon for {} (backend={}, timeout={:?})",
               env_name, config.backend, config.timeout);

    // fork_and_execute handles:
    // - Linux: namespace setup + bind mounts + QEMU/libkrun
    // - macOS/Windows: direct libkrun with virtiofs mounts
    // When dummy command returns with exit code 0, VM is ready
    // Note: returns Some(child_pid) for background mode, but we don't need it -
    // the child process is supervising the VM, session is registered by child.
    fork_and_execute(env_root, &run_options)?;

    // Note: VM session is already registered by child process (via register_vm_session_with_timeout
    // in libkrun/core.rs or qemu.rs). The session file at ~/.epkg/run/vm-sessions/ is
    // visible to parent because home_epkg is not namespace-isolated for the parent.
    // Parent just needs to return success; child is supervising the VM.
    log::info!("vm_start: VM started for {} (supervised by child process)", env_name);
    Ok(())
}

/// Entry point for `epkg vm start` command.
pub fn cmd_vm_start(args: &ArgMatches) -> Result<()> {
    let cfg = crate::models::config();
    let env_name = cfg.common.env_name.clone();
    let env_root = if cfg.common.env_root.is_empty() {
        crate::dirs::get_env_root(env_name.clone())?
    } else {
        std::path::PathBuf::from(&cfg.common.env_root)
    };

    // Parse key=value config with optional --vmm backend override
    let vmm = args.get_one::<String>("vmm").map(|s| s.as_str());
    let config = parse_kv_args(args.get_many::<String>("set"), vmm);

    // Unified start for all platforms
    vm_start(&env_root, &env_name, config.clone())?;

    let timeout_desc = match config.timeout {
        None => "immediate".to_string(),
        Some(0) => "never".to_string(),
        Some(n) => format!("{}s", n),
    };
    println!("VM started for {} (timeout={}, extend={}s)",
             env_name, timeout_desc, config.extend);

    Ok(())
}