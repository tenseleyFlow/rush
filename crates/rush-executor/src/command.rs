use std::env;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};
use thiserror::Error;
use rush_interactive::ErrorHints;

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

#[derive(Error, Debug)]
pub enum ExecutionError {
    #[error("{0}")]
    CommandNotFound(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Empty command")]
    EmptyCommand,
}

pub struct ExecutionResult {
    pub exit_status: ExitStatus,
    /// Job control information (Unix only)
    #[cfg(unix)]
    pub job_control: Option<JobControlInfo>,
}

#[cfg(unix)]
#[derive(Debug, Clone)]
pub struct JobControlInfo {
    /// Process ID
    pub pid: nix::unistd::Pid,
    /// Process group ID
    pub pgid: nix::unistd::Pid,
    /// Whether the job was stopped (Ctrl-Z)
    pub stopped: bool,
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

    #[cfg(unix)]
    pub fn is_stopped(&self) -> bool {
        self.job_control.as_ref().map_or(false, |jc| jc.stopped)
    }

    #[cfg(not(unix))]
    pub fn is_stopped(&self) -> bool {
        false
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

    // Expand aliases (only for the command name, not args)
    let (actual_command, actual_args): (String, Vec<String>) = if let Some(alias_value) = context.aliases.get(command).cloned() {
        // Parse the alias value to get command and its args
        let parts: Vec<String> = alias_value.split_whitespace().map(|s| s.to_string()).collect();
        if parts.is_empty() {
            (command.to_string(), args.to_vec())
        } else {
            let cmd = parts[0].clone();
            let mut new_args: Vec<String> = parts[1..].to_vec();
            new_args.extend_from_slice(args);
            (cmd, new_args)
        }
    } else {
        (command.to_string(), args.to_vec())
    };

    // Check if it's a built-in command
    if let Some(result) = execute_builtin(&actual_command, &actual_args, context) {
        return Ok(result);
    }

    // Try to find the command in PATH
    let program_path = find_in_path(&actual_command)
        .ok_or_else(|| ExecutionError::CommandNotFound(ErrorHints::command_not_found(&actual_command)))?;

    // Build the command
    let mut cmd = Command::new(program_path);
    cmd.args(&actual_args);

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
        "source" | "." => {
            match builtin_source(args, context) {
                Ok(result) => Some(result),
                Err(err) => {
                    eprintln!("{}: {}", command, err);
                    Some(error_result())
                }
            }
        }
        "eval" => {
            match builtin_eval(args, context) {
                Ok(result) => Some(result),
                Err(err) => {
                    eprintln!("eval: {}", err);
                    Some(error_result())
                }
            }
        }
        "alias" => Some(builtin_alias(args, context)),
        "unalias" => Some(builtin_unalias(args, context)),
        "trap" => Some(builtin_trap(args, context)),
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
            job_control: None,
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

/// alias builtin - Manage command aliases
fn builtin_alias(args: &[String], context: &mut rush_expand::Context) -> ExecutionResult {
    // No arguments: list all aliases
    if args.is_empty() {
        let mut aliases: Vec<_> = context.aliases.iter().collect();
        aliases.sort_by_key(|(name, _)| *name);
        for (name, value) in aliases {
            println!("alias {}='{}'", name, value);
        }
        return success_result();
    }

    // Process each argument
    for arg in args {
        if let Some(eq_pos) = arg.find('=') {
            // Define alias: name=value
            let name = &arg[..eq_pos];
            let value = &arg[eq_pos + 1..];
            context.aliases.insert(name.to_string(), value.to_string());
        } else {
            // Display specific alias
            match context.aliases.get(arg) {
                Some(value) => println!("alias {}='{}'", arg, value),
                None => {
                    eprintln!("alias: {}: not found", arg);
                    return error_result();
                }
            }
        }
    }

    success_result()
}

/// unalias builtin - Remove command aliases
fn builtin_unalias(args: &[String], context: &mut rush_expand::Context) -> ExecutionResult {
    if args.is_empty() {
        eprintln!("unalias: usage: unalias [-a] name [name ...]");
        return error_result();
    }

    // Check for -a flag (remove all aliases)
    if args[0] == "-a" {
        context.aliases.clear();
        return success_result();
    }

    // Remove specified aliases
    let mut had_error = false;
    for name in args {
        if context.aliases.remove(name).is_none() {
            eprintln!("unalias: {}: not found", name);
            had_error = true;
        }
    }

    if had_error {
        error_result()
    } else {
        success_result()
    }
}

/// Normalize signal name (SIGINT, INT, 2 all -> INT)
fn normalize_signal_name(sig: &str) -> Option<String> {
    // Special trap signals
    match sig.to_uppercase().as_str() {
        "EXIT" | "0" => return Some("EXIT".to_string()),
        "ERR" => return Some("ERR".to_string()),
        "DEBUG" => return Some("DEBUG".to_string()),
        "RETURN" => return Some("RETURN".to_string()),
        _ => {}
    }

    // Regular signals - strip SIG prefix if present
    let name = sig.to_uppercase();
    let name = name.strip_prefix("SIG").unwrap_or(&name);

    // Map common signal names/numbers
    match name {
        "HUP" | "1" => Some("HUP".to_string()),
        "INT" | "2" => Some("INT".to_string()),
        "QUIT" | "3" => Some("QUIT".to_string()),
        "ABRT" | "6" => Some("ABRT".to_string()),
        "KILL" | "9" => Some("KILL".to_string()),
        "ALRM" | "14" => Some("ALRM".to_string()),
        "TERM" | "15" => Some("TERM".to_string()),
        "USR1" | "10" => Some("USR1".to_string()),
        "USR2" | "12" => Some("USR2".to_string()),
        "CHLD" | "CHILD" | "17" => Some("CHLD".to_string()),
        "CONT" | "18" => Some("CONT".to_string()),
        "STOP" | "19" => Some("STOP".to_string()),
        "TSTP" | "20" => Some("TSTP".to_string()),
        "TTIN" | "21" => Some("TTIN".to_string()),
        "TTOU" | "22" => Some("TTOU".to_string()),
        _ => None,
    }
}

/// trap builtin - Set or display signal handlers
fn builtin_trap(args: &[String], context: &mut rush_expand::Context) -> ExecutionResult {
    // No arguments: list all traps
    if args.is_empty() {
        let mut traps: Vec<_> = context.traps.iter().collect();
        traps.sort_by_key(|(sig, _)| *sig);
        for (signal, command) in traps {
            if command.is_empty() {
                println!("trap -- '' {}", signal);
            } else {
                println!("trap -- '{}' {}", command, signal);
            }
        }
        return success_result();
    }

    // Handle -p flag (print traps)
    if args[0] == "-p" {
        if args.len() == 1 {
            // Print all traps
            let mut traps: Vec<_> = context.traps.iter().collect();
            traps.sort_by_key(|(sig, _)| *sig);
            for (signal, command) in traps {
                if command.is_empty() {
                    println!("trap -- '' {}", signal);
                } else {
                    println!("trap -- '{}' {}", command, signal);
                }
            }
        } else {
            // Print specific traps
            for sig in &args[1..] {
                if let Some(normalized) = normalize_signal_name(sig) {
                    if let Some(command) = context.traps.get(&normalized) {
                        if command.is_empty() {
                            println!("trap -- '' {}", normalized);
                        } else {
                            println!("trap -- '{}' {}", command, normalized);
                        }
                    }
                }
            }
        }
        return success_result();
    }

    // Handle -l flag (list signal names)
    if args[0] == "-l" {
        println!(" 1) HUP\t 2) INT\t 3) QUIT\t 6) ABRT\t 9) KILL");
        println!("10) USR1\t12) USR2\t14) ALRM\t15) TERM\t17) CHLD");
        println!("18) CONT\t19) STOP\t20) TSTP\t21) TTIN\t22) TTOU");
        return success_result();
    }

    // trap COMMAND SIGNAL...
    let command = &args[0];
    let signals = &args[1..];

    if signals.is_empty() {
        eprintln!("trap: usage: trap [-lp] [[arg] signal_spec ...]");
        return error_result();
    }

    // Check if we're clearing traps (trap - SIGNAL)
    let clearing = command == "-";

    let mut had_error = false;
    for sig in signals {
        if let Some(normalized) = normalize_signal_name(sig) {
            if clearing {
                context.traps.remove(&normalized);
            } else {
                context.traps.insert(normalized, command.clone());
            }
        } else {
            eprintln!("trap: {}: invalid signal specification", sig);
            had_error = true;
        }
    }

    if had_error {
        error_result()
    } else {
        success_result()
    }
}

#[cfg(unix)]
pub(crate) fn success_result() -> ExecutionResult {
    ExecutionResult {
        exit_status: std::process::ExitStatus::from_raw(0),
        job_control: None,
    }
}

#[cfg(unix)]
fn error_result() -> ExecutionResult {
    ExecutionResult {
        exit_status: std::process::ExitStatus::from_raw(1 << 8),
        job_control: None,
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

/// source/. builtin - Execute commands from a file in the current shell context
fn builtin_source(args: &[String], context: &mut rush_expand::Context) -> Result<ExecutionResult, String> {
    if args.is_empty() {
        return Err("filename required".to_string());
    }

    let filename = &args[0];

    // Read the file
    let content = std::fs::read_to_string(filename)
        .map_err(|e| format!("{}: {}", filename, e))?;

    // Parse the entire content as a single unit to handle multiline constructs
    use rush_parser::parse_line;

    match parse_line(&content) {
        Ok(statement) => {
            execute_statement(&statement, context)
                .map_err(|e| format!("{}: {}", filename, e))
        }
        Err(e) => {
            Err(format!("{}: parse error: {}", filename, e))
        }
    }
}

/// eval builtin - Evaluate arguments as a shell command
fn builtin_eval(args: &[String], context: &mut rush_expand::Context) -> Result<ExecutionResult, String> {
    if args.is_empty() {
        return Ok(success_result());
    }

    // Join all arguments into a single command string
    let command = args.join(" ");

    // Parse and execute
    use rush_parser::parse_line;

    match parse_line(&command) {
        Ok(statement) => {
            execute_statement(&statement, context)
                .map_err(|e| format!("{}", e))
        }
        Err(e) => {
            Err(format!("parse error: {}", e))
        }
    }
}

/// Helper to execute a parsed statement
fn execute_statement(
    statement: &rush_parser::Statement,
    context: &mut rush_expand::Context,
) -> Result<ExecutionResult, String> {
    use rush_parser::Statement;

    match statement {
        Statement::Empty => Ok(success_result()),
        Statement::Complete(cmd) => {
            crate::control_flow::execute_complete_command(cmd, context)
                .map_err(|e| e.to_string())
        }
        Statement::Script(commands) => {
            let mut last_result = success_result();
            for cmd in commands {
                last_result = crate::control_flow::execute_complete_command(cmd, context)
                    .map_err(|e| e.to_string())?;
                context.set_exit_status(last_result.exit_code());
            }
            Ok(last_result)
        }
    }
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
