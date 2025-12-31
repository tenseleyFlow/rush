use rush_expand::Context;
use rush_parser::{CaseStatement, CompleteCommand, ForStatement, IfStatement, WhileStatement};
use crate::{ExecutionResult, PipelineError};

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
        last_result = execute_command_list(&while_stmt.body, context)?;
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
        context.set_var(&for_stmt.var_name, &value);

        // Execute the loop body
        last_result = execute_command_list(&for_stmt.body, context)?;
    }

    Ok(last_result)
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

            // TODO: Implement glob pattern matching
            // For now, just do exact string matching
            if word_value == pattern_value {
                // Pattern matched, execute this clause's commands
                return execute_command_list(&clause.body, context);
            }
        }
    }

    // No clause matched, return success (standard shell behavior)
    Ok(crate::command::success_result())
}

/// Execute a complete command (helper for recursive execution)
fn execute_complete_command(
    cmd: &CompleteCommand,
    context: &mut Context,
) -> Result<ExecutionResult, PipelineError> {
    use rush_parser::ast::CommandType;

    // TODO: Handle background execution (cmd.background)
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
