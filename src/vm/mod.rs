//! VM lifecycle management.
//!
//! Provides `epkg vm` subcommands for managing VM sessions:
//! - `vm start` - Start a VM for an environment (all platforms)
//! - `vm stop` - Stop a running VM (all platforms)
//! - `vm list` - List running VMs (all platforms)
//! - `vm status` - Show VM status (YAML) (all platforms)
//!
//! Session management (session.rs) is used by VM backends (libkrun, qemu) for
//! cross-platform VM discovery. This is needed on all platforms where VM backends run.
//!
//! Guest daemon (guest_daemon.rs) runs inside the VM to handle commands from host.

pub mod session;

mod start;
mod stop;
mod list;
mod status;

#[cfg(target_os = "linux")]
pub mod guest_daemon;

#[cfg(target_os = "linux")]
pub mod client;

// Re-export session functions used by VM backends (libkrun, qemu)
// Available on Linux (for qemu) or when libkrun feature is enabled
#[cfg(any(feature = "libkrun", target_os = "linux"))]
#[allow(dead_code, unused_imports)] // Linux VM guest build may not use these
pub use session::{register_vm_session, unregister_vm_session, VmConfig};

// These are only available/used with libkrun feature (also used by qemu on Linux)
#[cfg(feature = "libkrun")]
pub use session::{
    VmSessionInfo, discover_vm_session, register_vm_session_with_timeout, vm_socket_path_for_env,
};

// For Linux (qemu backend without libkrun feature), export discover_vm_session
#[cfg(all(target_os = "linux", not(feature = "libkrun")))]
pub use session::discover_vm_session;

pub use start::cmd_vm_start;
pub use stop::cmd_vm_stop;

pub use list::cmd_vm_list;
pub use status::cmd_vm_status;