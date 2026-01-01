use rush_expand::Context;
use rush_parser::{CaseStatement, CompleteCommand, ForStatement, IfStatement, WhileStatement};
use rush_parser::ast::{CondExpr, SelectStatement, Word};
use crate::{ExecutionError, ExecutionResult, PipelineError};
use regex::Regex;
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

/// Execute a select loop
///
/// The select loop displays a numbered menu and reads user input:
/// ```bash
/// select var in option1 option2 option3; do
///     echo "Selected: $var"
/// done
/// ```
pub fn execute_select(
    select_stmt: &SelectStatement,
    context: &mut Context,
) -> Result<ExecutionResult, PipelineError> {
    use rush_expand::expand_word;
    use std::io::{self, BufRead, Write};

    let mut last_result = crate::command::success_result();

    // Expand all the words to get menu items
    let mut items = Vec::new();
    for word in &select_stmt.words {
        let expanded = expand_word(word, context)
            .map_err(|e| PipelineError::ExpansionError(e.to_string()))?;
        items.push(expanded);
    }

    // If no items, return immediately
    if items.is_empty() {
        return Ok(last_result);
    }

    // Get PS3 prompt (default: "#? ") - converted to owned String to avoid borrow conflicts
    let ps3 = context.get_var("PS3").unwrap_or("#? ").to_string();

    // Calculate column width for menu display
    let num_width = items.len().to_string().len();

    loop {
        // Display the menu to stderr (standard bash behavior)
        for (i, item) in items.iter().enumerate() {
            eprintln!("{:>width$}) {}", i + 1, item, width = num_width);
        }

        // Print the prompt and flush
        eprint!("{}", ps3);
        io::stderr().flush().ok();

        // Read user input
        let stdin = io::stdin();
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                // EOF - exit the loop
                break;
            }
            Ok(_) => {
                let input = line.trim();

                // Store raw input in REPLY
                let _ = context.set_var("REPLY", input);

                // Parse the number
                if let Ok(num) = input.parse::<usize>() {
                    if num >= 1 && num <= items.len() {
                        // Valid selection - set the variable
                        if let Err(name) = context.set_var(&select_stmt.var_name, &items[num - 1]) {
                            return Err(PipelineError::IoError(std::io::Error::new(
                                std::io::ErrorKind::PermissionDenied,
                                format!("{}: readonly variable", name),
                            )));
                        }
                    } else {
                        // Invalid number - set variable to empty
                        let _ = context.set_var(&select_stmt.var_name, "");
                    }
                } else {
                    // Not a number - set variable to empty
                    let _ = context.set_var(&select_stmt.var_name, "");
                }

                // Execute the loop body
                match execute_command_list(&select_stmt.body, context) {
                    Ok(result) => last_result = result,
                    Err(PipelineError::Break) => break,
                    Err(PipelineError::Continue) => continue,
                    Err(e) => return Err(e),
                }
            }
            Err(e) => {
                return Err(PipelineError::IoError(e));
            }
        }
    }

    Ok(last_result)
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
        CommandType::ExtendedTest(cond_expr) => {
            execute_extended_test(cond_expr, context)
        }
        CommandType::Select(select_stmt) => execute_select(select_stmt, context),
    }
}

/// Execute an extended test [[ expression ]]
pub fn execute_extended_test(
    expr: &CondExpr,
    context: &mut Context,
) -> Result<ExecutionResult, PipelineError> {
    let result = evaluate_cond_expr(expr, context)?;
    Ok(crate::command::exit_code_to_result(if result { 0 } else { 1 }))
}

/// Evaluate a conditional expression and return true/false
fn evaluate_cond_expr(expr: &CondExpr, context: &mut Context) -> Result<bool, PipelineError> {
    match expr {
        CondExpr::Or(left, right) => {
            // Short-circuit OR
            if evaluate_cond_expr(left, context)? {
                Ok(true)
            } else {
                evaluate_cond_expr(right, context)
            }
        }
        CondExpr::And(left, right) => {
            // Short-circuit AND
            if !evaluate_cond_expr(left, context)? {
                Ok(false)
            } else {
                evaluate_cond_expr(right, context)
            }
        }
        CondExpr::Not(inner) => {
            Ok(!evaluate_cond_expr(inner, context)?)
        }
        CondExpr::Unary { op, operand } => {
            let value = expand_word(operand, context)?;
            evaluate_unary_test(op, &value)
        }
        CondExpr::Binary { left, op, right } => {
            let left_val = expand_word(left, context)?;
            let right_val = expand_word(right, context)?;
            evaluate_binary_test(op, &left_val, &right_val, context)
        }
        CondExpr::Word(word) => {
            // Single word is true if non-empty
            let value = expand_word(word, context)?;
            Ok(!value.is_empty())
        }
    }
}

/// Helper to expand a word
fn expand_word(word: &Word, context: &mut Context) -> Result<String, PipelineError> {
    rush_expand::expand_word(word, context)
        .map_err(|e| PipelineError::ExpansionError(e.to_string()))
}

/// Evaluate unary test operator
fn evaluate_unary_test(op: &str, value: &str) -> Result<bool, PipelineError> {
    use std::fs;
    use std::os::unix::fs::FileTypeExt;

    match op {
        "-z" => Ok(value.is_empty()),
        "-n" => Ok(!value.is_empty()),
        "-e" => Ok(fs::metadata(value).is_ok()),
        "-f" => Ok(fs::metadata(value).map(|m| m.is_file()).unwrap_or(false)),
        "-d" => Ok(fs::metadata(value).map(|m| m.is_dir()).unwrap_or(false)),
        "-r" => {
            use std::os::unix::fs::PermissionsExt;
            Ok(fs::metadata(value).map(|m| m.permissions().mode() & 0o444 != 0).unwrap_or(false))
        }
        "-w" => {
            use std::os::unix::fs::PermissionsExt;
            Ok(fs::metadata(value).map(|m| m.permissions().mode() & 0o222 != 0).unwrap_or(false))
        }
        "-x" => {
            use std::os::unix::fs::PermissionsExt;
            Ok(fs::metadata(value).map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false))
        }
        "-s" => Ok(fs::metadata(value).map(|m| m.len() > 0).unwrap_or(false)),
        "-L" | "-h" => Ok(fs::symlink_metadata(value).map(|m| m.file_type().is_symlink()).unwrap_or(false)),
        "-p" => Ok(fs::metadata(value).map(|m| m.file_type().is_fifo()).unwrap_or(false)),
        "-S" => Ok(fs::metadata(value).map(|m| m.file_type().is_socket()).unwrap_or(false)),
        "-b" => Ok(fs::metadata(value).map(|m| m.file_type().is_block_device()).unwrap_or(false)),
        "-c" => Ok(fs::metadata(value).map(|m| m.file_type().is_char_device()).unwrap_or(false)),
        _ => Err(PipelineError::ExpansionError(format!("Unknown unary operator: {}", op))),
    }
}

/// Evaluate binary test operator
fn evaluate_binary_test(op: &str, left: &str, right: &str, context: &mut Context) -> Result<bool, PipelineError> {
    match op {
        "==" | "=" => {
            // Pattern matching (glob-style)
            if right.contains('*') || right.contains('?') || right.contains('[') {
                let pattern = format!("^{}$", glob_to_regex(right));
                let re = Regex::new(&pattern)
                    .map_err(|e| PipelineError::ExpansionError(e.to_string()))?;
                Ok(re.is_match(left))
            } else {
                Ok(left == right)
            }
        }
        "!=" => {
            if right.contains('*') || right.contains('?') || right.contains('[') {
                let pattern = format!("^{}$", glob_to_regex(right));
                let re = Regex::new(&pattern)
                    .map_err(|e| PipelineError::ExpansionError(e.to_string()))?;
                Ok(!re.is_match(left))
            } else {
                Ok(left != right)
            }
        }
        "=~" => {
            // Regex matching, sets BASH_REMATCH
            match Regex::new(right) {
                Ok(re) => {
                    if let Some(captures) = re.captures(left) {
                        // Set BASH_REMATCH array
                        let matches: Vec<String> = captures
                            .iter()
                            .map(|m| m.map(|m| m.as_str().to_string()).unwrap_or_default())
                            .collect();
                        context.arrays.insert(
                            "BASH_REMATCH".to_string(),
                            rush_expand::context::ArrayType::Indexed(matches),
                        );
                        Ok(true)
                    } else {
                        // Clear BASH_REMATCH on no match
                        context.arrays.remove("BASH_REMATCH");
                        Ok(false)
                    }
                }
                Err(_) => {
                    context.arrays.remove("BASH_REMATCH");
                    Ok(false)
                }
            }
        }
        "<" => Ok(left < right),
        ">" => Ok(left > right),
        "-eq" => {
            let l: i64 = left.parse().unwrap_or(0);
            let r: i64 = right.parse().unwrap_or(0);
            Ok(l == r)
        }
        "-ne" => {
            let l: i64 = left.parse().unwrap_or(0);
            let r: i64 = right.parse().unwrap_or(0);
            Ok(l != r)
        }
        "-lt" => {
            let l: i64 = left.parse().unwrap_or(0);
            let r: i64 = right.parse().unwrap_or(0);
            Ok(l < r)
        }
        "-le" => {
            let l: i64 = left.parse().unwrap_or(0);
            let r: i64 = right.parse().unwrap_or(0);
            Ok(l <= r)
        }
        "-gt" => {
            let l: i64 = left.parse().unwrap_or(0);
            let r: i64 = right.parse().unwrap_or(0);
            Ok(l > r)
        }
        "-ge" => {
            let l: i64 = left.parse().unwrap_or(0);
            let r: i64 = right.parse().unwrap_or(0);
            Ok(l >= r)
        }
        "-nt" => {
            // Newer than
            use std::fs;
            let left_time = fs::metadata(left).and_then(|m| m.modified()).ok();
            let right_time = fs::metadata(right).and_then(|m| m.modified()).ok();
            Ok(match (left_time, right_time) {
                (Some(l), Some(r)) => l > r,
                (Some(_), None) => true,
                _ => false,
            })
        }
        "-ot" => {
            // Older than
            use std::fs;
            let left_time = fs::metadata(left).and_then(|m| m.modified()).ok();
            let right_time = fs::metadata(right).and_then(|m| m.modified()).ok();
            Ok(match (left_time, right_time) {
                (Some(l), Some(r)) => l < r,
                (None, Some(_)) => true,
                _ => false,
            })
        }
        "-ef" => {
            // Same file (same device and inode)
            use std::os::unix::fs::MetadataExt;
            use std::fs;
            let left_meta = fs::metadata(left).ok();
            let right_meta = fs::metadata(right).ok();
            Ok(match (left_meta, right_meta) {
                (Some(l), Some(r)) => l.dev() == r.dev() && l.ino() == r.ino(),
                _ => false,
            })
        }
        _ => Err(PipelineError::ExpansionError(format!("Unknown binary operator: {}", op))),
    }
}

/// Convert glob pattern to regex
fn glob_to_regex(pattern: &str) -> String {
    let mut result = String::new();
    let mut chars = pattern.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '*' => result.push_str(".*"),
            '?' => result.push('.'),
            '[' => {
                result.push('[');
                // Handle character class
                while let Some(&c) = chars.peek() {
                    chars.next();
                    if c == ']' {
                        result.push(']');
                        break;
                    }
                    result.push(c);
                }
            }
            '.' | '+' | '^' | '$' | '(' | ')' | '{' | '}' | '|' | '\\' => {
                result.push('\\');
                result.push(ch);
            }
            _ => result.push(ch),
        }
    }

    result
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
