// Rush job - Job control implementation
//
// This module provides job control functionality including:
// - Process group management
// - Background/foreground job execution
// - Job suspension and continuation
// - Signal handling (SIGCHLD, SIGTSTP, SIGINT)

#![cfg(unix)]

pub mod job;
pub mod signals;
pub mod terminal;

pub use job::{Job, JobId, JobList, JobState};
pub use signals::{check_children, setup_job_control_signals};
pub use terminal::{give_terminal_to, restore_shell_terminal, setup_shell_terminal};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum JobError {
    #[error("Job not found: {0}")]
    JobNotFound(String),

    #[error("No current job")]
    NoCurrentJob,

    #[error("Job is not stopped")]
    JobNotStopped,

    #[error("System error: {0}")]
    SystemError(#[from] nix::Error),
}

pub type Result<T> = std::result::Result<T, JobError>;
