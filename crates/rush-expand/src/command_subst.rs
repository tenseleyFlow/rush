use thiserror::Error;
use crate::Context;

#[derive(Error, Debug)]
pub enum CommandSubstError {
    #[error("Failed to execute command: {0}")]
    ExecutionFailed(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Execute a command and capture its stdout
/// This implements $(command) substitution
pub fn execute_command_substitution(cmd: &str, context: &mut Context) -> Result<String, CommandSubstError> {
    // Use internal command executor
    // The executor is registered during Context creation in rush-cli
    // Clone the Rc to avoid borrow checker issues
    if let Some(executor) = context.command_executor.0.clone() {
        return executor(cmd, context)
            .map_err(|e| CommandSubstError::ExecutionFailed(e));
    }

    // No executor configured - this should not happen in normal rush usage
    // Rush is fully bespoke and does not delegate to external shells
    Err(CommandSubstError::ExecutionFailed(
        "Internal command executor not configured. Command substitution requires rush's internal executor.".to_string()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_command_substitution() {
        let mut context = Context::empty();
        let result = execute_command_substitution("echo hello", &mut context).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_command_with_trailing_newline() {
        let mut context = Context::empty();
        // echo adds a newline, which should be trimmed
        let result = execute_command_substitution("printf 'hello\n'", &mut context).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_multiple_trailing_newlines() {
        let mut context = Context::empty();
        let result = execute_command_substitution("printf 'hello\n\n\n'", &mut context).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_pwd() {
        let mut context = Context::empty();
        // Just make sure it doesn't error
        let result = execute_command_substitution("pwd", &mut context);
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }
}
