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
    let mut last_exit_code = 0;
    for line in content.lines() {
        match execute_line(line, &mut context, false) {
            Ok(code) => last_exit_code = code,
            Err(e) => {
                eprintln!("rush: {}", e);
                last_exit_code = 1;
            }
        }
    }

    ExitCode::from(last_exit_code as u8)
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
    use rush_executor::execute_command;
    use rush_expand::expand_words;
    use rush_parser::{parse_line, Statement};

    let statement = parse_line(line).map_err(|e| e.to_string())?;

    match statement {
        Statement::Empty => Ok(0),
        Statement::Simple(cmd) => {
            // Process variable assignments
            for assignment in &cmd.assignments {
                let value = expand_words(&[assignment.value.clone()], context)
                    .map_err(|e| e.to_string())?;
                context.set_var(&assignment.name, value.join(" "));
            }

            // If there's a command to execute (not just assignments)
            if cmd.has_command() {
                // Expand all words (command + arguments)
                let expanded = expand_words(&cmd.words, context)
                    .map_err(|e| e.to_string())?;

                if let Some(command) = expanded.first() {
                    let args = &expanded[1..];
                    let exit_code = execute_command(command, args, interactive)
                        .map(|result| result.exit_code())
                        .map_err(|e| e.to_string())?;

                    context.set_exit_status(exit_code);
                    Ok(exit_code)
                } else {
                    Ok(0)
                }
            } else {
                // Just assignments, no command
                Ok(0)
            }
        }
    }
}

// Make execute_line available to the repl module
pub(crate) fn execute_interactive_line(line: &str, context: &mut Context) -> Result<(), String> {
    execute_line(line, context, true).map(|_| ())
}
