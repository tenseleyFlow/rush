// Rush executor - Command execution engine

pub mod background;
pub mod command;
pub mod control_flow;
pub mod pipeline;
pub mod redirect;
pub mod terminal;
pub mod test_builtin;

pub use background::{execute_pipeline_background, execute_simple_background};
pub use command::{execute_command, ExecutionError, ExecutionResult};
#[cfg(unix)]
pub use command::JobControlInfo;
pub use control_flow::{execute_case, execute_for, execute_if, execute_while};
pub use pipeline::{execute_and_or_list, execute_pipeline, execute_simple_with_redirects, PipelineError};
pub use redirect::RedirectError;

// Export signal setup function
#[cfg(unix)]
pub use terminal::unix::setup_shell_signals;

#[cfg(not(unix))]
pub use terminal::non_unix::setup_shell_signals;
