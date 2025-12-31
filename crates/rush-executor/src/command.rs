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
    context: &mut rush_expand::Context,
) -> Result<ExecutionResult, ExecutionError> {
    if command.is_empty() {
        return Err(ExecutionError::EmptyCommand);
    }

    // Check if it's a built-in command
    if let Some(result) = execute_builtin(command, args, context) {
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
pub(crate) fn execute_builtin(
    command: &str,
    args: &[String],
    context: &mut rush_expand::Context,
) -> Option<ExecutionResult> {
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
        #[cfg(unix)]
        "jobs" => Some(builtin_jobs(context)),
        #[cfg(unix)]
        "fg" => Some(builtin_fg(args, context)),
        #[cfg(unix)]
        "bg" => Some(builtin_bg(args, context)),
        #[cfg(not(unix))]
        "jobs" | "fg" | "bg" => {
            eprintln!("{}: job control not supported on this platform", command);
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

// Job control builtins (Unix only)

#[cfg(unix)]
fn builtin_jobs(context: &mut rush_expand::Context) -> ExecutionResult {
    // List all jobs in sorted order
    for job in context.job_list.jobs_sorted() {
        println!("[{}]  {}  {}", job.id, job.status_string(), job.command);
    }

    success_result()
}

#[cfg(unix)]
fn builtin_fg(args: &[String], context: &mut rush_expand::Context) -> ExecutionResult {
    use nix::sys::signal::{kill, Signal};
    use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
    use rush_job::{give_terminal_to, JobState};

    // Parse job ID argument (default to most recent job)
    let job_id = if let Some(arg) = args.first() {
        match arg.parse::<u32>() {
            Ok(id) => id,
            Err(_) => {
                eprintln!("fg: invalid job id: {}", arg);
                return error_result();
            }
        }
    } else {
        // Get most recent job
        match context.job_list.current_job() {
            Some(job) => job.id,
            None => {
                eprintln!("fg: no current job");
                return error_result();
            }
        }
    };

    // Get the job's pgid before we mutate the job
    let (pgid, command, is_stopped) = {
        let job = match context.job_list.get_job(job_id) {
            Some(job) => job,
            None => {
                eprintln!("fg: job {} not found", job_id);
                return error_result();
            }
        };
        (job.pgid, job.command.clone(), job.is_stopped())
    };

    // If the job is stopped, send SIGCONT to resume it
    if is_stopped {
        if let Err(e) = kill(pgid, Signal::SIGCONT) {
            eprintln!("fg: failed to continue job: {}", e);
            return error_result();
        }
    }

    // Give terminal control to the job
    if let Err(e) = give_terminal_to(pgid) {
        eprintln!("fg: failed to give terminal control: {}", e);
        return error_result();
    }

    // Update job state
    if let Some(job) = context.job_list.get_job_mut(job_id) {
        job.state = JobState::Running;
    }
    println!("{}", command);

    // Wait for the job to complete or stop (wait on the process group leader)
    loop {
        match waitpid(pgid, Some(WaitPidFlag::WUNTRACED)) {
            Ok(WaitStatus::Exited(_, code)) => {
                if let Some(job) = context.job_list.get_job_mut(job_id) {
                    job.state = JobState::Done(code);
                }

                // Restore terminal to shell
                if let Err(e) = rush_job::restore_shell_terminal(nix::unistd::getpgrp()) {
                    eprintln!("fg: failed to restore terminal: {}", e);
                }

                return exit_code_to_result(code);
            }
            Ok(WaitStatus::Signaled(_, sig, _)) => {
                let exit_code = 128 + sig as i32;
                if let Some(job) = context.job_list.get_job_mut(job_id) {
                    job.state = JobState::Done(exit_code);
                }

                // Restore terminal to shell
                if let Err(e) = rush_job::restore_shell_terminal(nix::unistd::getpgrp()) {
                    eprintln!("fg: failed to restore terminal: {}", e);
                }

                return exit_code_to_result(exit_code);
            }
            Ok(WaitStatus::Stopped(_, _)) => {
                if let Some(job) = context.job_list.get_job_mut(job_id) {
                    job.state = JobState::Stopped;
                }

                // Restore terminal to shell
                if let Err(e) = rush_job::restore_shell_terminal(nix::unistd::getpgrp()) {
                    eprintln!("fg: failed to restore terminal: {}", e);
                }

                return success_result();
            }
            Err(e) => {
                eprintln!("fg: wait failed: {}", e);

                // Restore terminal to shell
                if let Err(e) = rush_job::restore_shell_terminal(nix::unistd::getpgrp()) {
                    eprintln!("fg: failed to restore terminal: {}", e);
                }

                return error_result();
            }
            _ => continue,
        }
    }
}

#[cfg(unix)]
fn builtin_bg(args: &[String], context: &mut rush_expand::Context) -> ExecutionResult {
    use nix::sys::signal::{kill, Signal};
    use rush_job::JobState;

    // Parse job ID argument (default to most recent stopped job)
    let job_id = if let Some(arg) = args.first() {
        match arg.parse::<u32>() {
            Ok(id) => id,
            Err(_) => {
                eprintln!("bg: invalid job id: {}", arg);
                return error_result();
            }
        }
    } else {
        // Get most recent stopped job
        match context
            .job_list
            .jobs()
            .filter(|j| j.is_stopped())
            .max_by_key(|j| j.id)
        {
            Some(job) => job.id,
            None => {
                eprintln!("bg: no stopped job");
                return error_result();
            }
        }
    };

    // Get the job
    let job = match context.job_list.get_job_mut(job_id) {
        Some(job) => job,
        None => {
            eprintln!("bg: job {} not found", job_id);
            return error_result();
        }
    };

    // Job must be stopped
    if !job.is_stopped() {
        eprintln!("bg: job {} is not stopped", job_id);
        return error_result();
    }

    // Send SIGCONT to resume the job in the background
    let pgid = job.pgid;
    if let Err(e) = kill(pgid, Signal::SIGCONT) {
        eprintln!("bg: failed to continue job: {}", e);
        return error_result();
    }

    // Update job state
    job.state = JobState::Running;
    println!("[{}]  {}", job.id, job.command);

    success_result()
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
        let mut context = rush_expand::Context::empty();
        let result = execute_command("nonexistent_command_12345", &[], false, &mut context);
        assert!(result.is_err());
    }
}
