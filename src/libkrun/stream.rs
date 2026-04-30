//! Vsock command bridge + streaming I/O (Unix sockets vs Windows named pipes).

use color_eyre::eyre;
use color_eyre::Result;
use std::io::{BufRead, Read, Write};
#[cfg(not(windows))]
use std::io::IsTerminal;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use crate::models::IoMode;

#[cfg(unix)]
use lazy_static::lazy_static;
#[cfg(unix)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::FlushFileBuffers;
#[cfg(windows)]
use windows::Win32::Foundation::HANDLE;

#[cfg(unix)]
lazy_static! {
    static ref RESIZE_PENDING: AtomicBool = AtomicBool::new(false);
    static ref ATEXIT_REGISTERED: AtomicBool = AtomicBool::new(false);
}

#[cfg(unix)]
extern "C" fn handle_sigwinch(_: i32) {
    RESIZE_PENDING.store(true, Ordering::SeqCst);
}

/// Flush Windows named pipe to ensure data is sent to the other end.
/// Standard File::flush() is a no-op; we need FlushFileBuffers for named pipes.
#[cfg(windows)]
fn flush_named_pipe(file: &std::fs::File) -> std::io::Result<()> {
    let handle = file.as_raw_handle();
    unsafe {
        let result = FlushFileBuffers(HANDLE(handle as *mut _));
        if result.is_err() {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Streaming message types for interactive/TUI modes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
enum StreamMessage {
    #[serde(rename = "stdin")]
    Stdin { data: String, seq: u64 },
    #[serde(rename = "stdin_eof")]
    StdinEof { seq: u64 },
    #[serde(rename = "stdout")]
    Stdout { data: String, seq: u64 },
    #[serde(rename = "stderr")]
    Stderr { data: String, seq: u64 },
    #[serde(rename = "resize")]
    Resize { cols: u16, rows: u16 },
    #[serde(rename = "exit")]
    Exit { code: i32 },
    #[serde(rename = "signal")]
    Signal { sig: i32 },
    #[serde(rename = "error")]
    Error { message: String },
}

pub(crate) fn build_command_request(
    cmd_parts: &[String],
    io_mode: IoMode,
    reuse_vm: bool,
    vm_keep_timeout_secs: Option<u32>,
    extend_timeout_secs: Option<u32>,
    env_vars: Option<&std::collections::HashMap<String, String>>,
    cwd: Option<&str>,
    stdin: Option<&[u8]>,
) -> serde_json::Value {
    crate::debug_epkg!("build_command_request: starting");
    // On Windows, is_terminal() can hang - avoid calling it
    let use_pty = matches!(io_mode, IoMode::Tty) ||
        (matches!(io_mode, IoMode::Auto) && {
            #[cfg(windows)]
            { false }  // Default to non-PTY on Windows to avoid is_terminal hang
            #[cfg(not(windows))]
            { std::io::stdin().is_terminal() }
        });
    let is_batch = matches!(io_mode, IoMode::Batch) ||
        (matches!(io_mode, IoMode::Auto) && {
            #[cfg(windows)]
            { true }  // Default to batch on Windows
            #[cfg(not(windows))]
            { false }
        });

    let mut m = serde_json::Map::new();
    m.insert(
        "command".to_string(),
        serde_json::Value::Array(
            cmd_parts
                .iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect(),
        ),
    );
    m.insert("pty".to_string(), serde_json::Value::Bool(use_pty));
    if is_batch {
        m.insert("batch".to_string(), serde_json::Value::Bool(true));
    }
    if reuse_vm {
        m.insert("reuse_vm".to_string(), serde_json::Value::Bool(true));
        if let Some(secs) = vm_keep_timeout_secs {
            m.insert("vm_keep_timeout_secs".to_string(), serde_json::Value::Number(secs.into()));
        }
        if let Some(secs) = extend_timeout_secs {
            m.insert("extend_timeout_secs".to_string(), serde_json::Value::Number(secs.into()));
        }
    }
    // Add environment variables if provided
    if let Some(env) = env_vars {
        if !env.is_empty() {
            m.insert(
                "env".to_string(),
                serde_json::Value::Object(
                    env.iter()
                        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                        .collect(),
                ),
            );
        }
    }
    // Add working directory if provided
    if let Some(dir) = cwd {
        log::debug!("build_command_request: adding cwd={}", dir);
        m.insert("cwd".to_string(), serde_json::Value::String(dir.to_string()));
    }
    // Add stdin data if provided (for hooks with NeedsTargets)
    // stdin is base64-encoded for safe JSON transport
    if let Some(stdin_data) = stdin {
        if !stdin_data.is_empty() {
            use base64::Engine;
            let encoded = base64::engine::general_purpose::STANDARD.encode(stdin_data);
            m.insert("stdin".to_string(), serde_json::Value::String(encoded));
        }
    }
    // Pass host time for guest clock synchronization
    // VM guest clocks often start at wrong values (e.g., epoch or 1999)
    let host_time_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    m.insert("host_time".to_string(), serde_json::Value::Number(host_time_ns.into()));
    serde_json::Value::Object(m)
}

fn resolve_io_mode(io_mode: IoMode) -> (bool, bool) {
    crate::debug_epkg!("resolve_io_mode: io_mode={:?}", io_mode);
    match io_mode {
        IoMode::Auto => {
            crate::debug_epkg!("resolve_io_mode: checking is_terminal...");
            // On Windows, is_terminal() can hang in some contexts.
            // Use a timeout to avoid blocking indefinitely.
            #[cfg(windows)]
            {
                // On Windows, default to batch mode to avoid is_terminal hang
                crate::debug_epkg!("resolve_io_mode: Windows - defaulting to batch mode");
                (false, true)
            }
            #[cfg(not(windows))]
            {
                let is_tty = std::io::stdin().is_terminal();
                crate::debug_epkg!("resolve_io_mode: is_terminal={}", is_tty);
                (is_tty, false)
            }
        }
        IoMode::Tty => (true, false),
        IoMode::Stream => (false, false),
        IoMode::Batch => (false, true),
    }
}

#[cfg(unix)]
fn handle_streaming_simple(stream: &mut std::os::unix::net::UnixStream, is_batch: bool) -> Result<i32> {
    use std::os::unix::io::AsRawFd;

    log::debug!("handle_streaming_simple: starting, is_batch={}", is_batch);

    let exit_code = Arc::new(Mutex::new(None));
    let exit_code_clone = exit_code.clone();

    // Reader thread: reads stdout/stderr/exit messages from stream
    let stream_clone = stream.try_clone()?;
    log::debug!("handle_streaming_simple: stream cloned, starting reader thread");
    let reader = thread::spawn(move || {
        log::debug!("handle_streaming_simple: reader thread started");
        let mut reader = std::io::BufReader::new(&stream_clone);
        let mut line = String::new();
        let mut total_bytes = 0usize;
        loop {
            line.clear();
            log::trace!("handle_streaming_simple: calling read_line");
            match reader.read_line(&mut line) {
                Ok(0) => {
                    log::debug!("handle_streaming_simple: reader got EOF after {} bytes", total_bytes);
                    // EOF: set exit_code so main loop can break
                    *exit_code_clone.lock().unwrap() = Some(-1);
                    break;
                }
                Ok(n) => {
                    total_bytes += n;
                    log::debug!("handle_streaming_simple: reader got line ({} bytes, total {})", n, total_bytes);
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    // Skip "READY" signal from reverse vsock handshake
                    if trimmed == "READY" {
                        crate::debug_epkg!("handle_streaming_simple: stream mode - skipped READY signal");
                        continue;
                    }

                    let msg: StreamMessage = match serde_json::from_str(trimmed) {
                        Ok(m) => m,
                        Err(e) => {
                            log::debug!("Failed to parse stream message: {} (line: {})", e, trimmed);
                            continue;
                        }
                    };

                    match msg {
                        StreamMessage::Stdout { data, .. } => {
                            if let Ok(decoded) = STANDARD.decode(&data) {
                                let _ = std::io::stdout().write_all(&decoded);
                                let _ = std::io::stdout().flush();
                            }
                        }
                        StreamMessage::Stderr { data, .. } => {
                            if let Ok(decoded) = STANDARD.decode(&data) {
                                let _ = std::io::stderr().write_all(&decoded);
                                let _ = std::io::stderr().flush();
                            }
                        }
                        StreamMessage::Exit { code } => {
                            log::debug!("handle_streaming_simple: got Exit with code={}", code);
                            *exit_code_clone.lock().unwrap() = Some(code);
                            break;
                        }
                        StreamMessage::Error { message } => {
                            log::debug!("VM error: {}", message);
                            // Error message is also a valid response - treat like exit with code -1
                            *exit_code_clone.lock().unwrap() = Some(-1);
                            break;
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    log::debug!("handle_streaming_simple: reader error: {}", e);
                    // Error: set exit_code so main loop can break
                    *exit_code_clone.lock().unwrap() = Some(-1);
                    break;
                }
            }
        }
        log::debug!("handle_streaming_simple: reader thread exiting");
    });
    log::debug!("handle_streaming_simple: reader thread spawned");

    // Stdin thread: only run when NOT batch mode (batch mode has no stdin)
    if !is_batch {
        let stdin_fd = std::io::stdin().as_raw_fd();
        let mut seq: u64 = 0;

        loop {
            if exit_code.lock().unwrap().is_some() {
                break;
            }

            // Poll for stdin input with timeout
            let mut pfd = [libc::pollfd {
                fd: stdin_fd,
                events: libc::POLLIN,
                revents: 0,
            }];
            let ready = unsafe { libc::poll(pfd.as_mut_ptr(), 1, 50) };
            if ready > 0 && ((pfd[0].revents & libc::POLLIN) != 0 || (pfd[0].revents & libc::POLLHUP) != 0) {
                let mut buf = [0u8; 4096];
                match std::io::stdin().read(&mut buf) {
                    Ok(0) => {
                        // EOF from host stdin - send StdinEof to close guest stdin pipe
                        let msg = StreamMessage::StdinEof { seq };
                        if let Ok(json) = serde_json::to_string(&msg) {
                            let _ = stream.write_all(json.as_bytes());
                            let _ = stream.write_all(b"\n");
                        }
                        break;
                    }
                    Ok(n) => {
                        let data = STANDARD.encode(&buf[..n]);
                        let msg = StreamMessage::Stdin { data, seq };
                        seq += 1;
                        if let Ok(json) = serde_json::to_string(&msg) {
                            let _ = stream.write_all(json.as_bytes());
                            let _ = stream.write_all(b"\n");
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    }

    reader.join().ok();
    let code = exit_code.lock().unwrap().unwrap_or(0);
    Ok(code)
}

#[cfg(unix)]
pub fn send_command_via_vsock(
    cmd_parts: &[String],
    io_mode: IoMode,
    reuse_vm: bool,
    vm_keep_timeout_secs: Option<u32>,
    extend_timeout_secs: Option<u32>,
    sock_path: &Path,
    env_vars: Option<&std::collections::HashMap<String, String>>,
    cwd: Option<&str>,
    stdin: Option<&[u8]>,
) -> Result<i32> {
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let (use_pty, is_batch) = resolve_io_mode(io_mode);
    log::debug!(
        "libkrun: io_mode={:?}, use_pty={}, is_batch={}, reuse_vm={}",
        io_mode,
        use_pty,
        is_batch,
        reuse_vm
    );

    let mut stream = {
        let mut retry_count = 0;
        let mut last_error = None;
        let mut s = None;
        while retry_count < 30 {
            match UnixStream::connect(sock_path) {
                Ok(unix_stream) => {
                    // Increase socket buffer sizes to handle large data transfers
                    // Default system buffer (~200KB) is too small for batch mode
                    use std::os::unix::io::AsRawFd;
                    super::set_socket_buffer_size(unix_stream.as_raw_fd());
                    s = Some(unix_stream);
                    break;
                }
                Err(e) => {
                    last_error = Some(e);
                    retry_count += 1;
                    if retry_count >= 30 {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }
        s.ok_or_else(|| {
            eyre::eyre!(
                "Failed to connect to Unix socket {} after 30 retries: {}",
                sock_path.display(),
                last_error.unwrap_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "connection failed"))
            )
        })?
    };

    log::debug!("libkrun: Unix socket connected, sending command {:?}", cmd_parts);

    let request = build_command_request(cmd_parts, io_mode, reuse_vm, vm_keep_timeout_secs, extend_timeout_secs, env_vars, cwd, stdin);
    let request_json = serde_json::to_vec(&request)?;
    stream.write_all(&request_json)?;
    stream.write_all(b"\n")?;
    log::debug!("libkrun: request sent ({} bytes)", request_json.len());

    if use_pty {
        handle_streaming_unix(&mut stream)
    } else {
        handle_streaming_simple(&mut stream, is_batch)
    }
}

/// RAII guard to restore terminal settings on drop.
/// Can also be manually restored via restore() method.
/// On macOS, also registers atexit handler because std::process::exit() skips Drop.
#[cfg(unix)]
struct TerminalGuard {
    original_mode: Option<libc::termios>,
    restored: bool,  // Track if we've already restored
}

#[cfg(unix)]
static TERMINAL_STATE: std::sync::Mutex<Option<libc::termios>> = std::sync::Mutex::new(None);

#[cfg(unix)]
extern "C" fn restore_terminal_atexit() {
    let guard = TERMINAL_STATE.lock().unwrap();
    if let Some(orig) = guard.as_ref() {
        // Use fd 0 (stdin) directly - no need for stdin.lock() in atexit
        // At process exit, all threads except the caller are already terminated
        let fd = 0;
        // Use TCSANOW for immediate restoration
        let _ = unsafe { libc::tcsetattr(fd, libc::TCSANOW, orig) };
        log::debug!("TerminalGuard::atexit: terminal restored via atexit handler");
    }
}

#[cfg(unix)]
impl TerminalGuard {
    fn new() -> Self {
        use std::os::unix::io::AsRawFd;

        let stdin = std::io::stdin();
        let stdin_lock = stdin.lock();
        let stdin_fd = stdin_lock.as_raw_fd();

        // Check if stdin is a tty before attempting terminal operations
        let is_tty = unsafe { libc::isatty(stdin_fd) } == 1;
        log::debug!("TerminalGuard::new: stdin_fd={}, is_tty={}", stdin_fd, is_tty);

        let original_mode = if is_tty {
            let mut termios: libc::termios = unsafe { std::mem::zeroed() };
            if unsafe { libc::tcgetattr(stdin_fd, &mut termios) } == 0 {
                // Log original mode: lflags
                let lflags = termios.c_lflag;
                log::debug!("TerminalGuard::new: tcgetattr succeeded, original lflags=0x{:04x} (icanon={} isig={} echo={} iexten={})",
                    lflags,
                    (lflags & libc::ICANON) != 0,
                    (lflags & libc::ISIG) != 0,
                    (lflags & libc::ECHO) != 0,
                    (lflags & libc::IEXTEN) != 0);
                Some(termios)
            } else {
                let err = std::io::Error::last_os_error();
                log::debug!("TerminalGuard::new: tcgetattr failed: {}", err);
                None
            }
        } else {
            None
        };

        // Store in global for atexit handler
        {
            let mut guard = TERMINAL_STATE.lock().unwrap();
            *guard = original_mode.clone();
        }

        // Register atexit handler (only once globally)
        if original_mode.is_some() &&
           ATEXIT_REGISTERED.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            // Note: libc::atexit returns 0 on success
            let result = unsafe { libc::atexit(restore_terminal_atexit) };
            log::debug!("TerminalGuard::new: atexit registered globally, result={}", result);
        }

        if let Some(ref orig) = original_mode {
            let mut raw = *orig;
            // cfmakeraw equivalent from libc
            raw.c_iflag &= !(libc::IGNBRK | libc::BRKINT | libc::PARMRK | libc::ISTRIP
                          | libc::INLCR | libc::IGNCR | libc::ICRNL | libc::IXON);
            raw.c_oflag &= !libc::OPOST;
            raw.c_lflag &= !(libc::ECHO | libc::ECHONL | libc::ICANON | libc::ISIG
                          | libc::IEXTEN);
            raw.c_cflag &= !(libc::CSIZE | libc::PARENB);
            raw.c_cflag |= libc::CS8;

            // Log new raw mode
            let raw_lflags = raw.c_lflag;
            log::debug!("TerminalGuard::new: setting raw mode, new lflags=0x{:04x} (icanon={} isig={} echo={} iexten={})",
                raw_lflags,
                (raw_lflags & libc::ICANON) != 0,
                (raw_lflags & libc::ISIG) != 0,
                (raw_lflags & libc::ECHO) != 0,
                (raw_lflags & libc::IEXTEN) != 0);

            if unsafe { libc::tcsetattr(stdin_fd, libc::TCSAFLUSH, &raw) } != 0 {
                let err = std::io::Error::last_os_error();
                log::debug!("TerminalGuard::new: tcsetattr(TCSAFLUSH, raw) failed: {}", err);
            } else {
                // Verify what was actually set
                let mut verify: libc::termios = unsafe { std::mem::zeroed() };
                if unsafe { libc::tcgetattr(stdin_fd, &mut verify) } == 0 {
                    let verify_lflags = verify.c_lflag;
                    log::debug!("TerminalGuard::new: after tcsetattr, verified lflags=0x{:04x} (icanon={} isig={} echo={})",
                        verify_lflags,
                        (verify_lflags & libc::ICANON) != 0,
                        (verify_lflags & libc::ISIG) != 0,
                        (verify_lflags & libc::ECHO) != 0);
                }
            }
        }
        // stdin_lock is dropped here, releasing the lock
        Self { original_mode, restored: false }
    }

    /// Manually restore terminal settings before drop.
    /// This should be called before the function returns to ensure
    /// the shell prompt is printed in cooked mode.
    fn restore(&mut self) {
        use std::os::unix::io::AsRawFd;

        if self.restored {
            log::debug!("TerminalGuard::restore: already restored, skipping");
            return;
        }
        self.restored = true;
        log::debug!("TerminalGuard::restore: starting restore");

        if let Some(ref orig) = self.original_mode {
            let orig_lflags = orig.c_lflag;
            log::debug!("TerminalGuard::restore: will restore to lflags=0x{:04x} (icanon={} isig={} echo={} iexten={})",
                orig_lflags,
                (orig_lflags & libc::ICANON) != 0,
                (orig_lflags & libc::ISIG) != 0,
                (orig_lflags & libc::ECHO) != 0,
                (orig_lflags & libc::IEXTEN) != 0);

            // Flush all output before restoring terminal settings
            let _ = std::io::stdout().flush();
            let _ = std::io::stderr().flush();

            // IMMEDIATELY restore - no delay!
            // The shell prints its prompt on SIGCHLD which happens as soon
            // as the child process exits. Any delay means the prompt gets
            // printed in raw mode.
            let stdin = std::io::stdin();
            let stdin_lock = stdin.lock();
            let fd = stdin_lock.as_raw_fd();
            log::debug!("TerminalGuard::restore: fd={}", fd);

            let result = unsafe { libc::tcsetattr(fd, libc::TCSANOW, orig) };
            log::debug!("TerminalGuard::restore: tcsetattr(TCSANOW) returned {}", result);

            // Verify what was actually restored
            let mut verify: libc::termios = unsafe { std::mem::zeroed() };
            if unsafe { libc::tcgetattr(fd, &mut verify) } == 0 {
                let verify_lflags = verify.c_lflag;
                log::debug!("TerminalGuard::restore: after tcsetattr, verified lflags=0x{:04x} (icanon={} isig={} echo={})",
                    verify_lflags,
                    (verify_lflags & libc::ICANON) != 0,
                    (verify_lflags & libc::ISIG) != 0,
                    (verify_lflags & libc::ECHO) != 0);
            } else {
                let err = std::io::Error::last_os_error();
                log::debug!("TerminalGuard::restore: tcgetattr verification failed: {}", err);
            }
        } else {
            log::debug!("TerminalGuard::restore: no original_mode to restore");
        }
        log::debug!("TerminalGuard::restore: done");
    }
}

#[cfg(unix)]
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // If not already restored, restore now (backup for early panics/returns)
        if !self.restored {
            log::debug!("TerminalGuard::drop: not restored yet, calling restore()");
            self.restore();
        } else {
            log::debug!("TerminalGuard::drop: already restored, nothing to do");
        }
    }
}

/// RAII guard to restore signal handlers on drop.
#[cfg(unix)]
struct SignalGuard {
    original_sigwinch: Option<nix::sys::signal::SigHandler>,
    original_sigint: Option<nix::sys::signal::SigHandler>,
    original_sigterm: Option<nix::sys::signal::SigHandler>,
}

#[cfg(unix)]
impl SignalGuard {
    fn new() -> Self {
        use nix::sys::signal::{signal, SigHandler, Signal};

        let original_sigwinch = unsafe { signal(Signal::SIGWINCH, SigHandler::Handler(handle_sigwinch)) }.ok();
        let original_sigint = unsafe { signal(Signal::SIGINT, SigHandler::SigIgn) }.ok();
        let original_sigterm = unsafe { signal(Signal::SIGTERM, SigHandler::SigIgn) }.ok();
        log::debug!("SignalGuard::new: sigwinch={:?}, sigint={:?}, sigterm={:?}",
            original_sigwinch, original_sigint, original_sigterm);
        Self { original_sigwinch, original_sigint, original_sigterm }
    }
}

#[cfg(unix)]
impl Drop for SignalGuard {
    fn drop(&mut self) {
        use nix::sys::signal::{signal, Signal};

        log::debug!("SignalGuard::drop: restoring signal handlers");
        // Clear any pending resize flag to avoid spurious resize after cleanup
        RESIZE_PENDING.store(false, Ordering::SeqCst);
        // Restore SIGWINCH
        if let Some(handler) = self.original_sigwinch {
            let _ = unsafe { signal(Signal::SIGWINCH, handler) };
        }
        // Restore SIGINT
        if let Some(handler) = self.original_sigint {
            let _ = unsafe { signal(Signal::SIGINT, handler) };
        }
        // Restore SIGTERM
        if let Some(handler) = self.original_sigterm {
            let _ = unsafe { signal(Signal::SIGTERM, handler) };
        }
        log::debug!("SignalGuard::drop: signal handlers restored");
    }
}

#[cfg(unix)]
fn handle_streaming_unix(stream: &mut std::os::unix::net::UnixStream) -> Result<i32> {
    use std::os::unix::io::AsRawFd;

    use console::Term;

    log::debug!("handle_streaming_unix: starting");

    // Create guards but we'll manually restore terminal before returning
    let mut term_guard = TerminalGuard::new();
    log::debug!("handle_streaming_unix: TerminalGuard created");
    let _signal_guard = SignalGuard::new();
    log::debug!("handle_streaming_unix: SignalGuard created");

    let term = Term::stdout();

    let stdin_fd = std::io::stdin().as_raw_fd();
    let stream_clone = stream.try_clone()?;
    let exit_code = Arc::new(Mutex::new(None));
    let exit_code_clone = exit_code.clone();

    log::debug!("handle_streaming_unix: spawning reader thread");
    let reader = thread::spawn(move || {
        let mut reader = std::io::BufReader::new(&stream_clone);
        let mut line = String::new();
        log::debug!("handle_streaming_unix: reader thread started");
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    // EOF: set exit_code so main loop can break and restore terminal
                    log::debug!("handle_streaming_unix: reader got EOF, setting exit_code=-1");
                    *exit_code_clone.lock().unwrap() = Some(-1);
                    break;
                }
                Ok(_) => {
                    if let Ok(msg) = serde_json::from_str::<StreamMessage>(&line) {
                        match msg {
                            StreamMessage::Stdout { data, .. } => {
                                if let Ok(decoded) = STANDARD.decode(&data) {
                                    let _ = std::io::stdout().write_all(&decoded);
                                    let _ = std::io::stdout().flush();
                                }
                            }
                            StreamMessage::Stderr { data, .. } => {
                                if let Ok(decoded) = STANDARD.decode(&data) {
                                    let _ = std::io::stderr().write_all(&decoded);
                                    let _ = std::io::stderr().flush();
                                }
                            }
                            StreamMessage::Exit { code } => {
                                log::debug!("handle_streaming_unix: got Exit code={}", code);
                                *exit_code_clone.lock().unwrap() = Some(code);
                                break;
                            }
                            StreamMessage::Error { message } => {
                                log::debug!("VM error: {}", message);
                                // Error message is also a valid response - treat like exit with code -1
                                *exit_code_clone.lock().unwrap() = Some(-1);
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    log::debug!("handle_streaming_unix: reader error: {}", e);
                    *exit_code_clone.lock().unwrap() = Some(-1);
                    break;
                }
            }
        }
        log::debug!("handle_streaming_unix: reader thread exiting");
    });
    log::debug!("handle_streaming_unix: reader thread spawned, entering main loop");

    let mut seq: u64 = 0;
    let mut buf = [0u8; 4096];
    let mut poll_count = 0;
    loop {
        poll_count += 1;
        if poll_count % 20 == 0 {
            log::debug!("handle_streaming_unix: poll iteration {}, checking exit_code", poll_count);
        }
        if exit_code.lock().unwrap().is_some() {
            log::debug!("handle_streaming_unix: exit_code detected, breaking from main loop");
            break;
        }

        if RESIZE_PENDING.swap(false, Ordering::SeqCst) {
            let (cols, rows) = term.size();
            let resize_msg = StreamMessage::Resize { cols, rows };
            if let Ok(json) = serde_json::to_string(&resize_msg) {
                let _ = stream.write_all(json.as_bytes());
                let _ = stream.write_all(b"\n");
            }
        }

        let mut pfd = [libc::pollfd {
            fd:      stdin_fd,
            events:  libc::POLLIN,
            revents: 0,
        }];
        log::trace!("handle_streaming_unix: calling poll()");
        let ready = unsafe { libc::poll(pfd.as_mut_ptr(), 1, 50) };
        log::trace!("handle_streaming_unix: poll returned {}", ready);
        if ready > 0 && ((pfd[0].revents & libc::POLLIN) != 0 || (pfd[0].revents & libc::POLLHUP) != 0) {
            match std::io::stdin().read(&mut buf) {
                Ok(0) => {
                    log::debug!("handle_streaming_unix: stdin EOF, breaking");
                    break;
                }
                Ok(n) => {
                    log::debug!("handle_streaming_unix: stdin read {} bytes", n);
                    let data = STANDARD.encode(&buf[..n]);
                    let msg = StreamMessage::Stdin { data, seq };
                    seq += 1;
                    if let Ok(json) = serde_json::to_string(&msg) {
                        let _ = stream.write_all(json.as_bytes());
                        let _ = stream.write_all(b"\n");
                    }
                }
                Err(e) => {
                    log::debug!("handle_streaming_unix: stdin error: {}, breaking", e);
                    break;
                }
            }
        }
    }
    log::debug!("handle_streaming_unix: main loop exited after {} iterations, calling reader.join()", poll_count);

    reader.join().ok();
    log::debug!("handle_streaming_unix: reader thread joined, about to restore terminal");

    // CRITICAL: Restore terminal IMMEDIATELY before any other output
    // The shell prints its prompt on SIGCHLD which fires when child exits.
    // We must restore BEFORE that happens, with NO delays or logging.
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    log::debug!("handle_streaming_unix: flushed stdout/stderr, calling restore()");
    term_guard.restore();
    log::debug!("handle_streaming_unix: restore() completed");

    let code = exit_code.lock().unwrap().unwrap_or(0);
    log::debug!("handle_streaming_unix: returning with code {}", code);
    Ok(code)
}

#[cfg(windows)]
pub fn send_command_via_vsock(
    cmd_parts: &[String],
    io_mode: IoMode,
    reuse_vm: bool,
    vm_keep_timeout_secs: Option<u32>,
    extend_timeout_secs: Option<u32>,
    sock_path: &Path,
    env_vars: Option<&std::collections::HashMap<String, String>>,
    cwd: Option<&str>,
    stdin: Option<&[u8]>,
) -> Result<i32> {
    crate::debug_epkg!("libkrun_stream: send_command_via_vsock starting");
    crate::debug_epkg!("libkrun_stream: about to resolve io_mode...");
    let (use_pty, is_batch) = resolve_io_mode(io_mode);
    crate::debug_epkg!("libkrun_stream: io_mode resolved");
    crate::debug_epkg!("libkrun_stream: io_mode={:?}, use_pty={}, is_batch={}, reuse_vm={}",
        io_mode, use_pty, is_batch, reuse_vm);
    crate::debug_epkg!("libkrun_stream: connecting to vsock bridge at {:?}", sock_path);

    crate::debug_epkg!("libkrun_stream: about to call connect_vsock_bridge");
    let mut stream = super::bridge::connect_vsock_bridge(sock_path, super::bridge::VSOCK_BRIDGE_MAX_RETRIES)?;
    crate::debug_epkg!("libkrun_stream: connected to vsock bridge");

    // WaitNamedPipeA already ensures the named pipe is ready (guest has connected).
    // The guest sends READY signal immediately after connection.
    // We can proceed directly - handlers will skip the READY signal.
    // No additional delay needed since WaitNamedPipeA ensures the guest is ready.
    crate::debug_epkg!("libkrun_stream: connection ready, proceeding immediately");
    let request = build_command_request(cmd_parts, io_mode, reuse_vm, vm_keep_timeout_secs, extend_timeout_secs, env_vars, cwd, stdin);
    crate::debug_epkg!("libkrun_stream: serializing to json");
    let request_json = serde_json::to_vec(&request)?;
    crate::debug_epkg!("libkrun_stream: writing {} bytes to stream", request_json.len());
    stream.write_all(&request_json)?;
    crate::debug_epkg!("libkrun_stream: writing newline");
    stream.write_all(b"\n")?;
    crate::debug_epkg!("libkrun_stream: flushing named pipe with FlushFileBuffers");
    flush_named_pipe(&stream)?;
    crate::debug_epkg!("libkrun_stream: request sent");

    // All non-batch modes need stdin forwarding on Windows
    if is_batch {
        handle_batch_response_windows(&mut stream)
    } else {
        handle_streaming_with_stdin(&mut stream)
    }
}

#[cfg(windows)]
fn handle_streaming_with_stdin(stream: &mut std::fs::File) -> Result<i32> {
    use std::sync::mpsc;
    use std::time::Duration;

    let stream_clone = stream.try_clone()?;
    let exit_code = Arc::new(Mutex::new(None));
    let exit_code_clone = exit_code.clone();

    let reader = thread::spawn(move || {
        let mut reader = std::io::BufReader::new(&stream_clone);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if let Ok(msg) = serde_json::from_str::<StreamMessage>(trimmed) {
                        match msg {
                            StreamMessage::Stdout { data, .. } => {
                                if let Ok(decoded) = STANDARD.decode(&data) {
                                    let _ = std::io::stdout().write_all(&decoded);
                                    let _ = std::io::stdout().flush();
                                }
                            }
                            StreamMessage::Stderr { data, .. } => {
                                if let Ok(decoded) = STANDARD.decode(&data) {
                                    let _ = std::io::stderr().write_all(&decoded);
                                    let _ = std::io::stderr().flush();
                                }
                            }
                            StreamMessage::Exit { code } => {
                                *exit_code_clone.lock().unwrap() = Some(code);
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });

    let (stdin_tx, stdin_rx) = mpsc::channel::<Vec<u8>>();
    let _stdin_thread = thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match std::io::stdin().read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if stdin_tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let mut seq: u64 = 0;
    loop {
        if exit_code.lock().unwrap().is_some() {
            break;
        }
        match stdin_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(bytes) => {
                let data = STANDARD.encode(&bytes);
                let msg = StreamMessage::Stdin { data, seq };
                seq += 1;
                if let Ok(json) = serde_json::to_string(&msg) {
                    let _ = stream.write_all(json.as_bytes());
                    let _ = stream.write_all(b"\n");
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    reader.join().ok();
    let code = exit_code.lock().unwrap().unwrap_or(0);
    Ok(code)
}

/// Handle batch mode on Windows using stream protocol.
#[cfg(windows)]
fn handle_batch_response_windows(stream: &mut std::fs::File) -> Result<i32> {
    let mut reader = std::io::BufReader::new(stream);
    let mut line = String::new();
    let mut exit_code = 0;

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed == "READY" {
                    continue;
                }

                let msg: StreamMessage = match serde_json::from_str(trimmed) {
                    Ok(m) => m,
                    Err(e) => {
                        log::debug!("Failed to parse stream message: {} (line: {})", e, trimmed);
                        continue;
                    }
                };

                match msg {
                    StreamMessage::Stdout { data, .. } => {
                        if let Ok(decoded) = STANDARD.decode(&data) {
                            let _ = std::io::stdout().write_all(&decoded);
                            let _ = std::io::stdout().flush();
                        }
                    }
                    StreamMessage::Stderr { data, .. } => {
                        if let Ok(decoded) = STANDARD.decode(&data) {
                            let _ = std::io::stderr().write_all(&decoded);
                            let _ = std::io::stderr().flush();
                        }
                    }
                    StreamMessage::Exit { code } => {
                        exit_code = code;
                        break;
                    }
                    StreamMessage::Error { message } => {
                        return Err(eyre::eyre!("VM error: {}", message));
                    }
                    _ => {}
                }
            }
            Err(_) => break,
        }
    }

    Ok(exit_code)
}

// =============================================================================
// Reverse mode support: Send command over an existing stream
// =============================================================================

/// Simple output-only handler for reverse mode and other cases where stdin is not available.
/// Reads line by line and handles stdout/stderr/exit messages.
/// Note: batch mode now uses the same stream protocol as stream mode, so no distinction needed.
#[cfg(not(windows))]
#[allow(dead_code)]
fn handle_output_only(stream: &mut (impl Read + Write), _is_batch: bool) -> Result<i32> {
    use std::io::BufReader;

    // Both batch and stream modes use the same stream protocol now
    let reader = BufReader::new(stream);
    let mut exit_code = 0;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if line == "READY" {
            continue;
        }

        let msg: StreamMessage = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(e) => {
                log::debug!("Failed to parse stream message: {} (line: {})", e, line);
                continue;
            }
        };

        match msg {
            StreamMessage::Stdout { data, .. } => {
                let stdout_bytes = STANDARD.decode(&data)
                    .map_err(|e| eyre::eyre!("Failed to decode stdout: {}", e))?;
                std::io::stdout().write_all(&stdout_bytes)?;
                std::io::stdout().flush()?;
            }
            StreamMessage::Stderr { data, .. } => {
                let stderr_bytes = STANDARD.decode(&data)
                    .map_err(|e| eyre::eyre!("Failed to decode stderr: {}", e))?;
                std::io::stderr().write_all(&stderr_bytes)?;
                std::io::stderr().flush()?;
            }
            StreamMessage::Exit { code } => {
                exit_code = code;
                break;
            }
            StreamMessage::Error { message } => {
                return Err(eyre::eyre!("VM error: {}", message));
            }
            _ => {}
        }
    }
    Ok(exit_code)
}

/// Send command over an existing stream (for reverse mode).
/// In reverse mode, the Host accepts a connection from Guest, then uses that
/// connection to send commands and receive results.
/// For non-batch modes, this forwards stdin from host to the guest.
#[cfg(not(windows))]
#[allow(dead_code)]
pub fn send_command_over_stream(
    cmd_parts: &[String],
    io_mode: IoMode,
    reuse_vm: bool,
    vm_keep_timeout_secs: Option<u32>,
    extend_timeout_secs: Option<u32>,
    env_vars: Option<&std::collections::HashMap<String, String>>,
    cwd: Option<&str>,
    stdin: Option<&[u8]>,
    mut stream: impl Read + Write + Send + 'static + std::os::unix::io::AsRawFd,
) -> Result<i32> {
    use std::os::unix::io::AsRawFd;

    crate::debug_epkg!("libkrun_stream: send_command_over_stream starting");
    let (use_pty, is_batch) = resolve_io_mode(io_mode);
    crate::debug_epkg!("libkrun_stream: io_mode={:?}, use_pty={}, reuse_vm={}",
        io_mode, use_pty, reuse_vm);

    // Create TerminalGuard for PTY mode to restore terminal on exit
    let mut term_guard = if use_pty {
        Some(TerminalGuard::new())
    } else {
        None
    };

    // Build and send command request
    let request = build_command_request(cmd_parts, io_mode, reuse_vm, vm_keep_timeout_secs, extend_timeout_secs, env_vars, cwd, stdin);
    let request_json = serde_json::to_vec(&request)?;
    stream.write_all(&request_json)?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    // For batch mode, use simple output-only handler
    if is_batch {
        return handle_output_only(&mut stream, true);
    }

    // For non-batch mode, use polling to handle both stdin forwarding and output
    // Set stream to non-blocking mode
    let stream_fd = stream.as_raw_fd();
    let original_flags = unsafe { libc::fcntl(stream_fd, libc::F_GETFL) };
    if original_flags >= 0 {
        unsafe { libc::fcntl(stream_fd, libc::F_SETFL, original_flags | libc::O_NONBLOCK); }
    }

    let stdin_fd = std::io::stdin().as_raw_fd();
    let mut exit_code = 0;
    let mut got_exit = false;
    let mut stdin_eof_sent = false;
    let mut line_buf = Vec::new();
    let mut seq: u64 = 0;

    loop {
        if got_exit {
            break;
        }

        // Poll both stdin and stream for readability
        let mut poll_fds = [
            libc::pollfd { fd: stdin_fd, events: libc::POLLIN, revents: 0 },
            libc::pollfd { fd: stream_fd, events: libc::POLLIN, revents: 0 },
        ];

        let ready = unsafe { libc::poll(poll_fds.as_mut_ptr(), 2, 50) };

        if ready < 0 {
            let errno = std::io::Error::last_os_error();
            if errno.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            break;
        }

        // Check for stdin input (or EOF via POLLHUP)
        if !stdin_eof_sent && ((poll_fds[0].revents & libc::POLLIN) != 0 || (poll_fds[0].revents & libc::POLLHUP) != 0) {
            let mut buf = [0u8; 4096];
            match std::io::stdin().read(&mut buf) {
                Ok(0) => {
                    // EOF from stdin - send stdin_eof to close guest's stdin pipe
                    let msg = StreamMessage::StdinEof { seq };
                    if let Ok(json) = serde_json::to_string(&msg) {
                        let _ = stream.write_all(json.as_bytes());
                        let _ = stream.write_all(b"\n");
                        let _ = stream.flush();
                    }
                    stdin_eof_sent = true;
                }
                Ok(n) => {
                    let encoded = STANDARD.encode(&buf[..n]);
                    let msg = StreamMessage::Stdin { data: encoded, seq };
                    seq += 1;
                    if let Ok(json) = serde_json::to_string(&msg) {
                        let _ = stream.write_all(json.as_bytes());
                        let _ = stream.write_all(b"\n");
                        let _ = stream.flush();
                    }
                }
                Err(_) => {}
            }
        }

        // Check for stream output (or connection closed via POLLHUP - may still have data)
        if (poll_fds[1].revents & libc::POLLIN) != 0 || (poll_fds[1].revents & libc::POLLHUP) != 0 {
            let mut buf = [0u8; 4096];
            match stream.read(&mut buf) {
                Ok(0) => {
                    // EOF from stream
                    log::debug!("libkrun: stream EOF received, got_exit={}", got_exit);
                    break;
                }
                Ok(n) => {
                    // Process received data - may contain partial lines
                    for &byte in &buf[..n] {
                        line_buf.push(byte);
                        if byte == b'\n' {
                            let line = String::from_utf8_lossy(&line_buf);
                            let trimmed = line.trim();
                            if !trimmed.is_empty() && trimmed != "READY" {
                                if let Ok(msg) = serde_json::from_str::<StreamMessage>(trimmed) {
                                    match msg {
                                        StreamMessage::Stdout { data, .. } => {
                                            if let Ok(decoded) = STANDARD.decode(&data) {
                                                let _ = std::io::stdout().write_all(&decoded);
                                                let _ = std::io::stdout().flush();
                                            }
                                        }
                                        StreamMessage::Stderr { data, .. } => {
                                            if let Ok(decoded) = STANDARD.decode(&data) {
                                                let _ = std::io::stderr().write_all(&decoded);
                                                let _ = std::io::stderr().flush();
                                            }
                                        }
                                        StreamMessage::Exit { code } => {
                                            exit_code = code;
                                            got_exit = true;
                                        }
                                        StreamMessage::Error { message } => {
                                            log::debug!("VM error: {}", message);
                                            // Error message is also a valid response - treat like exit
                                            got_exit = true;
                                            exit_code = -1;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            line_buf.clear();
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => break,
            }
        }
    }

    // Restore original flags
    if original_flags >= 0 {
        unsafe { libc::fcntl(stream_fd, libc::F_SETFL, original_flags); }
    }

    // CRITICAL: Restore terminal IMMEDIATELY before returning
    // The shell prints its prompt on SIGCHLD which fires when child exits.
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    if let Some(ref mut guard) = term_guard {
        guard.restore();
    }

    // If we didn't receive an exit message, the connection was closed prematurely
    if !got_exit {
        log::warn!("libkrun: connection closed without exit message, command may not have executed");
        return Err(eyre::eyre!("VM connection closed prematurely - command may not have executed"));
    }

    Ok(exit_code)
}

/// Windows-specific function to send command over a named pipe.
/// Uses FlushFileBuffers to ensure data is sent immediately.
#[cfg(windows)]
pub fn send_command_over_named_pipe(
    cmd_parts: &[String],
    io_mode: IoMode,
    reuse_vm: bool,
    vm_keep_timeout_secs: Option<u32>,
    extend_timeout_secs: Option<u32>,
    env_vars: Option<&std::collections::HashMap<String, String>>,
    cwd: Option<&str>,
    stdin: Option<&[u8]>,
    mut stream: std::fs::File,
) -> Result<i32> {
    crate::debug_epkg!("libkrun_stream: send_command_over_named_pipe starting");
    let (_use_pty, is_batch) = resolve_io_mode(io_mode);
    crate::debug_epkg!("libkrun_stream: io_mode={:?}, is_batch={}, reuse_vm={}",
        io_mode, is_batch, reuse_vm);

    // Build and send command request
    let request = build_command_request(cmd_parts, io_mode, reuse_vm, vm_keep_timeout_secs, extend_timeout_secs, env_vars, cwd, stdin);
    let request_json = serde_json::to_vec(&request)?;
    crate::debug_epkg!("libkrun_stream: [PERF] writing {} bytes to named pipe", request_json.len());
    let write_start = std::time::Instant::now();
    stream.write_all(&request_json)?;
    stream.write_all(b"\n")?;

    // CRITICAL: Use FlushFileBuffers to ensure data is sent to the named pipe.
    // Standard flush() is a no-op for File; named pipes need this Windows API.
    flush_named_pipe(&stream)?;
    crate::debug_epkg!("libkrun_stream: [PERF] write+flush took {:.3}ms", write_start.elapsed().as_secs_f64() * 1000.0);
    crate::debug_epkg!("libkrun_stream: [PERF] waiting for response...");

    // Handle response with stdin forwarding
    let response_start = std::time::Instant::now();
    let result = if is_batch {
        handle_batch_response_windows(&mut stream)
    } else {
        handle_streaming_with_stdin(&mut stream)
    };
    crate::debug_epkg!("libkrun_stream: [PERF] response handling took {:.3}ms", response_start.elapsed().as_secs_f64() * 1000.0);

    match &result {
        Ok(code) => crate::debug_epkg!("libkrun_stream: command completed with exit code {}", code),
        Err(e) => crate::debug_epkg!("libkrun_stream: command failed with error: {}", e),
    }

    result
}
