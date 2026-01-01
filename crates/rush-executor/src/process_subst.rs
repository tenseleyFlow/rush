//! Process substitution implementation using named pipes (FIFOs)
//!
//! Process substitution allows commands to be used where filenames are expected:
//! - `<(command)` creates a FIFO that provides the command's stdout
//! - `>(command)` creates a FIFO that feeds into the command's stdin
//!
//! Example: `diff <(ls dir1) <(ls dir2)`

use nix::sys::stat::Mode;
use nix::unistd::{fork, ForkResult, Pid};
use rush_expand::Context;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProcessSubstError {
    #[error("Failed to create FIFO: {0}")]
    FifoCreationError(String),

    #[error("Fork failed: {0}")]
    ForkError(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Command execution error: {0}")]
    CommandError(String),
}

/// Global registry of active process substitutions
/// Tracks FIFOs and PIDs for cleanup
static PROCESS_SUBST_REGISTRY: Mutex<Option<ProcessSubstRegistry>> = Mutex::new(None);

struct ProcessSubstRegistry {
    /// Map of FIFO paths to process IDs
    active_fifos: HashMap<PathBuf, Pid>,
    /// Counter for unique FIFO names
    counter: u64,
}

impl ProcessSubstRegistry {
    fn new() -> Self {
        Self {
            active_fifos: HashMap::new(),
            counter: 0,
        }
    }

    fn generate_fifo_path(&mut self) -> PathBuf {
        let pid = std::process::id();
        let count = self.counter;
        self.counter += 1;
        PathBuf::from(format!("/tmp/rush-psub-{}-{}", pid, count))
    }

    fn register(&mut self, path: PathBuf, pid: Pid) {
        self.active_fifos.insert(path, pid);
    }

    fn cleanup_all(&mut self) {
        // Remove all FIFOs
        for (path, _) in self.active_fifos.drain() {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Initialize the process substitution registry
pub fn init_registry() {
    let mut registry = PROCESS_SUBST_REGISTRY.lock().unwrap();
    if registry.is_none() {
        *registry = Some(ProcessSubstRegistry::new());
    }
}

/// Cleanup all process substitutions (call on shell exit)
pub fn cleanup_all() {
    let mut registry = PROCESS_SUBST_REGISTRY.lock().unwrap();
    if let Some(ref mut reg) = *registry {
        reg.cleanup_all();
    }
}

/// Execute process substitution for input: <(command)
/// Creates a FIFO and forks a process that writes command stdout to it
/// Returns the FIFO path that can be used as a filename
#[cfg(unix)]
pub fn execute_process_subst_input(
    command: &str,
    context: &mut Context,
) -> Result<String, ProcessSubstError> {
    init_registry();

    // Generate unique FIFO path
    let fifo_path = {
        let mut registry = PROCESS_SUBST_REGISTRY.lock().unwrap();
        let reg = registry.as_mut().unwrap();
        reg.generate_fifo_path()
    };

    // Create the FIFO
    nix::unistd::mkfifo(&fifo_path, Mode::S_IRUSR | Mode::S_IWUSR)
        .map_err(|e| ProcessSubstError::FifoCreationError(format!("{}", e)))?;

    // Fork a process to execute the command
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            // Child process: execute command with stdout redirected to FIFO
            use std::fs::OpenOptions;
            use std::os::unix::io::AsRawFd;

            // Open FIFO for writing
            let fifo_file = OpenOptions::new()
                .write(true)
                .open(&fifo_path)
                .expect("Failed to open FIFO");

            // Redirect stdout to FIFO
            let fifo_fd = fifo_file.as_raw_fd();
            unsafe {
                nix::unistd::dup2(fifo_fd, 1).expect("Failed to dup2 stdout");
            }

            // Execute the command via the command substitution executor
            let executor_opt = context.command_executor.0.clone();
            if let Some(executor) = executor_opt {
                match executor(command, context) {
                    Ok(_) => std::process::exit(0),
                    Err(_) => std::process::exit(1),
                }
            } else {
                // Fallback: use sh -c
                let status = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(command)
                    .status()
                    .expect("Failed to execute command");
                std::process::exit(status.code().unwrap_or(1));
            }
        }
        Ok(ForkResult::Parent { child }) => {
            // Parent process: register the FIFO and return path
            let mut registry = PROCESS_SUBST_REGISTRY.lock().unwrap();
            let reg = registry.as_mut().unwrap();
            reg.register(fifo_path.clone(), child);

            Ok(fifo_path.to_string_lossy().to_string())
        }
        Err(e) => {
            // Fork failed - clean up FIFO
            let _ = std::fs::remove_file(&fifo_path);
            Err(ProcessSubstError::ForkError(format!("{}", e)))
        }
    }
}

/// Execute process substitution for output: >(command)
/// Creates a FIFO and forks a process that reads from it as stdin
/// Returns the FIFO path that can be used as a filename
#[cfg(unix)]
pub fn execute_process_subst_output(
    command: &str,
    context: &mut Context,
) -> Result<String, ProcessSubstError> {
    init_registry();

    // Generate unique FIFO path
    let fifo_path = {
        let mut registry = PROCESS_SUBST_REGISTRY.lock().unwrap();
        let reg = registry.as_mut().unwrap();
        reg.generate_fifo_path()
    };

    // Create the FIFO
    nix::unistd::mkfifo(&fifo_path, Mode::S_IRUSR | Mode::S_IWUSR)
        .map_err(|e| ProcessSubstError::FifoCreationError(format!("{}", e)))?;

    // Fork a process to execute the command
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            // Child process: execute command with stdin redirected from FIFO
            use std::fs::OpenOptions;
            use std::os::unix::io::AsRawFd;

            // Open FIFO for reading
            let fifo_file = OpenOptions::new()
                .read(true)
                .open(&fifo_path)
                .expect("Failed to open FIFO");

            // Redirect stdin from FIFO
            let fifo_fd = fifo_file.as_raw_fd();
            unsafe {
                nix::unistd::dup2(fifo_fd, 0).expect("Failed to dup2 stdin");
            }

            // Execute the command via the command substitution executor
            let executor_opt = context.command_executor.0.clone();
            if let Some(executor) = executor_opt {
                match executor(command, context) {
                    Ok(_) => std::process::exit(0),
                    Err(_) => std::process::exit(1),
                }
            } else {
                // Fallback: use sh -c
                let status = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(command)
                    .status()
                    .expect("Failed to execute command");
                std::process::exit(status.code().unwrap_or(1));
            }
        }
        Ok(ForkResult::Parent { child }) => {
            // Parent process: register the FIFO and return path
            let mut registry = PROCESS_SUBST_REGISTRY.lock().unwrap();
            let reg = registry.as_mut().unwrap();
            reg.register(fifo_path.clone(), child);

            Ok(fifo_path.to_string_lossy().to_string())
        }
        Err(e) => {
            // Fork failed - clean up FIFO
            let _ = std::fs::remove_file(&fifo_path);
            Err(ProcessSubstError::ForkError(format!("{}", e)))
        }
    }
}

/// Wait for all process substitutions to complete
#[cfg(unix)]
pub fn wait_for_all() {
    use nix::sys::wait::{waitpid, WaitPidFlag};

    let mut registry = PROCESS_SUBST_REGISTRY.lock().unwrap();
    if let Some(ref mut reg) = *registry {
        // Wait for all child processes (non-blocking)
        for (path, pid) in reg.active_fifos.drain() {
            let _ = waitpid(pid, Some(WaitPidFlag::WNOHANG));
            let _ = std::fs::remove_file(&path);
        }
    }
}

#[cfg(not(unix))]
pub fn execute_process_subst_input(
    _command: &str,
    _context: &mut Context,
) -> Result<String, ProcessSubstError> {
    Err(ProcessSubstError::CommandError(
        "Process substitution not supported on this platform".to_string(),
    ))
}

#[cfg(not(unix))]
pub fn execute_process_subst_output(
    _command: &str,
    _context: &mut Context,
) -> Result<String, ProcessSubstError> {
    Err(ProcessSubstError::CommandError(
        "Process substitution not supported on this platform".to_string(),
    ))
}

#[cfg(not(unix))]
pub fn wait_for_all() {
    // No-op on non-Unix
}
