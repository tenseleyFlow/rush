use nix::unistd::{tcgetpgrp, tcsetpgrp, Pid};
use std::io;

/// Give terminal control to a process group
///
/// This allows the process group to read from and write to the terminal.
/// The shell should call this when putting a job in the foreground.
pub fn give_terminal_to(pgid: Pid) -> Result<(), nix::Error> {
    let stdin = io::stdin();
    tcsetpgrp(&stdin, pgid)
}

/// Get the current foreground process group of the terminal
pub fn get_terminal_pgid() -> Result<Pid, nix::Error> {
    let stdin = io::stdin();
    tcgetpgrp(&stdin)
}

/// Restore terminal control to the shell
///
/// The shell should call this when a foreground job completes or is suspended.
pub fn restore_shell_terminal(shell_pgid: Pid) -> Result<(), nix::Error> {
    let stdin = io::stdin();
    tcsetpgrp(&stdin, shell_pgid)
}

/// Initialize shell for job control
///
/// This should be called once at shell startup to:
/// 1. Put the shell in its own process group
/// 2. Take control of the terminal
/// 3. Ignore job control signals
pub fn setup_shell_terminal() -> Result<Pid, nix::Error> {
    use nix::sys::signal::{signal, SigHandler, Signal};
    use nix::unistd::{getpid, setpgid};

    // Put the shell in its own process group
    let shell_pid = getpid();
    setpgid(shell_pid, shell_pid)?;

    // Take control of the terminal
    let stdin = io::stdin();
    tcsetpgrp(&stdin, shell_pid)?;

    // Ignore job control signals (shell should handle them explicitly)
    unsafe {
        signal(Signal::SIGTTOU, SigHandler::SigIgn)?;
        signal(Signal::SIGTTIN, SigHandler::SigIgn)?;
        signal(Signal::SIGTSTP, SigHandler::SigIgn)?;
    }

    Ok(shell_pid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::IsTerminal;

    #[test]
    fn test_get_terminal_pgid() {
        // Skip if stdin is not a terminal (e.g., in CI)
        if !std::io::stdin().is_terminal() {
            return;
        }

        // This test just verifies the function works without error
        // The actual pgid will vary
        let result = get_terminal_pgid();
        assert!(result.is_ok());
    }
}
