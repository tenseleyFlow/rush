use rush_expand::Context;
use rush_parser::{CaseStatement, CompleteCommand, ForStatement, IfStatement, WhileStatement};
use crate::{ExecutionError, ExecutionResult, PipelineError};
use globset::Glob;

/// Execute an if statement
pub fn execute_if(
    if_stmt: &IfStatement,
    context: &mut Context,
) -> Result<ExecutionResult, PipelineError> {
    // Execute the condition and check if it succeeded
    let condition_result = execute_complete_command(&if_stmt.condition, context)?;

    if condition_result.success() {
        // Condition was true, execute then body
        execute_command_list(&if_stmt.then_body, context)
    } else {
        // Check elif clauses
        for elif in &if_stmt.elif_clauses {
            let elif_result = execute_complete_command(&elif.condition, context)?;
            if elif_result.success() {
                return execute_command_list(&elif.then_body, context);
            }
        }

        // No elif matched, execute else body if present
        if let Some(else_body) = &if_stmt.else_body {
            execute_command_list(else_body, context)
        } else {
            // No else clause, return success (standard shell behavior)
            Ok(crate::command::success_result())
        }
    }
}

/// Execute a while loop
pub fn execute_while(
    while_stmt: &WhileStatement,
    context: &mut Context,
) -> Result<ExecutionResult, PipelineError> {
    let mut last_result = crate::command::success_result();

    loop {
        // Execute the condition
        let condition_result = execute_complete_command(&while_stmt.condition, context)?;

        if !condition_result.success() {
            // Condition failed, exit loop
            break;
        }

        // Execute the loop body
        match execute_command_list(&while_stmt.body, context) {
            Ok(result) => last_result = result,
            Err(PipelineError::Break) => break,
            Err(PipelineError::Continue) => continue,
            Err(e) => return Err(e),
        }
    }

    Ok(last_result)
}

/// Execute a for loop
pub fn execute_for(
    for_stmt: &ForStatement,
    context: &mut Context,
) -> Result<ExecutionResult, PipelineError> {
    use rush_expand::expand_word;

    let mut last_result = crate::command::success_result();

    // Expand all the words in the list
    let mut values = Vec::new();
    for word in &for_stmt.words {
        let expanded = expand_word(word, context)
            .map_err(|e| PipelineError::ExpansionError(e.to_string()))?;
        values.push(expanded);
    }

    // Execute the loop body for each value
    for value in values {
        // Set the loop variable
        if let Err(name) = context.set_var(&for_stmt.var_name, &value) {
            return Err(PipelineError::IoError(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("{}: readonly variable", name),
            )));
        }

        // Execute the loop body
        match execute_command_list(&for_stmt.body, context) {
            Ok(result) => last_result = result,
            Err(PipelineError::Break) => break,
            Err(PipelineError::Continue) => continue,
            Err(e) => return Err(e),
        }
    }

    Ok(last_result)
}

/// Check if a string matches a glob pattern (for case statement matching)
fn matches_pattern(text: &str, pattern: &str) -> bool {
    // Try to compile the glob pattern
    match Glob::new(pattern) {
        Ok(glob) => {
            let matcher = glob.compile_matcher();
            matcher.is_match(text)
        }
        Err(_) => {
            // If pattern is invalid, fall back to exact string matching
            text == pattern
        }
    }
}

/// Execute a case statement
pub fn execute_case(
    case_stmt: &CaseStatement,
    context: &mut Context,
) -> Result<ExecutionResult, PipelineError> {
    use rush_expand::expand_word;

    // Expand the word to match against
    let word_value = expand_word(&case_stmt.word, context)
        .map_err(|e| PipelineError::ExpansionError(e.to_string()))?;

    // Try each case clause
    for clause in &case_stmt.clauses {
        // Check if any pattern matches
        for pattern in &clause.patterns {
            let pattern_value = expand_word(pattern, context)
                .map_err(|e| PipelineError::ExpansionError(e.to_string()))?;

            // Use glob pattern matching
            if matches_pattern(&word_value, &pattern_value) {
                // Pattern matched, execute this clause's commands
                return execute_command_list(&clause.body, context);
            }
        }
    }

    // No clause matched, return success (standard shell behavior)
    Ok(crate::command::success_result())
}

/// Execute a complete command (helper for recursive execution)
pub(crate) fn execute_complete_command(
    cmd: &CompleteCommand,
    context: &mut Context,
) -> Result<ExecutionResult, PipelineError> {
    use rush_parser::ast::CommandType;

    // Handle background execution
    #[cfg(unix)]
    if cmd.background {
        return execute_background(cmd, context);
    }

    match &cmd.command {
        CommandType::Simple(simple_cmd) => {
            Ok(crate::execute_simple_with_redirects(simple_cmd, context, false)?)
        }
        CommandType::Pipeline(pipeline) => crate::execute_pipeline(pipeline, context),
        CommandType::AndOrList(and_or_list) => crate::execute_and_or_list(and_or_list, context),
        CommandType::If(if_stmt) => execute_if(if_stmt, context),
        CommandType::While(while_stmt) => execute_while(while_stmt, context),
        CommandType::For(for_stmt) => execute_for(for_stmt, context),
        CommandType::Case(case_stmt) => execute_case(case_stmt, context),
        CommandType::Function(function_def) => {
            // Store function in context
            context.functions.insert(function_def.name.clone(), function_def.clone());
            Ok(crate::command::success_result())
        }
        CommandType::Subshell(subshell) => {
            crate::execute_subshell(subshell, context).map_err(|e| {
                PipelineError::ExecutionError(ExecutionError::CommandNotFound(e))
            })
        }
    }
}

#[cfg(unix)]
fn execute_background(
    cmd: &CompleteCommand,
    context: &mut Context,
) -> Result<ExecutionResult, PipelineError> {
    use rush_parser::ast::CommandType;
    use std::process::{Command, Stdio};
    use std::os::unix::process::CommandExt;
    use nix::unistd::{setpgid, Pid};

    // For now, only support simple commands and pipelines in background
    match &cmd.command {
        CommandType::Simple(simple_cmd) => {
            // Expand the command
            let expanded = rush_expand::expand_words(&simple_cmd.words, context)
                .map_err(|e| PipelineError::ExpansionError(e.to_string()))?;

            if expanded.is_empty() {
                return Ok(crate::command::success_result());
            }

            let command_name = &expanded[0];
            let args = &expanded[1..];

            // Build the command
            let program_path = crate::command::find_in_path(command_name)
                .ok_or_else(|| crate::ExecutionError::CommandNotFound(
                    rush_interactive::ErrorHints::command_not_found(command_name)
                ))?;

            let mut command = Command::new(program_path);
            command.args(args);

            // Background jobs: stdin from /dev/null, stdout/stderr inherited
            command.stdin(Stdio::null());
            command.stdout(Stdio::inherit());
            command.stderr(Stdio::inherit());

            // Set up process group
            unsafe {
                command.pre_exec(|| {
                    // Put the child in its own process group
                    setpgid(Pid::from_raw(0), Pid::from_raw(0))?;
                    Ok(())
                });
            }

            // Spawn the child process
            let child = command.spawn()
                .map_err(|e| PipelineError::IoError(e))?;
            let child_pid = Pid::from_raw(child.id() as i32);

            // Set the child's process group (belt and suspenders)
            let _ = setpgid(child_pid, child_pid);

            // Add to job list
            let command_str = expanded.join(" ");
            let job_id = context.job_list.add_job(
                child_pid,  // pgid = pid for simple commands
                command_str.clone(),
                vec![child_pid],
                false,  // not foreground
            );

            // Print job notification
            println!("[{}] {}", job_id, child_pid);

            // Return success immediately (don't wait)
            Ok(crate::command::success_result())
        }
        _ => {
            // For now, don't support complex commands in background
            // Fall back to foreground execution
            eprintln!("rush: background execution of complex commands not yet supported");
            execute_complete_command(&CompleteCommand::foreground(cmd.command.clone()), context)
        }
    }
}

/// Execute a list of commands
fn execute_command_list(
    commands: &[CompleteCommand],
    context: &mut Context,
) -> Result<ExecutionResult, PipelineError> {
    let mut last_result = crate::command::success_result();

    for cmd in commands {
        last_result = execute_complete_command(cmd, context)?;
        context.set_exit_status(last_result.exit_code());
    }

    Ok(last_result)
}
