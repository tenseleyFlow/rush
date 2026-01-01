use crate::command::find_in_path;
use crate::redirect::{apply_redirects, RedirectError};
use crate::{ExecutionError, ExecutionResult};
use rush_expand::Context;
use rush_interactive::ErrorHints;
use rush_parser::ast::{AndOrList, AndOrOp, Pipeline, SimpleCommand};
use std::process::{Command, Stdio};
use thiserror::Error;

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

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

    // Control flow signals (not really errors)
    #[error("break")]
    Break,

    #[error("continue")]
    Continue,

    #[error("return")]
    Return(i32),
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
        match &pipeline.commands[0] {
            rush_parser::ast::PipelineElement::Simple(cmd) => {
                return execute_simple_with_redirects(cmd, context, false)
                    .map_err(PipelineError::from);
            }
            rush_parser::ast::PipelineElement::Subshell(subshell) => {
                return crate::execute_subshell(subshell, context)
                    .map_err(|e| PipelineError::ExecutionError(ExecutionError::CommandNotFound(e)));
            }
            rush_parser::ast::PipelineElement::ExtendedTest(cond) => {
                return crate::control_flow::execute_extended_test(cond, context);
            }
        }
    }

    // Build and spawn all commands in the pipeline
    let mut children = Vec::new();
    let mut subshell_pids = Vec::new(); // Track subshell PIDs separately
    let mut prev_stdout = None;

    for (i, element) in pipeline.commands.iter().enumerate() {
        let is_first = i == 0;
        let is_last = i == pipeline.commands.len() - 1;

        match element {
            rush_parser::ast::PipelineElement::Simple(simple_cmd) => {
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
                apply_redirects(&mut cmd, &simple_cmd.redirects, context)?;

                let mut child = cmd.spawn()?;
                if !is_last {
                    prev_stdout = child.stdout.take().map(Stdio::from);
                }
                children.push(child);
            }
            rush_parser::ast::PipelineElement::Subshell(subshell) => {
                // Execute subshell in pipeline using fork
                #[cfg(unix)]
                {
                    use nix::unistd::{fork, ForkResult, pipe as nix_pipe, dup2};
                    use std::os::unix::io::{IntoRawFd, FromRawFd};

                    // Create pipe for stdout if not last
                    let pipe_fds = if !is_last {
                        let (r, w) = nix_pipe().map_err(|e| ExecutionError::IoError(
                            std::io::Error::new(std::io::ErrorKind::Other, format!("pipe failed: {}", e))
                        ))?;
                        Some((r.into_raw_fd(), w.into_raw_fd()))
                    } else {
                        None
                    };

                    match unsafe { fork() } {
                        Ok(ForkResult::Child) => {
                            // Child process: execute subshell with redirected I/O
                            use nix::libc;

                            // Set up stdin from previous command if we have one
                            if let Some(prev) = prev_stdout {
                                // prev is Stdio, we need to extract the file descriptor
                                // This is tricky - Stdio doesn't expose the raw FD easily
                                // We'll skip stdin redirection for now in subshells
                                // TODO: Properly handle stdin from previous pipeline element
                            }

                            // Set up stdout to pipe if not last
                            if let Some((read_fd, write_fd)) = pipe_fds {
                                unsafe { libc::close(read_fd); } // Close read end in child
                                let _ = dup2(write_fd, 1);
                                unsafe { libc::close(write_fd); }
                            }

                            // Execute subshell commands
                            let exit_code = crate::subshell::execute_subshell_child(&subshell.commands, context);
                            std::process::exit(exit_code);
                        }
                        Ok(ForkResult::Parent { child }) => {
                            // Parent process: save pipe and track child PID
                            use nix::libc;
                            if let Some((read_fd, write_fd)) = pipe_fds {
                                unsafe { libc::close(write_fd); }
                                prev_stdout = Some(Stdio::from(unsafe { std::fs::File::from_raw_fd(read_fd) }));
                            }

                            // Track the subshell PID for later waiting
                            subshell_pids.push(child);
                        }
                        Err(e) => {
                            return Err(PipelineError::ExecutionError(ExecutionError::IoError(
                                std::io::Error::new(std::io::ErrorKind::Other, format!("fork failed: {}", e))
                            )));
                        }
                    }
                }
                #[cfg(not(unix))]
                {
                    return Err(PipelineError::ExecutionError(ExecutionError::CommandNotFound(
                        "Subshells in pipelines not supported on this platform".to_string()
                    )));
                }
            }
            rush_parser::ast::PipelineElement::ExtendedTest(_) => {
                // Extended tests in multi-command pipelines are not supported
                return Err(PipelineError::ExecutionError(ExecutionError::CommandNotFound(
                    "Extended tests in pipelines not yet supported".to_string()
                )));
            }
        }
    }

    // Wait for all commands to complete
    let mut last_exit_status = None;

    // Wait for regular process children
    for child in &mut children {
        let status = child.wait()?;
        last_exit_status = Some(status);
    }

    // Wait for subshell PIDs
    #[cfg(unix)]
    {
        use nix::sys::wait::{waitpid, WaitStatus};
        for pid in subshell_pids {
            match waitpid(pid, None) {
                Ok(WaitStatus::Exited(_, code)) => {
                    last_exit_status = Some(crate::command::exit_code_to_result(code).exit_status);
                }
                Ok(WaitStatus::Signaled(_, signal, _)) => {
                    last_exit_status = Some(crate::command::exit_code_to_result(128 + signal as i32).exit_status);
                }
                _ => {}
            }
        }
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
) -> Result<ExecutionResult, PipelineError> {
    // Process variable assignments
    for assignment in &cmd.assignments {
        // Check if this is an array literal assignment: arr=(one two three)
        let is_array_literal = assignment.value.parts.iter().any(|part| {
            matches!(part, rush_parser::ast::WordPart::ArrayLiteral(_))
        });

        if is_array_literal {
            // Extract array elements from the ArrayLiteral
            for part in &assignment.value.parts {
                if let rush_parser::ast::WordPart::ArrayLiteral(elements) = part {
                    let mut array_values = Vec::new();
                    for elem in elements {
                        let expanded = rush_expand::expand_word(elem, context)
                            .map_err(|e| PipelineError::ExpansionError(e.to_string()))?;
                        array_values.push(expanded);
                    }
                    context.arrays.insert(assignment.name.clone(), rush_expand::context::ArrayType::Indexed(array_values));
                }
            }
        } else if let Some(index) = &assignment.index {
            // arr[index]=value - indexed array assignment
            let value = rush_expand::expand_words(&[assignment.value.clone()], context)
                .map_err(|e| PipelineError::ExpansionError(e.to_string()))?;

            // Use Context helper method which handles both indexed and associative arrays
            if let Err(err) = context.set_array_element(&assignment.name, index.clone(), value.join(" ")) {
                return Err(PipelineError::IoError(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    err,
                )));
            }
        } else {
            // Regular variable assignment
            let value = rush_expand::expand_words(&[assignment.value.clone()], context)
                .map_err(|e| PipelineError::ExpansionError(e.to_string()))?;

            // Check if readonly
            if let Err(name) = context.set_var(&assignment.name, value.join(" ")) {
                return Err(PipelineError::IoError(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("{}: readonly variable", name),
                )));
            }
        }
    }

    // If there's no command to execute (just assignments), return success
    if cmd.words.is_empty() {
        return Ok(crate::command::success_result());
    }

    // Expand all words
    let expanded = rush_expand::expand_words(&cmd.words, context)
        .map_err(|e| PipelineError::ExpansionError(e.to_string()))?;

    if expanded.is_empty() {
        return Ok(crate::command::success_result());
    }

    let command_name = &expanded[0];
    let args = &expanded[1..];

    // Expand aliases (only for the command name, not args)
    let (actual_command, actual_args): (String, Vec<String>) = if let Some(alias_value) = context.aliases.get(command_name).cloned() {
        // Parse the alias value to get command and its args
        let parts: Vec<String> = alias_value.split_whitespace().map(|s| s.to_string()).collect();
        if parts.is_empty() {
            (command_name.to_string(), args.to_vec())
        } else {
            let cmd = parts[0].clone();
            let mut new_args: Vec<String> = parts[1..].to_vec();
            new_args.extend_from_slice(args);
            (cmd, new_args)
        }
    } else {
        (command_name.to_string(), args.to_vec())
    };

    // Handle control flow commands (must propagate as errors, not regular results)
    match actual_command.as_str() {
        "break" => return Err(PipelineError::Break),
        "continue" => return Err(PipelineError::Continue),
        "return" => {
            let code = actual_args.first()
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(context.last_exit_status);
            return Err(PipelineError::Return(code));
        }
        _ => {}
    }

    // Check if it's a built-in command
    if let Some(result) = crate::command::execute_builtin(&actual_command, &actual_args, context) {
        return Ok(result);
    }

    // Check if it's a function
    if let Some(function_def) = context.functions.get(&actual_command).cloned() {
        // Set up function parameters ($1, $2, etc.)
        // Save current positional parameters
        let saved_params = context.positional_params.clone();

        // Set new positional parameters from function arguments
        context.positional_params = actual_args.to_vec();

        // Push a new local scope for this function
        context.push_scope();

        // Execute the function body
        let mut last_result = crate::command::success_result();
        for cmd in &function_def.body {
            match crate::control_flow::execute_complete_command(cmd, context) {
                Ok(result) => last_result = result,
                Err(PipelineError::Return(code)) => {
                    // Clean up: pop scope and restore positional parameters
                    context.pop_scope();
                    context.positional_params = saved_params;

                    // Return from function with specified exit code
                    #[cfg(unix)]
                    {
                        return Ok(crate::command::ExecutionResult {
                            exit_status: std::process::ExitStatus::from_raw(code << 8),
                            job_control: None,
                        });
                    }
                    #[cfg(not(unix))]
                    {
                        // On non-Unix, approximate the exit code
                        if code == 0 {
                            return Ok(crate::command::success_result());
                        } else {
                            return Ok(crate::command::ExecutionResult {
                                exit_status: std::process::ExitStatus::default(),
                            });
                        }
                    }
                }
                Err(e) => {
                    // Clean up: pop scope and restore positional parameters
                    context.pop_scope();
                    context.positional_params = saved_params;
                    return Err(e);
                }
            }
        }

        // Clean up: pop scope and restore positional parameters
        context.pop_scope();
        context.positional_params = saved_params;

        return Ok(last_result);
    }

    // Find the command in PATH
    let program_path = find_in_path(&actual_command)
        .ok_or_else(|| PipelineError::ExecutionError(
            ExecutionError::CommandNotFound(ErrorHints::command_not_found(&actual_command))
        ))?;

    // Build the command
    let mut command = Command::new(program_path);
    command.args(&actual_args);

    // Apply redirections and get optional stdin content
    let stdin_content = apply_redirects(&mut command, &cmd.redirects, context)?;

    // If we have stdin content (heredoc/herestring), handle it specially
    if let Some(content) = stdin_content {
        use std::io::Write;

        // Spawn the command
        let mut child = command.spawn()?;

        // Write to stdin
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(content.as_bytes())?;
            // Close stdin by dropping it
            drop(stdin);
        }

        // Wait for the command to complete
        let status = child.wait()?;

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
                .map_err(PipelineError::from)
        }

        #[cfg(not(unix))]
        {
            crate::terminal::non_unix::execute_with_terminal_control(command, interactive)
                .map_err(PipelineError::from)
        }
    }
}

// Make find_in_path public (it's currently private in command.rs)
// We'll need to update command.rs to make it pub(crate)
