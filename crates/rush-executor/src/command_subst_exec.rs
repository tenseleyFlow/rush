//! Internal command substitution execution
//!
//! This module provides the ability to execute commands internally and capture their stdout,
//! enabling $(command) substitution without relying on external shells.

use rush_expand::Context;

/// Execute a command string internally and capture its stdout
///
/// This is the callback function that gets registered with Context.command_executor
/// to enable internal command substitution.
///
/// The function:
/// 1. Parses the command string using rush-parser
/// 2. Forks a child process
/// 3. The child executes the command with stdout redirected to a pipe
/// 4. The parent reads and returns the captured output
pub fn execute_for_substitution(cmd: &str, context: &mut Context) -> Result<String, String> {
    // Parse the command
    let statement = rush_parser::parse_line(cmd)
        .map_err(|e| format!("Parse error: {}", e))?;

    // Use fork + pipe to capture stdout (like bash does)
    #[cfg(unix)]
    {
        execute_with_fork_capture(statement, context)
    }

    #[cfg(not(unix))]
    {
        // On non-Unix platforms, fall back to sh -c for now
        // TODO: Implement Windows-specific stdout capture
        Err("Command substitution on non-Unix platforms not yet supported".to_string())
    }
}

#[cfg(unix)]
fn execute_with_fork_capture(
    statement: rush_parser::Statement,
    context: &mut Context,
) -> Result<String, String> {
    use nix::libc;
    use nix::unistd::{pipe, dup2, fork, ForkResult};
    use nix::sys::wait::{waitpid, WaitStatus};
    use std::os::unix::io::{RawFd, FromRawFd, IntoRawFd};
    use std::io::{Read, Write};

    const STDOUT_FD: RawFd = 1;

    // Create a pipe for capturing output
    let (read_fd, write_fd) = pipe().map_err(|e| format!("Failed to create pipe: {}", e))?;
    let read_raw_fd = read_fd.into_raw_fd();
    let write_raw_fd = write_fd.into_raw_fd();

    // Fork to execute the command in a subprocess
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            // Child process: redirect stdout to pipe and execute

            // Close the read end of the pipe (child only writes)
            unsafe { libc::close(read_raw_fd); }

            // Redirect stdout to the write end of the pipe
            if let Err(_) = dup2(write_raw_fd, STDOUT_FD) {
                std::process::exit(1);
            }

            // Close the original write_fd (now that stdout points to it)
            unsafe { libc::close(write_raw_fd); }

            // Execute the command
            let exit_code = match statement {
                rush_parser::Statement::Complete(cmd) => {
                    match crate::control_flow::execute_complete_command(&cmd, context) {
                        Ok(result) => result.exit_code(),
                        Err(_) => 1,
                    }
                }
                rush_parser::Statement::Script(commands) => {
                    let mut last_code = 0;
                    for cmd in &commands {
                        match crate::control_flow::execute_complete_command(cmd, context) {
                            Ok(result) => last_code = result.exit_code(),
                            Err(_) => {
                                last_code = 1;
                                break;
                            }
                        }
                    }
                    last_code
                }
                rush_parser::Statement::Empty => 0,
            };

            // Ensure all output is flushed before exiting
            let _ = std::io::stdout().flush();

            // Exit the child process
            std::process::exit(exit_code);
        }
        Ok(ForkResult::Parent { child }) => {
            // Parent process: close write end and read from pipe

            // Close the write end (parent only reads)
            unsafe { libc::close(write_raw_fd); }

            // Read all output from the pipe
            let mut output = String::new();
            let mut reader = unsafe { std::fs::File::from_raw_fd(read_raw_fd) };
            if let Err(e) = reader.read_to_string(&mut output) {
                return Err(format!("Failed to read output: {}", e));
            }

            // Wait for child to complete
            match waitpid(child, None) {
                Ok(WaitStatus::Exited(_, code)) => {
                    context.set_exit_status(code);
                }
                Ok(_) => {
                    context.set_exit_status(0);
                }
                Err(e) => {
                    return Err(format!("Failed to wait for child: {}", e));
                }
            }

            // Bash behavior: trim trailing newlines from command substitution
            while output.ends_with('\n') {
                output.pop();
            }

            Ok(output)
        }
        Err(e) => {
            // Fork failed, clean up
            unsafe {
                libc::close(read_raw_fd);
                libc::close(write_raw_fd);
            }
            Err(format!("Failed to fork: {}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn test_internal_echo() {
        let mut context = Context::empty();
        let result = execute_for_substitution("echo hello", &mut context).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    #[cfg(unix)]
    fn test_internal_with_variables() {
        let mut context = Context::empty();
        context.set_var("x", "world").unwrap();
        let result = execute_for_substitution("echo hello $x", &mut context).unwrap();
        assert_eq!(result, "hello world");
    }
}
