//! Subshell execution
//!
//! Subshells execute commands in a subprocess, providing isolation:
//! - Variable changes don't affect the parent shell
//! - Directory changes (cd) don't affect the parent
//! - `exit` terminates the subshell, not the parent

use rush_expand::Context;
use rush_parser::ast::Subshell;
use crate::command::ExecutionResult;

/// Execute commands in a subshell (subprocess)
///
/// Syntax: (command; command; ...)
///
/// The commands run in a forked child process, so any state changes
/// (variables, working directory, etc.) are isolated from the parent shell.
#[cfg(unix)]
pub fn execute_subshell(
    subshell: &Subshell,
    context: &mut Context,
) -> Result<ExecutionResult, String> {
    use nix::sys::wait::{waitpid, WaitStatus};
    use nix::unistd::{fork, ForkResult};
    use std::process;

    // Fork the process
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            // Child process: execute the commands
            let exit_code = execute_subshell_child(&subshell.commands, context);
            process::exit(exit_code);
        }
        Ok(ForkResult::Parent { child }) => {
            // Parent process: wait for child to complete
            match waitpid(child, None) {
                Ok(WaitStatus::Exited(_, exit_code)) => {
                    // Child exited normally
                    Ok(crate::command::exit_code_to_result(exit_code))
                }
                Ok(WaitStatus::Signaled(_, signal, _)) => {
                    // Child was killed by a signal
                    // Return 128 + signal number (bash convention)
                    Ok(crate::command::exit_code_to_result(128 + signal as i32))
                }
                Ok(_) => {
                    // Other wait status (stopped, continued, etc.)
                    Ok(crate::command::exit_code_to_result(1))
                }
                Err(e) => {
                    Err(format!("Failed to wait for subshell: {}", e))
                }
            }
        }
        Err(e) => {
            Err(format!("Failed to fork for subshell: {}", e))
        }
    }
}

/// Execute commands in the child process
/// Returns the exit code to use for process::exit()
#[cfg(unix)]
pub(crate) fn execute_subshell_child(
    commands: &[rush_parser::CompleteCommand],
    context: &mut Context,
) -> i32 {
    let mut last_exit_code = 0;

    for cmd in commands {
        match crate::control_flow::execute_complete_command(cmd, context) {
            Ok(result) => {
                last_exit_code = result.exit_code();
                context.set_exit_status(last_exit_code);
            }
            Err(e) => {
                eprintln!("rush: {}", e);
                return 1;
            }
        }
    }

    last_exit_code
}

/// Execute a subshell in the background
/// Returns (pid, pgid, command_string)
#[cfg(unix)]
pub fn execute_subshell_background(
    subshell: &Subshell,
    context: &mut Context,
) -> Result<(nix::unistd::Pid, nix::unistd::Pid, String), String> {
    use nix::unistd::{fork, ForkResult, setpgid, Pid};

    // Build a command string for display
    let command_string = format!("(subshell with {} commands)", subshell.commands.len());

    // Fork the process
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            // Child process: create new process group and execute
            let pid = std::process::id() as i32;
            let _ = setpgid(Pid::from_raw(pid), Pid::from_raw(pid));

            let exit_code = execute_subshell_child(&subshell.commands, context);
            std::process::exit(exit_code);
        }
        Ok(ForkResult::Parent { child }) => {
            // Create new process group for the child
            let _ = setpgid(child, child);

            // Don't wait - that's what makes it background
            Ok((child, child, command_string))
        }
        Err(e) => {
            Err(format!("Failed to fork for background subshell: {}", e))
        }
    }
}

/// Fallback for non-Unix platforms
#[cfg(not(unix))]
pub fn execute_subshell(
    _subshell: &Subshell,
    _context: &mut Context,
) -> Result<ExecutionResult, String> {
    Err("Subshells are not supported on this platform".to_string())
}

#[cfg(not(unix))]
pub fn execute_subshell_background(
    _subshell: &Subshell,
    _context: &mut Context,
) -> Result<(i32, i32, String), String> {
    Err("Background subshells are not supported on this platform".to_string())
}
