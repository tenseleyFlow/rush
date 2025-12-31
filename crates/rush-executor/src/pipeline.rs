use crate::command::find_in_path;
use crate::redirect::{apply_redirects, RedirectError};
use crate::{ExecutionError, ExecutionResult};
use rush_expand::Context;
use rush_interactive::ErrorHints;
use rush_parser::ast::{AndOrList, AndOrOp, Pipeline, SimpleCommand};
use std::process::{Command, Stdio};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PipelineError {
    #[error("Execution error: {0}")]
    ExecutionError(#[from] ExecutionError),

    #[error("Redirect error: {0}")]
    RedirectError(#[from] RedirectError),

    #[error("Expansion error: {0}")]
    ExpansionError(String),

    #[error("Pipeline is empty")]
    EmptyPipeline,

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Execute a pipeline of commands
///
/// Each command's stdout is connected to the next command's stdin via a pipe.
/// Returns the exit status of the last command in the pipeline.
pub fn execute_pipeline(
    pipeline: &Pipeline,
    context: &mut Context,
) -> Result<ExecutionResult, PipelineError> {
    if pipeline.commands.is_empty() {
        return Err(PipelineError::EmptyPipeline);
    }

    // Special case: single command (not really a pipeline)
    if pipeline.commands.len() == 1 {
        return execute_simple_with_redirects(&pipeline.commands[0], context, false)
            .map_err(PipelineError::from);
    }

    // Build and spawn all commands in the pipeline
    let mut children = Vec::new();
    let mut prev_stdout = None;

    for (i, simple_cmd) in pipeline.commands.iter().enumerate() {
        let is_first = i == 0;
        let is_last = i == pipeline.commands.len() - 1;

        // Expand words
        let expanded = rush_expand::expand_words(&simple_cmd.words, context)
            .map_err(|e| PipelineError::ExpansionError(e.to_string()))?;

        if expanded.is_empty() {
            continue; // Skip empty commands
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
            // First command: use default stdin (unless redirected)
            cmd.stdin(Stdio::inherit());
        } else if let Some(prev_out) = prev_stdout.take() {
            // Middle/last commands: use previous command's stdout
            cmd.stdin(prev_out);
        }

        // Set up stdout
        if is_last {
            // Last command: use default stdout (unless redirected)
            cmd.stdout(Stdio::inherit());
        } else {
            // First/middle commands: create a pipe for next command
            cmd.stdout(Stdio::piped());
        }

        // stderr inherits from parent by default
        cmd.stderr(Stdio::inherit());

        // Apply redirections (can override stdin/stdout/stderr)
        apply_redirects(&mut cmd, &simple_cmd.redirects, context)?;

        // Spawn the command
        let mut child = cmd.spawn()?;

        // Save stdout for next command
        if !is_last {
            prev_stdout = child.stdout.take().map(Stdio::from);
        }

        children.push(child);
    }

    // Wait for all commands to complete
    let mut last_exit_status = None;
    for child in &mut children {
        let status = child.wait()?;
        last_exit_status = Some(status);
    }

    // Return the exit status of the last command
    Ok(ExecutionResult {
        exit_status: last_exit_status.unwrap(),
        #[cfg(unix)]
        job_control: None,
    })
}

/// Execute an AndOrList (commands connected by && or ||)
///
/// Commands are executed left-to-right with short-circuit evaluation:
/// - && executes the next command only if the previous succeeded (exit code 0)
/// - || executes the next command only if the previous failed (exit code != 0)
pub fn execute_and_or_list(
    and_or_list: &AndOrList,
    context: &mut Context,
) -> Result<ExecutionResult, PipelineError> {
    // Execute the first pipeline
    let mut last_result = execute_pipeline(&and_or_list.first, context)?;
    let mut last_exit_code = last_result.exit_code();

    // Execute remaining pipelines with their operators
    for (op, pipeline) in &and_or_list.rest {
        let should_execute = match op {
            AndOrOp::And => last_exit_code == 0,  // && - execute if previous succeeded
            AndOrOp::Or => last_exit_code != 0,   // || - execute if previous failed
        };

        if should_execute {
            last_result = execute_pipeline(pipeline, context)?;
            last_exit_code = last_result.exit_code();
        }
    }

    Ok(last_result)
}

/// Execute a simple command with redirections
///
/// This is used for both standalone commands and commands within pipelines.
pub fn execute_simple_with_redirects(
    cmd: &SimpleCommand,
    context: &mut Context,
    interactive: bool,
) -> Result<ExecutionResult, ExecutionError> {
    // Process variable assignments
    for assignment in &cmd.assignments {
        let value = rush_expand::expand_words(&[assignment.value.clone()], context)
            .map_err(|e| ExecutionError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            )))?;
        context.set_var(&assignment.name, value.join(" "));
    }

    // If there's no command to execute (just assignments), return success
    if cmd.words.is_empty() {
        return Ok(crate::command::success_result());
    }

    // Expand all words
    let expanded = rush_expand::expand_words(&cmd.words, context)
        .map_err(|e| ExecutionError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        )))?;

    if expanded.is_empty() {
        return Ok(crate::command::success_result());
    }

    let command_name = &expanded[0];
    let args = &expanded[1..];

    // Check if it's a built-in command
    if let Some(result) = crate::command::execute_builtin(command_name, args, context) {
        return Ok(result);
    }

    // Check if it's a function
    if let Some(function_def) = context.functions.get(command_name).cloned() {
        // TODO: Set up function parameters ($1, $2, etc.)
        // TODO: Create function scope
        // For now, just execute the function body
        let mut last_result = crate::command::success_result();
        for cmd in &function_def.body {
            last_result = crate::control_flow::execute_complete_command(cmd, context)
                .map_err(|e| ExecutionError::IoError(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                )))?;
        }
        return Ok(last_result);
    }

    // Find the command in PATH
    let program_path = find_in_path(command_name)
        .ok_or_else(|| ExecutionError::CommandNotFound(ErrorHints::command_not_found(command_name)))?;

    // Build the command
    let mut command = Command::new(program_path);
    command.args(args);

    // Apply redirections and get optional stdin content
    let stdin_content = apply_redirects(&mut command, &cmd.redirects, context)
        .map_err(|e| ExecutionError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        )))?;

    // If we have stdin content (heredoc/herestring), handle it specially
    if let Some(content) = stdin_content {
        use std::io::Write;

        // Spawn the command
        let mut child = command.spawn()
            .map_err(|e| ExecutionError::IoError(e))?;

        // Write to stdin
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(content.as_bytes())
                .map_err(|e| ExecutionError::IoError(e))?;
            // Close stdin by dropping it
            drop(stdin);
        }

        // Wait for the command to complete
        let status = child.wait()
            .map_err(|e| ExecutionError::IoError(e))?;

        Ok(crate::command::ExecutionResult {
            exit_status: status,
            #[cfg(unix)]
            job_control: None,
        })
    } else {
        // No stdin content - execute normally with terminal handling
        #[cfg(unix)]
        {
            crate::terminal::unix::execute_with_terminal_control(command, interactive)
        }

        #[cfg(not(unix))]
        {
            crate::terminal::non_unix::execute_with_terminal_control(command, interactive)
        }
    }
}

// Make find_in_path public (it's currently private in command.rs)
// We'll need to update command.rs to make it pub(crate)
