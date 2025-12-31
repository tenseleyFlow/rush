// Terminal-aware command execution with proper signal handling

#[cfg(unix)]
pub mod unix {
    use nix::sys::signal::{self, SaFlags, SigAction, SigHandler, SigSet, Signal};
    use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
    use nix::unistd::{setpgid, tcsetpgrp, Pid};
    use std::io;
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    use std::process::Command;

    use super::super::{ExecutionError, ExecutionResult, JobControlInfo};

    /// Initialize signal handling for the shell
    /// The shell should ignore SIGINT and SIGTSTP so that Ctrl+C and Ctrl+Z
    /// only affect the foreground process, not the shell itself
    pub fn setup_shell_signals() -> Result<(), ExecutionError> {
        unsafe {
            // Ignore SIGINT (Ctrl+C)
            let handler = SigHandler::SigIgn;
            let sig_action = SigAction::new(handler, SaFlags::empty(), SigSet::empty());
            signal::sigaction(Signal::SIGINT, &sig_action)
                .map_err(|e| ExecutionError::IoError(io::Error::new(io::ErrorKind::Other, e)))?;

            // Ignore SIGTSTP (Ctrl+Z)
            signal::sigaction(Signal::SIGTSTP, &sig_action)
                .map_err(|e| ExecutionError::IoError(io::Error::new(io::ErrorKind::Other, e)))?;

            // Ignore SIGTTOU (background writes to terminal)
            signal::sigaction(Signal::SIGTTOU, &sig_action)
                .map_err(|e| ExecutionError::IoError(io::Error::new(io::ErrorKind::Other, e)))?;
        }
        Ok(())
    }

    /// Execute a command with proper terminal and signal handling
    pub fn execute_with_terminal_control(
        mut command: Command,
        interactive: bool,
    ) -> Result<ExecutionResult, ExecutionError> {
        if !interactive {
            // Non-interactive mode: just run the command normally
            let status = command.status()?;
            return Ok(ExecutionResult {
                exit_status: status,
                job_control: None,
            });
        }

        // Interactive mode: set up process groups and terminal control
        unsafe {
            command.pre_exec(|| {
                // Child process: restore default signal handlers
                let handler = SigHandler::SigDfl;
                let sig_action = SigAction::new(handler, SaFlags::empty(), SigSet::empty());

                signal::sigaction(Signal::SIGINT, &sig_action)?;
                signal::sigaction(Signal::SIGTSTP, &sig_action)?;
                signal::sigaction(Signal::SIGTTOU, &sig_action)?;

                // Put the child in its own process group
                setpgid(Pid::from_raw(0), Pid::from_raw(0))?;

                Ok(())
            });
        }

        // Spawn the child process
        let child = command.spawn()?;
        let child_pid = Pid::from_raw(child.id() as i32);

        // Set the child's process group (belt and suspenders)
        let _ = setpgid(child_pid, child_pid);

        // Give the child's process group control of the terminal
        let stdin = std::io::stdin();
        let _ = tcsetpgrp(&stdin, child_pid);

        // Wait for the child to complete
        let (status, job_control) = loop {
            match waitpid(child_pid, Some(WaitPidFlag::WUNTRACED)) {
                Ok(WaitStatus::Exited(_, code)) => {
                    break (std::process::ExitStatus::from_raw(code << 8), None);
                }
                Ok(WaitStatus::Signaled(_, signal, _)) => {
                    // Child was killed by a signal
                    break (std::process::ExitStatus::from_raw(signal as i32 + 128), None);
                }
                Ok(WaitStatus::Stopped(_, _)) => {
                    // Child was stopped (Ctrl+Z)
                    // Return job control info so the shell can add it to the job list
                    let job_control = Some(JobControlInfo {
                        pid: child_pid,
                        pgid: child_pid, // Child is its own process group leader
                        stopped: true,
                    });
                    break (std::process::ExitStatus::from_raw(148), job_control); // SIGTSTP + 128
                }
                Ok(WaitStatus::Continued(_)) => {
                    // Child continued, keep waiting
                    continue;
                }
                Ok(_) => {
                    // Other status, keep waiting
                    continue;
                }
                Err(nix::errno::Errno::EINTR) => {
                    // Interrupted by signal, try again
                    continue;
                }
                Err(e) => {
                    return Err(ExecutionError::IoError(io::Error::new(
                        io::ErrorKind::Other,
                        e,
                    )));
                }
            }
        };

        // Take back terminal control
        let stdin = std::io::stdin();
        let shell_pgid = Pid::from_raw(std::process::id() as i32);
        let _ = tcsetpgrp(&stdin, shell_pgid);

        Ok(ExecutionResult {
            exit_status: status,
            job_control,
        })
    }
}

#[cfg(not(unix))]
pub mod non_unix {
    use std::process::Command;

    use super::super::{ExecutionError, ExecutionResult};

    pub fn setup_shell_signals() -> Result<(), ExecutionError> {
        // No-op on non-Unix systems
        Ok(())
    }

    pub fn execute_with_terminal_control(
        mut command: Command,
        _interactive: bool,
    ) -> Result<ExecutionResult, ExecutionError> {
        let status = command.status()?;
        Ok(ExecutionResult {
            exit_status: status,
        })
    }
}
