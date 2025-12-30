use clap::Parser;
use rush_expand::Context;
use std::fs;
use std::io::{self, BufRead, IsTerminal};
use std::process::ExitCode;

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
        repl::run_interactive()
    } else {
        // Non-interactive mode (stdin)
        execute_stdin()
    }
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
    // Parse and execute the entire file as one unit to support multi-line control flow
    match execute_line(&content, &mut context, false) {
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            eprintln!("rush: {}", e);
            ExitCode::from(1)
        }
    }
}

/// Execute commands from stdin
fn execute_stdin() -> ExitCode {
    let stdin = io::stdin();
    let mut context = Context::new();
    let mut last_exit_code = 0;

    for line in stdin.lock().lines() {
        match line {
            Ok(line) => match execute_line(&line, &mut context, false) {
                Ok(code) => last_exit_code = code,
                Err(e) => {
                    eprintln!("rush: {}", e);
                    last_exit_code = 1;
                }
            },
            Err(e) => {
                eprintln!("rush: error reading stdin: {}", e);
                return ExitCode::from(1);
            }
        }
    }

    ExitCode::from(last_exit_code as u8)
}

/// Execute a single line of shell input
fn execute_line(line: &str, context: &mut Context, interactive: bool) -> Result<i32, String> {
    use rush_parser::{parse_line, Statement};

    let statement = parse_line(line).map_err(|e| e.to_string())?;

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
    use rush_parser::CompleteCommand;

    match cmd {
        CompleteCommand::Simple(simple_cmd) => {
            let exit_code = execute_simple_with_redirects(simple_cmd, context, interactive)
                .map(|result| result.exit_code())
                .map_err(|e| e.to_string())?;

            context.set_exit_status(exit_code);
            Ok(exit_code)
        }
        CompleteCommand::Pipeline(pipeline) => {
            let exit_code = execute_pipeline(pipeline, context)
                .map(|result| result.exit_code())
                .map_err(|e| e.to_string())?;

            context.set_exit_status(exit_code);
            Ok(exit_code)
        }
        CompleteCommand::AndOrList(and_or_list) => {
            let exit_code = execute_and_or_list(and_or_list, context)
                .map(|result| result.exit_code())
                .map_err(|e| e.to_string())?;

            context.set_exit_status(exit_code);
            Ok(exit_code)
        }
        CompleteCommand::If(if_stmt) => {
            let exit_code = execute_if(if_stmt, context)
                .map(|result| result.exit_code())
                .map_err(|e| e.to_string())?;

            context.set_exit_status(exit_code);
            Ok(exit_code)
        }
        CompleteCommand::While(while_stmt) => {
            let exit_code = execute_while(while_stmt, context)
                .map(|result| result.exit_code())
                .map_err(|e| e.to_string())?;

            context.set_exit_status(exit_code);
            Ok(exit_code)
        }
        CompleteCommand::For(for_stmt) => {
            let exit_code = execute_for(for_stmt, context)
                .map(|result| result.exit_code())
                .map_err(|e| e.to_string())?;

            context.set_exit_status(exit_code);
            Ok(exit_code)
        }
        CompleteCommand::Case(case_stmt) => {
            let exit_code = execute_case(case_stmt, context)
                .map(|result| result.exit_code())
                .map_err(|e| e.to_string())?;

            context.set_exit_status(exit_code);
            Ok(exit_code)
        }
    }
}

// Make execute_line available to the repl module
pub(crate) fn execute_interactive_line(line: &str, context: &mut Context) -> Result<(), String> {
    execute_line(line, context, true).map(|_| ())
}
