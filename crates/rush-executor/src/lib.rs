// Rush executor - Command execution engine

pub mod command;
pub mod pipeline;
pub mod redirect;
pub mod terminal;

pub use command::{execute_command, ExecutionError, ExecutionResult};
pub use pipeline::{execute_pipeline, execute_simple_with_redirects, PipelineError};
pub use redirect::RedirectError;

// Export signal setup function
#[cfg(unix)]
pub use terminal::unix::setup_shell_signals;

#[cfg(not(unix))]
pub use terminal::non_unix::setup_shell_signals;
