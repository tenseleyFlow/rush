// Rush executor - Command execution engine

pub mod background;
pub mod command;
pub mod command_subst_exec;
pub mod control_flow;
pub mod pipeline;
pub mod process_subst;
pub mod redirect;
pub mod subshell;
pub mod terminal;
pub mod test_builtin;

pub use background::{execute_pipeline_background, execute_simple_background};
pub use command::{execute_command, execute_statement, ExecutionError, ExecutionResult};
#[cfg(unix)]
pub use command::JobControlInfo;
pub use command_subst_exec::execute_for_substitution;
pub use control_flow::{execute_case, execute_for, execute_if, execute_select, execute_while};
pub use pipeline::{execute_and_or_list, execute_pipeline, execute_simple_with_redirects, PipelineError};
pub use process_subst::{cleanup_all as cleanup_process_substs, wait_for_all as wait_for_process_substs};
pub use redirect::RedirectError;
pub use subshell::{execute_subshell, execute_subshell_background};

// Export signal setup function
#[cfg(unix)]
pub use terminal::unix::setup_shell_signals;

#[cfg(not(unix))]
pub use terminal::non_unix::setup_shell_signals;
