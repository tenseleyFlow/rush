use crate::command::find_in_path;
use crate::redirect::apply_redirects;
use crate::{ExecutionError, PipelineError};
use rush_expand::Context;
use rush_interactive::ErrorHints;
use rush_parser::ast::{Pipeline, SimpleCommand};
use std::process::{Command, Stdio};

#[cfg(unix)]
use nix::unistd::{setpgid, Pid};

/// Execute a simple command in the background
///
/// Returns (pid, pgid, command_string) for adding to JobList
#[cfg(unix)]
pub fn execute_simple_background(
    simple_cmd: &SimpleCommand,
    context: &mut Context,
) -> Result<(Pid, Pid, String), ExecutionError> {
    // Expand words
    let expanded = rush_expand::expand_words(&simple_cmd.words, context)
        .map_err(|e| ExecutionError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        )))?;

    if expanded.is_empty() {
        return Err(ExecutionError::EmptyCommand);
    }

    let command_name = &expanded[0];
    let args = &expanded[1..];

    // Find the command in PATH
    let program_path = find_in_path(command_name)
        .ok_or_else(|| ExecutionError::CommandNotFound(ErrorHints::command_not_found(command_name)))?;

    // Build the command string for display
    let command_string = format!("{} {}", command_name, args.join(" "));

    // Build the command
    let mut cmd = Command::new(program_path);
    cmd.args(args);

    // Apply redirections
    apply_redirects(&mut cmd, &simple_cmd.redirects, context)
        .map_err(|e| ExecutionError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        )))?;

    // Spawn the command
    let child = cmd.spawn()?;
    let pid = Pid::from_raw(child.id() as i32);

    // Put the child in its own process group
    // This is important for job control
    let pgid = pid; // Use the PID as the PGID (process becomes group leader)
    if let Err(e) = setpgid(pid, pgid) {
        eprintln!("Warning: failed to set process group: {}", e);
    }

    Ok((pid, pgid, command_string))
}

/// Execute a pipeline in the background
///
/// Returns (pids, pgid, command_string) for adding to JobList
#[cfg(unix)]
pub fn execute_pipeline_background(
    pipeline: &Pipeline,
    context: &mut Context,
) -> Result<(Vec<Pid>, Pid, String), PipelineError> {
    if pipeline.commands.is_empty() {
        return Err(PipelineError::EmptyPipeline);
    }

    // Build command string for display
    let mut command_parts = Vec::new();
    for element in &pipeline.commands {
        let simple_cmd = match element {
            rush_parser::ast::PipelineElement::Simple(cmd) => cmd,
            rush_parser::ast::PipelineElement::Subshell(_) => {
                return Err(PipelineError::ExecutionError(ExecutionError::CommandNotFound(
                    "Subshells in pipelines not yet fully supported".to_string()
                )));
            }
        };
        let expanded = rush_expand::expand_words(&simple_cmd.words, context)
            .map_err(|e| PipelineError::ExpansionError(e.to_string()))?;
        if !expanded.is_empty() {
            command_parts.push(expanded.join(" "));
        }
    }
    let command_string = command_parts.join(" | ");

    // Build and spawn all commands in the pipeline
    let mut pids = Vec::new();
    let mut prev_stdout = None;
    let mut pgid: Option<Pid> = None;

    for (i, element) in pipeline.commands.iter().enumerate() {
        let is_first = i == 0;
        let is_last = i == pipeline.commands.len() - 1;

        // Get the simple command from the element
        let simple_cmd = match element {
            rush_parser::ast::PipelineElement::Simple(cmd) => cmd,
            rush_parser::ast::PipelineElement::Subshell(_) => {
                return Err(PipelineError::ExecutionError(ExecutionError::CommandNotFound(
                    "Subshells in pipelines not yet fully supported".to_string()
                )));
            }
        };

        // Expand words
        let expanded = rush_expand::expand_words(&simple_cmd.words, context)
            .map_err(|e| PipelineError::ExpansionError(e.to_string()))?;

        if expanded.is_empty() {
            continue;
        }

        let command_name = &expanded[0];
        let args = &expanded[1..];

        // Find the command in PATH
        let program_path = find_in_path(command_name)
            .ok_or_else(|| ExecutionError::CommandNotFound(ErrorHints::command_not_found(command_name)))?;

        // Build the command
        let mut cmd = Command::new(program_path);
        cmd.args(args);

        // Set up stdin
        if is_first {
            cmd.stdin(Stdio::inherit());
        } else if let Some(prev_out) = prev_stdout.take() {
            cmd.stdin(prev_out);
        }

        // Set up stdout
        if is_last {
            cmd.stdout(Stdio::inherit());
        } else {
            cmd.stdout(Stdio::piped());
        }

        cmd.stderr(Stdio::inherit());

        // Apply redirections
        apply_redirects(&mut cmd, &simple_cmd.redirects, context)?;

        // Spawn the command
        let mut child = cmd.spawn()?;
        let pid = Pid::from_raw(child.id() as i32);

        // Set up process group
        // First process in pipeline becomes the group leader
        if pgid.is_none() {
            pgid = Some(pid);
            if let Err(e) = setpgid(pid, pid) {
                eprintln!("Warning: failed to set process group leader: {}", e);
            }
        } else {
            // Other processes join the group
            if let Err(e) = setpgid(pid, pgid.unwrap()) {
                eprintln!("Warning: failed to join process group: {}", e);
            }
        }

        pids.push(pid);

        // Save stdout for next command
        if !is_last {
            prev_stdout = child.stdout.take().map(Stdio::from);
        }

        // Don't wait - we're running in background
        std::mem::forget(child); // Prevent child from being killed when dropped
    }

    Ok((pids, pgid.unwrap(), command_string))
}

#[cfg(not(unix))]
pub fn execute_simple_background(
    _simple_cmd: &SimpleCommand,
    _context: &mut Context,
) -> Result<(u32, u32, String), ExecutionError> {
    Err(ExecutionError::IoError(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Background execution not supported on this platform",
    )))
}

#[cfg(not(unix))]
pub fn execute_pipeline_background(
    _pipeline: &Pipeline,
    _context: &mut Context,
) -> Result<(Vec<u32>, u32, String), PipelineError> {
    Err(PipelineError::IoError(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Background execution not supported on this platform",
    )))
}
