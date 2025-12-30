use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CommandSubstError {
    #[error("Failed to execute command: {0}")]
    ExecutionFailed(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Execute a command and capture its stdout
/// This implements $(command) substitution
pub fn execute_command_substitution(cmd: &str) -> Result<String, CommandSubstError> {
    // For now, use sh -c to execute the command
    // TODO: In the future, we should parse and execute internally for full rush compatibility
    let output = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .output()
        .map_err(|e| CommandSubstError::IoError(e))?;

    if !output.status.success() {
        return Err(CommandSubstError::ExecutionFailed(format!(
            "Command '{}' failed with status: {}",
            cmd,
            output.status.code().unwrap_or(-1)
        )));
    }

    // Convert stdout to string
    let mut result = String::from_utf8_lossy(&output.stdout).to_string();

    // Bash behavior: trim trailing newlines from command substitution
    while result.ends_with('\n') {
        result.pop();
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_command_substitution() {
        let result = execute_command_substitution("echo hello").unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_command_with_trailing_newline() {
        // echo adds a newline, which should be trimmed
        let result = execute_command_substitution("printf 'hello\n'").unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_multiple_trailing_newlines() {
        let result = execute_command_substitution("printf 'hello\n\n\n'").unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_pwd() {
        // Just make sure it doesn't error
        let result = execute_command_substitution("pwd");
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }
}
