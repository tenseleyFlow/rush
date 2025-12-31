use std::env;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};
use thiserror::Error;

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

#[derive(Error, Debug)]
pub enum ExecutionError {
    #[error("Command not found: {0}")]
    CommandNotFound(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Empty command")]
    EmptyCommand,
}

pub struct ExecutionResult {
    pub exit_status: ExitStatus,
}

impl ExecutionResult {
    pub fn success_code() -> i32 {
        0
    }

    pub fn exit_code(&self) -> i32 {
        self.exit_status.code().unwrap_or(1)
    }

    pub fn success(&self) -> bool {
        self.exit_status.success()
    }
}

/// Execute a simple command (external program)
///
/// In interactive mode, this sets up proper process groups and terminal control.
/// In non-interactive mode, it runs the command normally.
pub fn execute_command(
    command: &str,
    args: &[String],
    interactive: bool,
) -> Result<ExecutionResult, ExecutionError> {
    if command.is_empty() {
        return Err(ExecutionError::EmptyCommand);
    }

    // Check if it's a built-in command
    if let Some(result) = execute_builtin(command, args) {
        return Ok(result);
    }

    // Try to find the command in PATH
    let program_path = find_in_path(command)
        .ok_or_else(|| ExecutionError::CommandNotFound(command.to_string()))?;

    // Build the command
    let mut cmd = Command::new(program_path);
    cmd.args(args);

    // Execute with proper terminal handling
    #[cfg(unix)]
    {
        crate::terminal::unix::execute_with_terminal_control(cmd, interactive)
    }

    #[cfg(not(unix))]
    {
        crate::terminal::non_unix::execute_with_terminal_control(cmd, interactive)
    }
}

/// Execute built-in commands
pub(crate) fn execute_builtin(command: &str, args: &[String]) -> Option<ExecutionResult> {
    match command {
        "exit" => {
            let code = args.first()
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(0);
            std::process::exit(code);
        }
        "cd" => {
            let default_home = env::var("HOME").unwrap_or_else(|_| "/".to_string());
            let dir = args.first()
                .map(|s| s.as_str())
                .unwrap_or(&default_home);

            match env::set_current_dir(dir) {
                Ok(_) => Some(success_result()),
                Err(_) => Some(error_result()),
            }
        }
        "pwd" => {
            match env::current_dir() {
                Ok(path) => {
                    println!("{}", path.display());
                    Some(success_result())
                }
                Err(_) => Some(error_result()),
            }
        }
        "test" | "[" => {
            let exit_code = crate::test_builtin::execute_test(args);
            Some(exit_code_to_result(exit_code))
        }
        "jobs" => {
            // TODO: Implement jobs builtin - list all jobs
            // Will need access to JobList from shell context
            eprintln!("jobs: not yet implemented");
            Some(success_result())
        }
        "fg" => {
            // TODO: Implement fg builtin - bring job to foreground
            // Usage: fg [job_id]
            // Will need access to JobList and terminal control
            eprintln!("fg: not yet implemented");
            Some(error_result())
        }
        "bg" => {
            // TODO: Implement bg builtin - continue job in background
            // Usage: bg [job_id]
            // Will need access to JobList
            eprintln!("bg: not yet implemented");
            Some(error_result())
        }
        _ => None,
    }
}

fn exit_code_to_result(code: i32) -> ExecutionResult {
    #[cfg(unix)]
    {
        ExecutionResult {
            exit_status: std::process::ExitStatus::from_raw(code << 8),
        }
    }

    #[cfg(not(unix))]
    {
        // On non-Unix, we can't easily create an ExitStatus with a specific code
        if code == 0 {
            success_result()
        } else {
            error_result()
        }
    }
}

#[cfg(unix)]
pub(crate) fn success_result() -> ExecutionResult {
    ExecutionResult {
        exit_status: std::process::ExitStatus::from_raw(0),
    }
}

#[cfg(unix)]
fn error_result() -> ExecutionResult {
    ExecutionResult {
        exit_status: std::process::ExitStatus::from_raw(1 << 8),
    }
}

#[cfg(not(unix))]
pub(crate) fn success_result() -> ExecutionResult {
    ExecutionResult {
        exit_status: std::process::ExitStatus::default(),
    }
}

#[cfg(not(unix))]
fn error_result() -> ExecutionResult {
    // On non-Unix, we can't easily create a failed ExitStatus
    // This is a limitation for now
    ExecutionResult {
        exit_status: std::process::ExitStatus::default(),
    }
}

/// Find a command in PATH
pub(crate) fn find_in_path(command: &str) -> Option<PathBuf> {
    // If the command contains a slash, treat it as a path
    if command.contains('/') {
        let path = PathBuf::from(command);
        if path.exists() && is_executable(&path) {
            return Some(path);
        }
        return None;
    }

    // Search in PATH
    let path_var = env::var_os("PATH")?;
    env::split_paths(&path_var)
        .map(|dir| dir.join(command))
        .find(|path| path.exists() && is_executable(path))
}

/// Check if a file is executable
#[cfg(unix)]
fn is_executable(path: &PathBuf) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(_path: &PathBuf) -> bool {
    // On non-Unix systems, assume existence is enough
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_in_path() {
        // ls should exist on most Unix systems
        let result = find_in_path("ls");
        assert!(result.is_some());
    }

    #[test]
    fn test_command_not_found() {
        let result = execute_command("nonexistent_command_12345", &[]);
        assert!(result.is_err());
    }
}
