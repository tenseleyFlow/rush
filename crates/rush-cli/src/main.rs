use clap::Parser;
use rush_expand::Context;
use std::fs;
use std::io::{self, IsTerminal, Read};
use std::process::ExitCode;

mod heredoc;
mod repl;

#[derive(Parser)]
#[command(name = "rush")]
#[command(about = "The Rust Shell", long_about = None)]
struct Cli {
    /// Execute command string
    #[arg(short = 'c', value_name = "COMMAND")]
    command: Option<String>,

    /// Script file to execute
    #[arg(value_name = "FILE")]
    file: Option<String>,
}

fn main() -> ExitCode {
    // Set up signal handling for the shell
    if let Err(e) = rush_executor::setup_shell_signals() {
        eprintln!("rush: failed to set up signal handlers: {}", e);
        return ExitCode::from(1);
    }

    let cli = Cli::parse();

    // Determine execution mode
    if let Some(command) = cli.command {
        // Execute command string mode: rush -c "command"
        execute_string(&command)
    } else if let Some(file) = cli.file {
        // Execute script file mode: rush script.sh
        execute_file(&file)
    } else if io::stdin().is_terminal() {
        // Interactive mode (terminal)
        // Set up job control for interactive mode
        #[cfg(unix)]
        if let Err(e) = setup_interactive_shell() {
            eprintln!("rush: warning: failed to set up job control: {}", e);
        }

        repl::run_interactive()
    } else {
        // Non-interactive mode (stdin)
        execute_stdin()
    }
}

/// Set up the shell for interactive use with job control
#[cfg(unix)]
fn setup_interactive_shell() -> Result<(), String> {
    use rush_job::setup_shell_terminal;

    // Put shell in its own process group and take terminal control
    setup_shell_terminal().map_err(|e| e.to_string())?;

    Ok(())
}

/// Check for completed or stopped background jobs and update their status
#[cfg(unix)]
pub(crate) fn check_background_jobs(context: &mut Context) {
    use rush_job::check_children;

    // Check for any children that have changed state
    for (pid, status) in check_children() {
        // Update the job in the job list
        if let Some(job_id) = context.job_list.update_job_status(pid, status) {
            // Print notification for completed jobs
            if let Some(job) = context.job_list.get_job(job_id) {
                if job.is_completed() {
                    println!("[{}]  Done  {}", job.id, job.command);
                } else if job.is_stopped() {
                    println!("[{}]  Stopped  {}", job.id, job.command);
                }
            }
        }
    }

    // Clean up completed jobs
    context.job_list.clean_completed();
}

/// Execute a command string
fn execute_string(command: &str) -> ExitCode {
    let mut context = Context::new();
    match execute_line(command, &mut context, false) {
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            eprintln!("rush: {}", e);
            ExitCode::from(1)
        }
    }
}

/// Execute a script file
fn execute_file(path: &str) -> ExitCode {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("rush: {}: {}", path, e);
            return ExitCode::from(1);
        }
    };

    let mut context = Context::new();

    // Split into lines for heredoc support
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut lines_iter = lines.into_iter();

    let mut last_exit_code = 0;

    // Execute line-by-line with heredoc support
    while let Some(line) = lines_iter.next() {
        // Skip empty lines and comments
        if line.trim().is_empty() || line.trim().starts_with('#') {
            continue;
        }

        match execute_statement_with_heredocs(&line, &mut lines_iter, &mut context, false) {
            Ok(code) => last_exit_code = code,
            Err(e) => {
                eprintln!("rush: {}", e);
                return ExitCode::from(1);
            }
        }
    }

    ExitCode::from(last_exit_code as u8)
}

/// Execute commands from stdin
fn execute_stdin() -> ExitCode {
    let stdin = io::stdin();
    let mut content = String::new();

    // Read all of stdin at once to support multi-line control flow
    if let Err(e) = stdin.lock().read_to_string(&mut content) {
        eprintln!("rush: error reading stdin: {}", e);
        return ExitCode::from(1);
    }

    let mut context = Context::new();
    match execute_line(&content, &mut context, false) {
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            eprintln!("rush: {}", e);
            ExitCode::from(1)
        }
    }
}

/// Execute a single line of shell input
fn execute_line(line: &str, context: &mut Context, interactive: bool) -> Result<i32, String> {
    // For single-line execution, heredocs won't work (no way to get more lines)
    execute_statement_with_heredocs(line, &mut std::iter::empty(), context, interactive)
}

/// Execute a statement, optionally reading heredoc content from lines
fn execute_statement_with_heredocs<I>(
    line: &str,
    lines: &mut I,
    context: &mut Context,
    interactive: bool,
) -> Result<i32, String>
where
    I: Iterator<Item = String>,
{
    use rush_parser::{parse_line, Statement};

    let mut statement = parse_line(line).map_err(|e| e.to_string())?;

    // Check if we need to collect heredoc content
    if heredoc::has_heredocs(&statement) {
        let delimiters = heredoc::get_heredoc_delimiters(&statement);
        let mut content_map = std::collections::HashMap::new();

        // Collect content for each heredoc
        for delimiter in delimiters {
            let mut content_lines = Vec::new();

            // Read lines until we find the delimiter
            for line in lines.by_ref() {
                if line.trim() == delimiter {
                    break;
                }
                content_lines.push(line);
            }

            content_map.insert(delimiter, content_lines);
        }

        // Fill the content into the statement
        heredoc::fill_heredoc_content(&mut statement, &content_map);
    }

    match statement {
        Statement::Empty => Ok(0),
        Statement::Complete(complete_cmd) => {
            execute_complete_command(&complete_cmd, context, interactive)
        }
        Statement::Script(commands) => {
            let mut last_exit_code = 0;
            for cmd in commands {
                last_exit_code = execute_complete_command(&cmd, context, interactive)?;
            }
            Ok(last_exit_code)
        }
    }
}

fn execute_complete_command(
    cmd: &rush_parser::CompleteCommand,
    context: &mut Context,
    interactive: bool,
) -> Result<i32, String> {
    use rush_executor::{
        execute_and_or_list, execute_case, execute_for, execute_if, execute_pipeline,
        execute_simple_with_redirects, execute_while,
    };
    use rush_parser::ast::CommandType;

    // Handle background execution
    if cmd.background {
        return execute_background_command(cmd, context);
    }

    // Foreground execution (normal case)
    let result = match &cmd.command {
        CommandType::Simple(simple_cmd) => {
            execute_simple_with_redirects(simple_cmd, context, interactive)
                .map_err(|e| e.to_string())?
        }
        CommandType::Pipeline(pipeline) => {
            execute_pipeline(pipeline, context)
                .map_err(|e| e.to_string())?
        }
        CommandType::AndOrList(and_or_list) => {
            execute_and_or_list(and_or_list, context)
                .map_err(|e| e.to_string())?
        }
        CommandType::If(if_stmt) => {
            execute_if(if_stmt, context)
                .map_err(|e| e.to_string())?
        }
        CommandType::While(while_stmt) => {
            execute_while(while_stmt, context)
                .map_err(|e| e.to_string())?
        }
        CommandType::For(for_stmt) => {
            execute_for(for_stmt, context)
                .map_err(|e| e.to_string())?
        }
        CommandType::Case(case_stmt) => {
            execute_case(case_stmt, context)
                .map_err(|e| e.to_string())?
        }
        CommandType::Function(function_def) => {
            // Store function in context
            context.functions.insert(function_def.name.clone(), function_def.clone());
            // Return success
            rush_executor::command::ExecutionResult {
                exit_status: std::process::ExitStatus::default(),
                #[cfg(unix)]
                job_control: None,
            }
        }
    };

    // Check if the job was stopped (Ctrl-Z)
    #[cfg(unix)]
    if result.is_stopped() {
        if let Some(job_control) = &result.job_control {
            // Construct command string from the AST
            let command_string = build_command_string(&cmd.command, context);

            // Add to job list
            let job_id = context.job_list.add_job(
                job_control.pgid,
                command_string.clone(),
                vec![job_control.pid],
                false, // not foreground anymore
            );

            // Print notification
            eprintln!("[{}]+  Stopped  {}", job_id, command_string);
        }
    }

    let exit_code = result.exit_code();
    context.set_exit_status(exit_code);
    Ok(exit_code)
}

/// Build a command string from AST for display purposes
#[cfg(unix)]
fn build_command_string(cmd: &rush_parser::ast::CommandType, context: &Context) -> String {
    use rush_parser::ast::CommandType;

    match cmd {
        CommandType::Simple(simple_cmd) => {
            // Expand words to get the command as it was executed
            if let Ok(expanded) = rush_expand::expand_words(&simple_cmd.words, context) {
                expanded.join(" ")
            } else {
                // Fallback if expansion fails
                "<command>".to_string()
            }
        }
        CommandType::Pipeline(pipeline) => {
            let parts: Vec<String> = pipeline.commands.iter()
                .filter_map(|simple_cmd| {
                    if let Ok(expanded) = rush_expand::expand_words(&simple_cmd.words, context) {
                        if !expanded.is_empty() {
                            Some(expanded.join(" "))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect();
            parts.join(" | ")
        }
        _ => {
            // For control flow commands, just show a generic label
            // In practice, these won't be stopped in the foreground in Phase 5
            "command".to_string()
        }
    }
}

/// Execute a command in the background
#[cfg(unix)]
fn execute_background_command(
    cmd: &rush_parser::CompleteCommand,
    context: &mut Context,
) -> Result<i32, String> {
    use rush_executor::{execute_pipeline_background, execute_simple_background};
    use rush_parser::ast::CommandType;

    match &cmd.command {
        CommandType::Simple(simple_cmd) => {
            // Execute in background
            let (pid, pgid, command_string) = execute_simple_background(simple_cmd, context)
                .map_err(|e| e.to_string())?;

            // Add to job list
            let job_id = context.job_list.add_job(
                pgid,
                command_string.clone(),
                vec![pid],
                false, // not foreground
            );

            // Print job notification
            println!("[{}] {}", job_id, pid);

            // Background jobs return success immediately
            Ok(0)
        }
        CommandType::Pipeline(pipeline) => {
            // Execute pipeline in background
            let (pids, pgid, command_string) = execute_pipeline_background(pipeline, context)
                .map_err(|e| e.to_string())?;

            // Add to job list
            let job_id = context.job_list.add_job(
                pgid,
                command_string.clone(),
                pids.clone(),
                false, // not foreground
            );

            // Print job notification (show last PID in pipeline)
            println!("[{}] {}", job_id, pids.last().unwrap());

            // Background jobs return success immediately
            Ok(0)
        }
        _ => {
            // Control flow commands in background not yet supported
            Err("Background execution of control flow commands not yet supported".to_string())
        }
    }
}

#[cfg(not(unix))]
fn execute_background_command(
    _cmd: &rush_parser::CompleteCommand,
    _context: &mut Context,
) -> Result<i32, String> {
    Err("Background execution not supported on this platform".to_string())
}

// Make execute_line available to the repl module
pub(crate) fn execute_interactive_line(line: &str, context: &mut Context) -> Result<(), String> {
    execute_line(line, context, true).map(|_| ())
}
