//! Internal command substitution execution
//!
//! This module provides the ability to execute commands internally and capture their stdout,
//! enabling $(command) substitution without relying on external shells.

use rush_expand::Context;
use std::io::Write;

/// Execute a command string internally and capture its stdout
///
/// This is the callback function that gets registered with Context.command_executor
/// to enable internal command substitution.
///
/// The function:
/// 1. Parses the command string using rush-parser
/// 2. Captures stdout to a buffer
/// 3. Executes the command using rush-executor
/// 4. Returns the captured output
pub fn execute_for_substitution(cmd: &str, context: &mut Context) -> Result<String, String> {
    // Parse the command
    let statement = rush_parser::parse_line(cmd)
        .map_err(|e| format!("Parse error: {}", e))?;

    // Use a pipe to capture stdout
    #[cfg(unix)]
    {
        execute_with_capture_unix(statement, context)
    }

    #[cfg(not(unix))]
    {
        // On non-Unix platforms, fall back to sh -c for now
        // TODO: Implement Windows-specific stdout capture
        Err("Command substitution on non-Unix platforms not yet supported".to_string())
    }
}

#[cfg(unix)]
fn execute_with_capture_unix(
    statement: rush_parser::Statement,
    context: &mut Context,
) -> Result<String, String> {
    use nix::libc;
    use nix::unistd::{pipe, dup, dup2};
    use std::os::unix::io::{RawFd, FromRawFd, IntoRawFd};
    use std::io::Read;

    const STDOUT_FD: RawFd = 1;

    // Create a pipe for capturing output
    let (read_fd, write_fd) = pipe().map_err(|e| format!("Failed to create pipe: {}", e))?;

    // Get raw fds and take ownership to prevent automatic close
    let read_raw_fd = read_fd.into_raw_fd();
    let write_raw_fd = write_fd.into_raw_fd();

    // Save the original stdout
    let saved_stdout = dup(STDOUT_FD).map_err(|e| format!("Failed to dup stdout: {}", e))?;
    let saved_stdout_raw = saved_stdout.into_raw_fd();

    // Redirect stdout to the write end of the pipe
    dup2(write_raw_fd, STDOUT_FD).map_err(|e| format!("Failed to redirect stdout: {}", e))?;

    // Close the write_fd as we've duplicated it to stdout
    // (write_fd is now owned by stdout, so we close the original)
    unsafe { libc::close(write_raw_fd); }

    // Execute the command
    let exec_result = match statement {
        rush_parser::Statement::Complete(cmd) => {
            crate::control_flow::execute_complete_command(&cmd, context)
        }
        rush_parser::Statement::Script(commands) => {
            let mut last_result = crate::command::success_result();
            for cmd in &commands {
                match crate::control_flow::execute_complete_command(cmd, context) {
                    Ok(result) => last_result = result,
                    Err(e) => {
                        // Restore stdout before returning error
                        let _ = dup2(saved_stdout_raw, STDOUT_FD);
                        unsafe {
                            libc::close(saved_stdout_raw);
                            libc::close(read_raw_fd);
                        }
                        return Err(format!("Execution error: {}", e));
                    }
                }
            }
            Ok(last_result)
        }
        rush_parser::Statement::Empty => Ok(crate::command::success_result()),
    };

    // Flush stdout to ensure all data is written to the pipe
    let _ = std::io::stdout().flush();

    // Restore the original stdout
    dup2(saved_stdout_raw, STDOUT_FD).map_err(|e| format!("Failed to restore stdout: {}", e))?;
    unsafe { libc::close(saved_stdout_raw); }

    // Check execution result
    exec_result.map_err(|e| format!("Execution error: {}", e))?;

    // Read the captured output from the pipe
    let mut output = String::new();
    let mut reader = unsafe { std::fs::File::from_raw_fd(read_raw_fd) };
    reader.read_to_string(&mut output).map_err(|e| format!("Failed to read output: {}", e))?;

    // Bash behavior: trim trailing newlines from command substitution
    while output.ends_with('\n') {
        output.pop();
    }

    Ok(output)
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
