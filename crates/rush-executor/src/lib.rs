// Rush executor - Command execution engine

pub mod command;
pub mod terminal;

pub use command::{execute_command, ExecutionError, ExecutionResult};

// Export signal setup function
#[cfg(unix)]
pub use terminal::unix::setup_shell_signals;

#[cfg(not(unix))]
pub use terminal::non_unix::setup_shell_signals;
