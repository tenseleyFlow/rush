use nix::sys::signal::{signal, SigHandler, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;

/// Setup job control signal handlers
///
/// This configures the shell to handle:
/// - SIGCHLD: Reap completed/stopped child processes
/// - SIGINT: Interrupt (Ctrl-C) - handled by foreground job
/// - SIGTSTP: Suspend (Ctrl-Z) - handled by foreground job
///
/// Note: The actual signal handling is done via polling in the main loop,
/// not via signal handlers (to avoid async-signal-safety issues).
pub fn setup_job_control_signals() -> Result<(), nix::Error> {
    // Set SIGCHLD to default (we'll poll for child status changes)
    unsafe {
        signal(Signal::SIGCHLD, SigHandler::SigDfl)?;
    }

    // SIGINT and SIGTSTP are handled by the foreground job
    // The shell ignores them (they're set up in terminal::setup_shell_terminal)

    Ok(())
}

/// Check for completed or stopped child processes
///
/// This should be called periodically (e.g., before displaying a prompt)
/// to reap zombie processes and update job states.
///
/// Returns a list of (pid, status) pairs for processes that changed state.
pub fn check_children() -> Vec<(Pid, WaitStatus)> {
    let mut statuses = Vec::new();

    loop {
        // Use WNOHANG to avoid blocking, and WUNTRACED to catch stopped processes
        let flags = WaitPidFlag::WNOHANG | WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED;

        match waitpid(Pid::from_raw(-1), Some(flags)) {
            Ok(WaitStatus::StillAlive) => break, // No more children have changed state
            Ok(status) => {
                // Extract PID from status
                if let Some(pid) = status.pid() {
                    statuses.push((pid, status));
                }
            }
            Err(_) => break, // No children or error
        }
    }

    statuses
}

/// Wait for a specific process to change state
///
/// This blocks until the process exits, is stopped, or is continued.
/// Used when waiting for a foreground job to complete.
pub fn wait_for_process(pid: Pid) -> Result<WaitStatus, nix::Error> {
    let flags = WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED;
    waitpid(pid, Some(flags))
}

/// Wait for any process in a process group
///
/// This blocks until any process in the group changes state.
pub fn wait_for_process_group(pgid: Pid) -> Result<WaitStatus, nix::Error> {
    let flags = WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED;
    // Negative PID means wait for process group
    waitpid(Pid::from_raw(-pgid.as_raw()), Some(flags))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_signals() {
        // Just verify it doesn't error
        let result = setup_job_control_signals();
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_children_no_block() {
        // This should return immediately even if there are no children
        let statuses = check_children();
        // We can't assert much about the result since it depends on system state
        // But we can verify it doesn't panic
        let _ = statuses;
    }
}
