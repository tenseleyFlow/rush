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
        "true" => Some(success_result()),
        "false" => Some(error_result()),
        ":" => Some(success_result()),
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
        "set" => Some(builtin_set(args, context)),
        "shopt" => Some(builtin_shopt(args, context)),
        "export" => Some(builtin_export(args, context)),
        "unset" => Some(builtin_unset(args, context)),
        "readonly" => Some(builtin_readonly(args, context)),
        "declare" | "typeset" => Some(builtin_declare(args, context)),
        "local" => Some(builtin_local(args, context)),
        "read" => Some(builtin_read(args, context)),
        "shift" => Some(builtin_shift(args, context)),
        "wait" => Some(builtin_wait(args, context)),
        "kill" => Some(builtin_kill(args, context)),
        "times" => Some(builtin_times(args, context)),
        "umask" => Some(builtin_umask(args, context)),
        "hash" => Some(builtin_hash(args, context)),
        "getopts" => Some(builtin_getopts(args, context)),
        "exec" => {
            match builtin_exec(args, context) {
                Ok(result) => Some(result),
                Err(err) => {
                    eprintln!("exec: {}", err);
                    Some(error_result())
                }
            }
        }
        "command" => {
            match builtin_command(args, context) {
                Ok(result) => Some(result),
                Err(err) => {
                    eprintln!("command: {}", err);
                    Some(error_result())
                }
            }
        }
        #[cfg(unix)]
        "jobs" => Some(builtin_jobs(context)),
        #[cfg(unix)]
        "fg" => Some(builtin_fg(args, context)),
        #[cfg(unix)]
        "bg" => Some(builtin_bg(args, context)),
        #[cfg(unix)]
        "coproc" => Some(builtin_coproc(args, context)),
        #[cfg(unix)]
        "disown" => Some(builtin_disown(args, context)),
        #[cfg(not(unix))]
        "jobs" | "fg" | "bg" | "coproc" | "disown" => {
            eprintln!("{}: job control not supported on this platform", command);
            Some(error_result())
        }
        "printf" => Some(builtin_printf(args, context)),
        "mapfile" | "readarray" => Some(builtin_mapfile(args, context)),
        _ => None,
    }
}

pub(crate) fn exit_code_to_result(code: i32) -> ExecutionResult {
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

/// set builtin - Set or display shell options
fn builtin_set(args: &[String], context: &mut rush_expand::Context) -> ExecutionResult {
    // No arguments: display all variables
    if args.is_empty() {
        let mut vars: Vec<_> = context.all_vars().iter().collect();
        vars.sort_by_key(|(name, _)| *name);
        for (name, value) in vars {
            println!("{}={}", name, value);
        }
        return success_result();
    }

    // Process options
    for arg in args {
        if arg.starts_with('-') || arg.starts_with('+') {
            let enable = arg.starts_with('-');
            let opts = &arg[1..];

            for ch in opts.chars() {
                match ch {
                    'e' => context.options.errexit = enable,
                    'x' => context.options.xtrace = enable,
                    'u' => context.options.nounset = enable,
                    'f' => context.options.noglob = enable,
                    'o' => {
                        // -o option_name format (next arg is the option)
                        // For simplicity, we'll handle this separately if needed
                        eprintln!("set: -o option requires an argument");
                        return error_result();
                    }
                    _ => {
                        eprintln!("set: -{}: invalid option", ch);
                        return error_result();
                    }
                }
            }
        } else if arg == "--" {
            // End of options marker
            break;
        } else {
            // Positional parameters (not implemented yet)
            eprintln!("set: positional parameters not yet supported");
            return error_result();
        }
    }

    success_result()
}

/// shopt builtin - Set or display bash-specific shell options
fn builtin_shopt(args: &[String], context: &mut rush_expand::Context) -> ExecutionResult {
    let mut print_format = false;
    let mut set_option = false;
    let mut unset_option = false;
    let mut quiet = false;
    let mut options_to_process = Vec::new();

    // Parse arguments
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-p" {
            print_format = true;
        } else if arg == "-s" {
            set_option = true;
        } else if arg == "-u" {
            unset_option = true;
        } else if arg == "-q" {
            quiet = true;
        } else if arg.starts_with('-') {
            eprintln!("shopt: {}: invalid option", arg);
            return error_result();
        } else {
            options_to_process.push(arg.clone());
        }
        i += 1;
    }

    // No arguments: list all options
    if options_to_process.is_empty() && !print_format {
        println!("nullglob\t{}", if context.options.nullglob { "on" } else { "off" });
        println!("dotglob\t\t{}", if context.options.dotglob { "on" } else { "off" });
        println!("extglob\t\t{}", if context.options.extglob { "on" } else { "off" });
        return success_result();
    }

    // -p: print in reusable format
    if print_format && options_to_process.is_empty() {
        println!("shopt -{} nullglob", if context.options.nullglob { "s" } else { "u" });
        println!("shopt -{} dotglob", if context.options.dotglob { "s" } else { "u" });
        println!("shopt -{} extglob", if context.options.extglob { "s" } else { "u" });
        return success_result();
    }

    // Process specific options
    let mut had_error = false;
    for opt_name in &options_to_process {
        match opt_name.as_str() {
            "nullglob" => {
                if set_option {
                    context.options.nullglob = true;
                } else if unset_option {
                    context.options.nullglob = false;
                } else if print_format {
                    println!("shopt -{} nullglob", if context.options.nullglob { "s" } else { "u" });
                } else if !quiet {
                    println!("nullglob\t{}", if context.options.nullglob { "on" } else { "off" });
                }
            }
            "dotglob" => {
                if set_option {
                    context.options.dotglob = true;
                } else if unset_option {
                    context.options.dotglob = false;
                } else if print_format {
                    println!("shopt -{} dotglob", if context.options.dotglob { "s" } else { "u" });
                } else if !quiet {
                    println!("dotglob\t\t{}", if context.options.dotglob { "on" } else { "off" });
                }
            }
            "extglob" => {
                if set_option {
                    context.options.extglob = true;
                } else if unset_option {
                    context.options.extglob = false;
                } else if print_format {
                    println!("shopt -{} extglob", if context.options.extglob { "s" } else { "u" });
                } else if !quiet {
                    println!("extglob\t\t{}", if context.options.extglob { "on" } else { "off" });
                }
            }
            _ => {
                if !quiet {
                    eprintln!("shopt: {}: invalid shell option name", opt_name);
                }
                had_error = true;
            }
        }
    }

    if had_error {
        error_result()
    } else {
        success_result()
    }
}

/// export builtin - Mark variables for export to child processes
fn builtin_export(args: &[String], context: &mut rush_expand::Context) -> ExecutionResult {
    let mut print_format = false;
    let mut unexport = false;
    let mut vars_to_process = Vec::new();

    // Parse arguments
    for arg in args {
        if arg == "-p" {
            print_format = true;
        } else if arg == "-n" {
            unexport = true;
        } else if arg.starts_with('-') {
            eprintln!("export: {}: invalid option", arg);
            return error_result();
        } else {
            vars_to_process.push(arg.clone());
        }
    }

    // No arguments or -p: list all exported variables
    if vars_to_process.is_empty() {
        let mut exported: Vec<_> = context.exported_vars().iter().collect();
        exported.sort_by_key(|(name, _)| *name);
        for (name, value) in exported {
            if print_format {
                println!("export {}={}", name, value);
            } else {
                println!("export {}={}", name, value);
            }
        }
        return success_result();
    }

    // Process each variable
    for var in &vars_to_process {
        if let Some(eq_pos) = var.find('=') {
            // VAR=value format
            let name = &var[..eq_pos];
            let value = &var[eq_pos + 1..];

            if unexport {
                eprintln!("export: -n: cannot assign value and unexport");
                return error_result();
            }

            context.export_var(name, value);
        } else {
            // Just VAR (no value)
            if unexport {
                // Remove from exported (but keep in variables)
                context.unexport_var(var);
            } else {
                // Export existing variable
                if let Some(value) = context.get_var(var).map(|s| s.to_string()) {
                    context.export_var(var, value);
                } else {
                    // Variable doesn't exist, export with empty value
                    context.export_var(var, "");
                }
            }
        }
    }

    success_result()
}

/// unset builtin - Unset variables or functions
fn builtin_unset(args: &[String], context: &mut rush_expand::Context) -> ExecutionResult {
    let mut unset_vars = true;
    let mut unset_funcs = true;
    let mut names_to_unset = Vec::new();

    // Parse arguments
    for arg in args {
        if arg == "-v" {
            unset_vars = true;
            unset_funcs = false;
        } else if arg == "-f" {
            unset_vars = false;
            unset_funcs = true;
        } else if arg.starts_with('-') {
            eprintln!("unset: {}: invalid option", arg);
            return error_result();
        } else {
            names_to_unset.push(arg.clone());
        }
    }

    if names_to_unset.is_empty() {
        // No error, just do nothing (POSIX behavior)
        return success_result();
    }

    // Unset each name
    for name in &names_to_unset {
        if unset_vars {
            context.unset_var(name);
        }
        if unset_funcs {
            context.functions.remove(name);
        }
    }

    success_result()
}

/// readonly builtin - Mark variables as readonly
fn builtin_readonly(args: &[String], context: &mut rush_expand::Context) -> ExecutionResult {
    let mut _print_format = false; // TODO: implement -p format output
    let mut vars_to_process = Vec::new();

    // Parse arguments
    for arg in args {
        if arg == "-p" {
            _print_format = true;
        } else if arg == "-f" {
            // Readonly functions not yet supported
            eprintln!("readonly: -f: readonly functions not yet supported");
            return error_result();
        } else if arg.starts_with('-') {
            eprintln!("readonly: {}: invalid option", arg);
            return error_result();
        } else {
            vars_to_process.push(arg.clone());
        }
    }

    // No arguments or -p: list all readonly variables
    if vars_to_process.is_empty() {
        let mut readonly_list: Vec<_> = context.readonly_vars().iter().collect();
        readonly_list.sort();
        for name in readonly_list {
            if let Some(value) = context.get_var(name) {
                println!("readonly {}={}", name, value);
            } else {
                println!("readonly {}", name);
            }
        }
        return success_result();
    }

    // Process each variable
    for var in &vars_to_process {
        if let Some(eq_pos) = var.find('=') {
            // VAR=value format
            let name = &var[..eq_pos];
            let value = &var[eq_pos + 1..];

            // Check if already readonly
            if context.is_readonly(name) {
                eprintln!("readonly: {}: readonly variable", name);
                return error_result();
            }

            // Set value and mark readonly
            // Ignore result since we already checked readonly above
            let _ = context.set_var(name, value);
            context.mark_readonly(name);
        } else {
            // Just VAR (no value) - mark existing variable as readonly
            if !context.is_readonly(var) {
                // If variable doesn't exist, create it with empty value
                if context.get_var(var).is_none() {
                    let _ = context.set_var(var, "");
                }
                context.mark_readonly(var);
            }
        }
    }

    success_result()
}

/// declare/typeset builtin - Declare variables and arrays with attributes
fn builtin_declare(args: &[String], context: &mut rush_expand::Context) -> ExecutionResult {
    let mut print_mode = false;
    let mut indexed_array = false;
    let mut associative_array = false;
    let mut readonly = false;
    let mut export = false;
    let mut vars_to_process = Vec::new();

    // Parse arguments
    for arg in args {
        if arg.starts_with('-') {
            for ch in arg.chars().skip(1) {
                match ch {
                    'a' => indexed_array = true,
                    'A' => associative_array = true,
                    'r' => readonly = true,
                    'x' => export = true,
                    'p' => print_mode = true,
                    _ => {
                        eprintln!("declare: -{}: invalid option", ch);
                        return error_result();
                    }
                }
            }
        } else {
            vars_to_process.push(arg.clone());
        }
    }

    // Check for conflicting flags
    if indexed_array && associative_array {
        eprintln!("declare: cannot use -a and -A together");
        return error_result();
    }

    // Print mode
    if print_mode {
        if vars_to_process.is_empty() {
            // Print all variables
            let mut all_vars: Vec<_> = context.all_vars().iter().collect();
            all_vars.sort_by_key(|(name, _)| *name);
            for (name, value) in all_vars {
                let mut attrs = String::new();
                if context.is_readonly(name) {
                    attrs.push_str("r");
                }
                if context.is_exported(name) {
                    attrs.push_str("x");
                }
                if context.is_associative_array(name) {
                    attrs.push_str("A");
                } else if context.arrays.contains_key(name) {
                    attrs.push_str("a");
                }

                if attrs.is_empty() {
                    println!("declare -- {}={}", name, value);
                } else {
                    println!("declare -{} {}={}", attrs, name, value);
                }
            }
            return success_result();
        } else {
            // Print specific variables
            for var in &vars_to_process {
                // Check if it's an array
                if let Some(array) = context.arrays.get(var) {
                    let mut attrs = String::new();
                    if context.is_readonly(var) {
                        attrs.push_str("r");
                    }
                    if context.is_exported(var) {
                        attrs.push_str("x");
                    }

                    match array {
                        rush_expand::context::ArrayType::Indexed(vec) => {
                            attrs.push_str("a");
                            // Print array elements
                            let elements: Vec<String> = vec.iter()
                                .enumerate()
                                .map(|(i, v)| format!("[{}]=\"{}\"", i, v))
                                .collect();
                            if attrs.is_empty() {
                                println!("declare -- {}=({})", var, elements.join(" "));
                            } else {
                                println!("declare -{} {}=({})", attrs, var, elements.join(" "));
                            }
                        }
                        rush_expand::context::ArrayType::Associative(map) => {
                            attrs.push_str("A");
                            // Print associative array elements
                            let mut elements: Vec<String> = map.iter()
                                .map(|(k, v)| format!("[{}]=\"{}\"", k, v))
                                .collect();
                            elements.sort();
                            if attrs.is_empty() {
                                println!("declare -- {}=({})", var, elements.join(" "));
                            } else {
                                println!("declare -{} {}=({})", attrs, var, elements.join(" "));
                            }
                        }
                    }
                } else if let Some(value) = context.get_var(var) {
                    // Regular variable
                    let mut attrs = String::new();
                    if context.is_readonly(var) {
                        attrs.push_str("r");
                    }
                    if context.is_exported(var) {
                        attrs.push_str("x");
                    }

                    if attrs.is_empty() {
                        println!("declare -- {}=\"{}\"", var, value);
                    } else {
                        println!("declare -{} {}=\"{}\"", attrs, var, value);
                    }
                }
            }
            return success_result();
        }
    }

    // Process each variable
    for var in &vars_to_process {
        if let Some(eq_pos) = var.find('=') {
            // VAR=value format
            let name = &var[..eq_pos];
            let value = &var[eq_pos + 1..];

            // Check if readonly
            if context.is_readonly(name) {
                eprintln!("declare: {}: readonly variable", name);
                return error_result();
            }

            // Handle array declarations
            if associative_array {
                // Create associative array
                context.create_assoc_array(name.to_string());
                // TODO: Parse compound assignments like arr=([key1]=val1 [key2]=val2)
                // For now, just create empty array
            } else if indexed_array {
                // Create indexed array
                // TODO: Parse array literal assignments like arr=(one two three)
                // For now, set as regular variable
                let _ = context.set_var(name, value);
            } else {
                // Regular variable
                let _ = context.set_var(name, value);
            }

            // Apply attributes
            if readonly {
                context.mark_readonly(name);
            }
            if export {
                // Clone the value to avoid borrow checker issues
                let val = context.get_var(name).map(|s| s.to_string());
                if let Some(v) = val {
                    context.export_var(name, v);
                }
            }
        } else {
            // Just VAR (no value) - declare without assignment

            // Create array if -a or -A specified
            if associative_array {
                if !context.is_readonly(var) {
                    context.create_assoc_array(var.to_string());
                } else {
                    eprintln!("declare: {}: readonly variable", var);
                    return error_result();
                }
            } else if indexed_array {
                if !context.is_readonly(var) {
                    context.create_indexed_array(var.to_string());
                } else {
                    eprintln!("declare: {}: readonly variable", var);
                    return error_result();
                }
            } else if context.get_var(var).is_none() {
                // Create variable with empty value if it doesn't exist
                let _ = context.set_var(var, "");
            }

            // Apply attributes
            if readonly {
                context.mark_readonly(var);
            }
            if export {
                // Clone the value to avoid borrow checker issues
                let val = context.get_var(var).map(|s| s.to_string());
                if let Some(v) = val {
                    context.export_var(var, v);
                } else {
                    // Export with empty value
                    context.export_var(var, "");
                }
            }
        }
    }

    success_result()
}

/// local builtin - Create function-local variables
fn builtin_local(args: &[String], context: &mut rush_expand::Context) -> ExecutionResult {
    if args.is_empty() {
        // No arguments - success (bash behavior)
        return success_result();
    }

    // Process each variable assignment
    for arg in args {
        if arg.starts_with('-') {
            eprintln!("local: {}: invalid option", arg);
            return error_result();
        }

        if let Some(eq_pos) = arg.find('=') {
            // VAR=value format
            let name = &arg[..eq_pos];
            let value = &arg[eq_pos + 1..];

            // Set as local variable in current function scope
            if let Err(var_name) = context.set_local_var(name, value) {
                eprintln!("local: {}: readonly variable", var_name);
                return error_result();
            }
        } else {
            // Just VAR (no value) - create with empty value
            if let Err(var_name) = context.set_local_var(arg, "") {
                eprintln!("local: {}: readonly variable", var_name);
                return error_result();
            }
        }
    }

    success_result()
}

/// read builtin - Read a line from stdin into variables
fn builtin_read(args: &[String], context: &mut rush_expand::Context) -> ExecutionResult {
    use std::io::{self, BufRead, Write};

    let mut raw_mode = false;
    let mut prompt = String::new();
    let mut var_names = Vec::new();

    // Parse arguments
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-r" {
            raw_mode = true;
        } else if arg == "-p" {
            // Next argument is the prompt
            i += 1;
            if i >= args.len() {
                eprintln!("read: -p: option requires an argument");
                return error_result();
            }
            prompt = args[i].clone();
        } else if arg.starts_with('-') {
            eprintln!("read: {}: invalid option", arg);
            return error_result();
        } else {
            var_names.push(arg.clone());
        }
        i += 1;
    }

    // Default to REPLY if no variable names given
    if var_names.is_empty() {
        var_names.push("REPLY".to_string());
    }

    // Display prompt if provided
    if !prompt.is_empty() {
        print!("{}", prompt);
        let _ = io::stdout().flush();
    }

    // Read line from stdin
    let stdin = io::stdin();
    let mut line = String::new();
    match stdin.lock().read_line(&mut line) {
        Ok(0) => {
            // EOF
            return error_result();
        }
        Ok(_) => {
            // Remove trailing newline
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
        }
        Err(_) => {
            return error_result();
        }
    }

    // Handle backslash continuation (unless in raw mode)
    if !raw_mode {
        while line.ends_with('\\') {
            line.pop(); // Remove backslash
            let mut continuation = String::new();
            match stdin.lock().read_line(&mut continuation) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    if continuation.ends_with('\n') {
                        continuation.pop();
                        if continuation.ends_with('\r') {
                            continuation.pop();
                        }
                    }
                    line.push_str(&continuation);
                }
                Err(_) => break,
            }
        }
    }

    // Get IFS for splitting (default to whitespace)
    let ifs = context.get_var("IFS").unwrap_or(" \t\n");
    let ifs_chars: Vec<char> = ifs.chars().collect();

    // Split the line and assign to variables
    if var_names.len() == 1 {
        // Single variable gets entire line
        let _ = context.set_var(&var_names[0], line);
    } else {
        // Multiple variables: split by IFS
        let words: Vec<&str> = if ifs_chars.is_empty() {
            // Empty IFS means split every character
            vec![line.as_str()]
        } else {
            // Split by IFS characters
            line.split(|c: char| ifs_chars.contains(&c))
                .filter(|s| !s.is_empty())
                .collect()
        };

        // Assign words to variables
        for (i, var_name) in var_names.iter().enumerate() {
            if i < words.len() {
                if i == var_names.len() - 1 {
                    // Last variable gets all remaining words
                    let remaining = words[i..].join(" ");
                    let _ = context.set_var(var_name, remaining);
                } else {
                    let _ = context.set_var(var_name, words[i]);
                }
            } else {
                // No more input, set to empty
                let _ = context.set_var(var_name, "");
            }
        }
    }

    success_result()
}

/// shift builtin - Shift positional parameters
fn builtin_shift(args: &[String], context: &mut rush_expand::Context) -> ExecutionResult {
    // Parse the count (default 1)
    let count = if args.is_empty() {
        1
    } else {
        match args[0].parse::<usize>() {
            Ok(n) => n,
            Err(_) => {
                eprintln!("shift: {}: numeric argument required", args[0]);
                return error_result();
            }
        }
    };

    // Check if we have enough parameters to shift
    if count > context.positional_params.len() {
        eprintln!("shift: shift count ({}) exceeds number of positional parameters ({})",
                  count, context.positional_params.len());
        return error_result();
    }

    // Shift the parameters
    context.positional_params.drain(0..count);

    success_result()
}

/// command builtin - Execute command bypassing functions and aliases
fn builtin_command(args: &[String], context: &mut rush_expand::Context) -> Result<ExecutionResult, String> {
    let mut verbose = false;
    let mut very_verbose = false;
    let mut _use_default_path = false; // TODO: implement -p to use default PATH
    let mut cmd_args = Vec::new();

    // Parse arguments
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-v" {
            verbose = true;
        } else if arg == "-V" {
            very_verbose = true;
        } else if arg == "-p" {
            _use_default_path = true;
        } else if arg == "--" {
            // End of options
            i += 1;
            cmd_args.extend_from_slice(&args[i..]);
            break;
        } else if arg.starts_with('-') {
            return Err(format!("{}: invalid option", arg));
        } else {
            cmd_args.extend_from_slice(&args[i..]);
            break;
        }
        i += 1;
    }

    if cmd_args.is_empty() {
        return Err("usage: command [-pvV] command [arg ...]".to_string());
    }

    let cmd_name = &cmd_args[0];

    // -v: print path to command
    if verbose {
        // Check if it's a builtin
        if execute_builtin(cmd_name, &[], context).is_some() {
            println!("{}", cmd_name);
            return Ok(success_result());
        }

        // Check if it's in PATH
        if let Some(path) = find_in_path(cmd_name) {
            println!("{}", path.display());
            return Ok(success_result());
        }

        return Ok(error_result());
    }

    // -V: verbose description
    if very_verbose {
        // Check if it's a builtin
        if execute_builtin(cmd_name, &[], context).is_some() {
            println!("{} is a shell builtin", cmd_name);
            return Ok(success_result());
        }

        // Check if it's a function
        if context.functions.contains_key(cmd_name) {
            println!("{} is a function", cmd_name);
            return Ok(success_result());
        }

        // Check if it's an alias
        if context.aliases.contains_key(cmd_name) {
            if let Some(alias_val) = context.aliases.get(cmd_name) {
                println!("{} is aliased to `{}'", cmd_name, alias_val);
            }
            return Ok(success_result());
        }

        // Check if it's in PATH
        if let Some(path) = find_in_path(cmd_name) {
            println!("{} is {}", cmd_name, path.display());
            return Ok(success_result());
        }

        return Err(format!("{}: not found", cmd_name));
    }

    // Execute the command, bypassing functions and aliases
    // First check if it's a builtin
    if let Some(result) = execute_builtin(cmd_name, &cmd_args[1..], context) {
        return Ok(result);
    }

    // Find in PATH and execute
    let program_path = find_in_path(cmd_name)
        .ok_or_else(|| format!("{}: command not found", cmd_name))?;

    let mut command = std::process::Command::new(program_path);
    command.args(&cmd_args[1..]);

    // Execute
    #[cfg(unix)]
    {
        crate::terminal::unix::execute_with_terminal_control(command, false)
            .map_err(|e| e.to_string())
    }

    #[cfg(not(unix))]
    {
        crate::terminal::non_unix::execute_with_terminal_control(command, false)
            .map_err(|e| e.to_string())
    }
}

/// wait builtin - Wait for background jobs to complete
fn builtin_wait(args: &[String], context: &mut rush_expand::Context) -> ExecutionResult {
    #[cfg(unix)]
    {
        use nix::sys::wait::{waitpid, WaitPidFlag};
        use nix::unistd::Pid;

        // If no arguments, wait for all background jobs
        if args.is_empty() {
            let mut last_status = 0;

            // Get all job PIDs
            let job_pids: Vec<Pid> = context.job_list.jobs()
                .map(|job| job.pgid)
                .collect();

            for pgid in job_pids {
                match waitpid(pgid, Some(WaitPidFlag::empty())) {
                    Ok(status) => {
                        use nix::sys::wait::WaitStatus;
                        match status {
                            WaitStatus::Exited(_, code) => last_status = code,
                            WaitStatus::Signaled(_, sig, _) => last_status = 128 + sig as i32,
                            _ => {}
                        }
                    }
                    Err(_) => {}
                }
            }

            return exit_code_to_result(last_status);
        }

        // Wait for specific PIDs
        let mut last_status = 0;
        for arg in args {
            let pid = match arg.parse::<i32>() {
                Ok(p) => Pid::from_raw(p),
                Err(_) => {
                    eprintln!("wait: {}: not a valid process ID", arg);
                    return error_result();
                }
            };

            match waitpid(pid, Some(WaitPidFlag::empty())) {
                Ok(status) => {
                    use nix::sys::wait::WaitStatus;
                    match status {
                        WaitStatus::Exited(_, code) => last_status = code,
                        WaitStatus::Signaled(_, sig, _) => last_status = 128 + sig as i32,
                        _ => {}
                    }
                }
                Err(_) => {
                    eprintln!("wait: pid {}: no such job", pid);
                    return error_result();
                }
            }
        }

        exit_code_to_result(last_status)
    }

    #[cfg(not(unix))]
    {
        eprintln!("wait: job control not supported on this platform");
        error_result()
    }
}

/// kill builtin - Send signals to processes
fn builtin_kill(args: &[String], _context: &mut rush_expand::Context) -> ExecutionResult {
    #[cfg(unix)]
    {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        let mut signal = Signal::SIGTERM; // Default signal
        let mut pids = Vec::new();
        let mut list_signals = false;

        // Parse arguments
        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];

            if arg == "-l" {
                list_signals = true;
                i += 1;
                continue;
            }

            if arg.starts_with('-') && arg.len() > 1 {
                // Parse signal
                let sig_str = &arg[1..];

                // Try to parse as signal number
                if let Ok(num) = sig_str.parse::<i32>() {
                    signal = match Signal::try_from(num) {
                        Ok(sig) => sig,
                        Err(_) => {
                            eprintln!("kill: {}: invalid signal specification", num);
                            return error_result();
                        }
                    };
                } else {
                    // Try to parse as signal name
                    let sig_upper = sig_str.to_uppercase();
                    let sig_name = sig_upper.strip_prefix("SIG").unwrap_or(&sig_upper);

                    signal = match sig_name {
                        "HUP" | "1" => Signal::SIGHUP,
                        "INT" | "2" => Signal::SIGINT,
                        "QUIT" | "3" => Signal::SIGQUIT,
                        "ABRT" | "6" => Signal::SIGABRT,
                        "KILL" | "9" => Signal::SIGKILL,
                        "ALRM" | "14" => Signal::SIGALRM,
                        "TERM" | "15" => Signal::SIGTERM,
                        "USR1" | "10" => Signal::SIGUSR1,
                        "USR2" | "12" => Signal::SIGUSR2,
                        "CONT" | "18" => Signal::SIGCONT,
                        "STOP" | "19" => Signal::SIGSTOP,
                        "TSTP" | "20" => Signal::SIGTSTP,
                        _ => {
                            eprintln!("kill: {}: invalid signal specification", sig_str);
                            return error_result();
                        }
                    };
                }
                i += 1;
                continue;
            }

            // It's a PID
            pids.push(arg.clone());
            i += 1;
        }

        // Handle -l (list signals)
        if list_signals {
            println!(" 1) HUP\t 2) INT\t 3) QUIT\t 6) ABRT\t 9) KILL");
            println!("10) USR1\t12) USR2\t14) ALRM\t15) TERM");
            println!("18) CONT\t19) STOP\t20) TSTP");
            return success_result();
        }

        // Must have at least one PID
        if pids.is_empty() {
            eprintln!("kill: usage: kill [-s sigspec | -signum] pid ...");
            return error_result();
        }

        // Send signal to each PID
        let mut had_error = false;
        for pid_str in &pids {
            let pid = match pid_str.parse::<i32>() {
                Ok(p) => Pid::from_raw(p),
                Err(_) => {
                    eprintln!("kill: {}: arguments must be process or job IDs", pid_str);
                    had_error = true;
                    continue;
                }
            };

            if let Err(e) = kill(pid, signal) {
                eprintln!("kill: ({}): {}", pid, e);
                had_error = true;
            }
        }

        if had_error {
            error_result()
        } else {
            success_result()
        }
    }

    #[cfg(not(unix))]
    {
        eprintln!("kill: not supported on this platform");
        error_result()
    }
}

/// exec builtin - Replace shell with command
fn builtin_exec(args: &[String], _context: &mut rush_expand::Context) -> Result<ExecutionResult, String> {
    if args.is_empty() {
        return Err("usage: exec command [args...]".to_string());
    }

    let cmd_name = &args[0];
    let cmd_args = &args[1..];

    // Find the command in PATH
    let program_path = find_in_path(cmd_name)
        .ok_or_else(|| format!("{}: command not found", cmd_name))?;

    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        // Convert path to CString
        let path_cstr = CString::new(program_path.as_os_str().as_bytes())
            .map_err(|e| format!("invalid path: {}", e))?;

        // Convert args to CStrings
        let mut exec_args = vec![path_cstr.clone()];
        for arg in cmd_args {
            let arg_cstr = CString::new(arg.as_bytes())
                .map_err(|e| format!("invalid argument: {}", e))?;
            exec_args.push(arg_cstr);
        }

        // Execute - this replaces the current process
        // If exec succeeds, this function never returns
        nix::unistd::execv(&path_cstr, &exec_args)
            .map_err(|e| format!("exec failed: {}", e))?;

        // This should never be reached
        unreachable!()
    }

    #[cfg(not(unix))]
    {
        Err("exec: not supported on this platform".to_string())
    }
}

/// times builtin - Display process times
fn builtin_times(_args: &[String], _context: &mut rush_expand::Context) -> ExecutionResult {
    #[cfg(unix)]
    {
        use nix::sys::resource::{getrusage, UsageWho};

        // Get rusage for self (the shell)
        match getrusage(UsageWho::RUSAGE_SELF) {
            Ok(self_usage) => {
                let user_sec = self_usage.user_time().tv_sec();
                let user_usec = self_usage.user_time().tv_usec();
                let sys_sec = self_usage.system_time().tv_sec();
                let sys_usec = self_usage.system_time().tv_usec();

                // Get rusage for children
                match getrusage(UsageWho::RUSAGE_CHILDREN) {
                    Ok(child_usage) => {
                        let child_user_sec = child_usage.user_time().tv_sec();
                        let child_user_usec = child_usage.user_time().tv_usec();
                        let child_sys_sec = child_usage.system_time().tv_sec();
                        let child_sys_usec = child_usage.system_time().tv_usec();

                        println!("{}m{:.3}s {}m{:.3}s",
                                 user_sec / 60,
                                 (user_sec % 60) as f64 + user_usec as f64 / 1_000_000.0,
                                 sys_sec / 60,
                                 (sys_sec % 60) as f64 + sys_usec as f64 / 1_000_000.0);
                        println!("{}m{:.3}s {}m{:.3}s",
                                 child_user_sec / 60,
                                 (child_user_sec % 60) as f64 + child_user_usec as f64 / 1_000_000.0,
                                 child_sys_sec / 60,
                                 (child_sys_sec % 60) as f64 + child_sys_usec as f64 / 1_000_000.0);
                    }
                    Err(_) => {
                        eprintln!("times: failed to get child process times");
                        return error_result();
                    }
                }
            }
            Err(_) => {
                eprintln!("times: failed to get process times");
                return error_result();
            }
        }

        success_result()
    }

    #[cfg(not(unix))]
    {
        eprintln!("times: not supported on this platform");
        error_result()
    }
}

/// umask builtin - Set or display file creation mask
fn builtin_umask(args: &[String], _context: &mut rush_expand::Context) -> ExecutionResult {
    #[cfg(unix)]
    {
        use nix::sys::stat::{umask, Mode};

        let mut symbolic = false;

        // Parse options
        let mut mask_arg = None;
        for arg in args {
            if arg == "-S" {
                symbolic = true;
            } else if arg.starts_with('-') {
                eprintln!("umask: {}: invalid option", arg);
                return error_result();
            } else {
                mask_arg = Some(arg);
            }
        }

        if let Some(mask_str) = mask_arg {
            // Set new mask
            let new_mask = if mask_str.starts_with('0') {
                // Octal format
                match u32::from_str_radix(mask_str, 8) {
                    Ok(m) => Mode::from_bits_truncate(m),
                    Err(_) => {
                        eprintln!("umask: {}: invalid octal number", mask_str);
                        return error_result();
                    }
                }
            } else {
                // Decimal format (also support)
                match mask_str.parse::<u32>() {
                    Ok(m) => Mode::from_bits_truncate(m),
                    Err(_) => {
                        eprintln!("umask: {}: invalid number", mask_str);
                        return error_result();
                    }
                }
            };

            umask(new_mask);
        } else {
            // Display current mask
            // We need to set it twice to read the current value
            let current = umask(Mode::empty());
            umask(current);

            if symbolic {
                // Symbolic format: u=rwx,g=rwx,o=rwx
                let mode = current.bits();
                let u_r = if mode & 0o400 == 0 { 'r' } else { '-' };
                let u_w = if mode & 0o200 == 0 { 'w' } else { '-' };
                let u_x = if mode & 0o100 == 0 { 'x' } else { '-' };
                let g_r = if mode & 0o040 == 0 { 'r' } else { '-' };
                let g_w = if mode & 0o020 == 0 { 'w' } else { '-' };
                let g_x = if mode & 0o010 == 0 { 'x' } else { '-' };
                let o_r = if mode & 0o004 == 0 { 'r' } else { '-' };
                let o_w = if mode & 0o002 == 0 { 'w' } else { '-' };
                let o_x = if mode & 0o001 == 0 { 'x' } else { '-' };

                println!("u={}{}{},g={}{}{},o={}{}{}",
                         u_r, u_w, u_x, g_r, g_w, g_x, o_r, o_w, o_x);
            } else {
                // Octal format
                println!("{:04o}", current.bits());
            }
        }

        success_result()
    }

    #[cfg(not(unix))]
    {
        eprintln!("umask: not supported on this platform");
        error_result()
    }
}

/// hash builtin - Remember or report command locations
fn builtin_hash(args: &[String], context: &mut rush_expand::Context) -> ExecutionResult {
    let mut clear_hash = false;
    let mut print_all = false;
    let mut commands = Vec::new();

    // Parse arguments
    for arg in args {
        if arg == "-r" {
            clear_hash = true;
        } else if arg == "-l" {
            print_all = true;
        } else if arg.starts_with('-') {
            eprintln!("hash: {}: invalid option", arg);
            return error_result();
        } else {
            commands.push(arg.clone());
        }
    }

    // Clear hash table
    if clear_hash {
        context.command_hash.clear();
        return success_result();
    }

    // No arguments: display hash table
    if commands.is_empty() {
        if context.command_hash.is_empty() {
            // Nothing to display
            return success_result();
        }

        if print_all {
            // List format: path
            for (name, path) in &context.command_hash {
                println!("builtin hash -p {} {}", path, name);
            }
        } else {
            // Table format: hits command
            for (name, path) in &context.command_hash {
                println!("{}\t{}", name, path);
            }
        }
        return success_result();
    }

    // Add commands to hash table
    let mut had_error = false;
    for cmd_name in &commands {
        if let Some(path) = find_in_path(cmd_name) {
            context.command_hash.insert(cmd_name.clone(), path.display().to_string());
        } else {
            eprintln!("hash: {}: not found", cmd_name);
            had_error = true;
        }
    }

    if had_error {
        error_result()
    } else {
        success_result()
    }
}

/// getopts builtin - Parse utility options
fn builtin_getopts(args: &[String], context: &mut rush_expand::Context) -> ExecutionResult {
    if args.len() < 2 {
        eprintln!("getopts: usage: getopts optstring name [args...]");
        return error_result();
    }

    let optstring = &args[0];
    let var_name = &args[1];

    // Get arguments to parse (either from args[2..] or positional parameters)
    let parse_args: Vec<String> = if args.len() > 2 {
        args[2..].to_vec()
    } else {
        context.positional_params.clone()
    };

    // Check if we're done parsing
    if context.optind > parse_args.len() {
        // Reset OPTIND for next getopts call
        context.optind = 1;
        let _ = context.set_var("OPTIND", "1");
        return error_result(); // Return 1 to indicate done
    }

    let current_arg = &parse_args[context.optind - 1];

    // Check if this is an option
    if !current_arg.starts_with('-') || current_arg == "-" {
        // Not an option, we're done
        context.optind = 1;
        let _ = context.set_var("OPTIND", "1");
        return error_result();
    }

    // Check for end of options marker "--"
    if current_arg == "--" {
        context.optind += 1;
        let _ = context.set_var("OPTIND", context.optind.to_string());
        context.optind = 1;
        return error_result();
    }

    // Parse the option
    // For simplicity, we'll just handle single character options
    let opt_chars: Vec<char> = current_arg.chars().skip(1).collect();
    if opt_chars.is_empty() {
        context.optind += 1;
        let _ = context.set_var("OPTIND", context.optind.to_string());
        return error_result();
    }

    let opt_char = opt_chars[0];

    // Check if option is in optstring
    if let Some(pos) = optstring.find(opt_char) {
        // Set the variable to the option character
        let _ = context.set_var(var_name, opt_char.to_string());

        // Check if option requires an argument
        if pos + 1 < optstring.len() && optstring.chars().nth(pos + 1) == Some(':') {
            // Option requires an argument
            if opt_chars.len() > 1 {
                // Argument is in same word: -oARG
                let arg_value: String = opt_chars[1..].iter().collect();
                let _ = context.set_var("OPTARG", arg_value);
            } else if context.optind < parse_args.len() {
                // Argument is next word: -o ARG
                context.optind += 1;
                let arg_value = &parse_args[context.optind - 1];
                let _ = context.set_var("OPTARG", arg_value);
            } else {
                // Missing required argument
                if optstring.starts_with(':') {
                    // Silent mode: set var to ':', OPTARG to option char
                    let _ = context.set_var(var_name, ":");
                    let _ = context.set_var("OPTARG", opt_char.to_string());
                } else {
                    eprintln!("getopts: option requires an argument -- {}", opt_char);
                    let _ = context.set_var(var_name, "?");
                    let _ = context.set_var("OPTARG", opt_char.to_string());
                }
                context.optind += 1;
                let _ = context.set_var("OPTIND", context.optind.to_string());
                return success_result();
            }
        }

        context.optind += 1;
        let _ = context.set_var("OPTIND", context.optind.to_string());
        success_result()
    } else {
        // Invalid option
        if optstring.starts_with(':') {
            // Silent mode: set var to '?'
            let _ = context.set_var(var_name, "?");
            let _ = context.set_var("OPTARG", opt_char.to_string());
        } else {
            eprintln!("getopts: illegal option -- {}", opt_char);
            let _ = context.set_var(var_name, "?");
            let _ = context.set_var("OPTARG", opt_char.to_string());
        }
        context.optind += 1;
        let _ = context.set_var("OPTIND", context.optind.to_string());
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

#[cfg(unix)]
fn builtin_coproc(args: &[String], context: &mut rush_expand::Context) -> ExecutionResult {
    use nix::unistd::{fork, pipe, close, dup2, setpgid, ForkResult};
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::io::IntoRawFd;

    // Parse arguments: coproc [NAME] command [args...]
    let (name, command_args) = if args.is_empty() {
        eprintln!("coproc: usage: coproc [NAME] command [args...]");
        return error_result();
    } else if args.len() == 1 {
        // Single arg: could be just command with default name
        ("COPROC".to_string(), args.to_vec())
    } else {
        // Check if first arg is a valid name (starts with letter or underscore)
        let first = &args[0];
        if first.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false)
            && first.chars().all(|c| c.is_alphanumeric() || c == '_')
            && args.len() > 1
        {
            // First arg is name, rest is command
            (first.clone(), args[1..].to_vec())
        } else {
            // First arg is part of command, use default name
            ("COPROC".to_string(), args.to_vec())
        }
    };

    // Close any existing coproc
    context.clear_coproc();

    // Create two pipes: one for reading from coproc stdout, one for writing to coproc stdin
    let (read_fd_parent, write_fd_child) = match pipe() {
        Ok((r, w)) => (r.into_raw_fd(), w.into_raw_fd()),
        Err(e) => {
            eprintln!("coproc: failed to create pipe: {}", e);
            return error_result();
        }
    };

    let (read_fd_child, write_fd_parent) = match pipe() {
        Ok((r, w)) => (r.into_raw_fd(), w.into_raw_fd()),
        Err(e) => {
            eprintln!("coproc: failed to create pipe: {}", e);
            // close() is already unsafe, no need for unsafe block
            close(read_fd_parent).ok();
            close(write_fd_child).ok();
            return error_result();
        }
    };

    // Fork the child process
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            // Child process: set up pipes and exec command

            // Close parent ends of pipes
            close(read_fd_parent).ok();
            close(write_fd_parent).ok();

            // Redirect stdin from read_fd_child
            if let Err(e) = dup2(read_fd_child, 0) {
                eprintln!("coproc: failed to dup2 stdin: {}", e);
                std::process::exit(1);
            }
            close(read_fd_child).ok();

            // Redirect stdout to write_fd_child
            if let Err(e) = dup2(write_fd_child, 1) {
                eprintln!("coproc: failed to dup2 stdout: {}", e);
                std::process::exit(1);
            }
            close(write_fd_child).ok();

            // Find and execute the command
            let command_name = &command_args[0];
            let program_path = match find_in_path(command_name) {
                Some(path) => path,
                None => {
                    eprintln!("coproc: {}: command not found", command_name);
                    std::process::exit(127);
                }
            };

            // Build argument list for execvp
            let program_cstring = match CString::new(program_path.as_os_str().as_bytes()) {
                Ok(s) => s,
                Err(_) => std::process::exit(1),
            };

            let arg_cstrings: Vec<CString> = command_args.iter()
                .filter_map(|arg| CString::new(arg.as_bytes()).ok())
                .collect();

            let arg_ptrs: Vec<*const i8> = std::iter::once(program_cstring.as_ptr())
                .chain(arg_cstrings.iter().map(|s| s.as_ptr()))
                .chain(std::iter::once(std::ptr::null()))
                .collect();

            // Exec the command
            unsafe {
                nix::libc::execvp(program_cstring.as_ptr(), arg_ptrs.as_ptr());
            }

            // If execvp returns, it failed
            eprintln!("coproc: failed to exec {}", command_name);
            std::process::exit(127);
        }
        Ok(ForkResult::Parent { child }) => {
            // Parent process: close child ends of pipes and set up coproc state

            // Close child ends
            close(read_fd_child).ok();
            close(write_fd_child).ok();

            let pid = child;
            let pgid = pid; // Use PID as PGID (process becomes group leader)

            // Put child in its own process group
            if let Err(e) = setpgid(pid, pgid) {
                eprintln!("coproc: warning: failed to set process group: {}", e);
            }

            // Build command string for display
            let command_string = command_args.join(" ");

            // Store coproc state
            context.coproc = Some(rush_expand::CoprocState {
                name: name.clone(),
                read_fd: read_fd_parent,
                write_fd: write_fd_parent,
                pid,
                pgid,
            });

            // Set array variables: NAME[0] = read_fd, NAME[1] = write_fd
            context.set_coproc_vars(&name, read_fd_parent, write_fd_parent);

            // Add to job list
            let job_id = context.job_list.add_job(
                pgid,
                command_string.clone(),
                vec![pid],
                false, // not foreground
            );

            // Print job notification
            println!("[{}] {}", job_id, pid);

            success_result()
        }
        Err(e) => {
            eprintln!("coproc: fork failed: {}", e);
            // Clean up pipes
            close(read_fd_parent).ok();
            close(write_fd_parent).ok();
            close(read_fd_child).ok();
            close(write_fd_child).ok();
            error_result()
        }
    }
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

/// printf builtin - formatted output
fn builtin_printf(args: &[String], _context: &mut rush_expand::Context) -> ExecutionResult {
    if args.is_empty() {
        eprintln!("printf: usage: printf format [arguments]");
        return error_result();
    }

    let format = &args[0];
    let arguments = &args[1..];
    let mut arg_index = 0;

    let mut output = String::new();
    let mut chars = format.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            // Handle escape sequences
            match chars.next() {
                Some('n') => output.push('\n'),
                Some('t') => output.push('\t'),
                Some('r') => output.push('\r'),
                Some('\\') => output.push('\\'),
                Some('"') => output.push('"'),
                Some('\'') => output.push('\''),
                Some('a') => output.push('\x07'), // bell
                Some('b') => output.push('\x08'), // backspace
                Some('f') => output.push('\x0C'), // form feed
                Some('v') => output.push('\x0B'), // vertical tab
                Some('0') => {
                    // Octal escape \0nnn
                    let mut oct = String::new();
                    for _ in 0..3 {
                        if let Some(&ch) = chars.peek() {
                            if ch >= '0' && ch <= '7' {
                                oct.push(chars.next().unwrap());
                            } else {
                                break;
                            }
                        }
                    }
                    if oct.is_empty() {
                        output.push('\0');
                    } else {
                        let val = u8::from_str_radix(&oct, 8).unwrap_or(0);
                        output.push(val as char);
                    }
                }
                Some('x') => {
                    // Hex escape \xHH
                    let mut hex = String::new();
                    for _ in 0..2 {
                        if let Some(&ch) = chars.peek() {
                            if ch.is_ascii_hexdigit() {
                                hex.push(chars.next().unwrap());
                            } else {
                                break;
                            }
                        }
                    }
                    if !hex.is_empty() {
                        let val = u8::from_str_radix(&hex, 16).unwrap_or(0);
                        output.push(val as char);
                    }
                }
                Some(other) => {
                    output.push('\\');
                    output.push(other);
                }
                None => output.push('\\'),
            }
        } else if c == '%' {
            // Handle format specifiers
            match chars.peek() {
                Some('%') => {
                    chars.next();
                    output.push('%');
                }
                _ => {
                    // Parse format specifier: %[flags][width][.precision]specifier
                    let mut spec = String::from('%');

                    // Flags
                    while let Some(&ch) = chars.peek() {
                        if ch == '-' || ch == '+' || ch == ' ' || ch == '#' || ch == '0' {
                            spec.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }

                    // Width
                    while let Some(&ch) = chars.peek() {
                        if ch.is_ascii_digit() {
                            spec.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }

                    // Precision
                    if chars.peek() == Some(&'.') {
                        spec.push(chars.next().unwrap());
                        while let Some(&ch) = chars.peek() {
                            if ch.is_ascii_digit() {
                                spec.push(chars.next().unwrap());
                            } else {
                                break;
                            }
                        }
                    }

                    // Specifier
                    let specifier = chars.next().unwrap_or('s');
                    let arg = arguments.get(arg_index).map(|s| s.as_str()).unwrap_or("");
                    arg_index += 1;

                    match specifier {
                        's' => {
                            // String - handle width/precision
                            output.push_str(arg);
                        }
                        'd' | 'i' => {
                            let val: i64 = arg.parse().unwrap_or(0);
                            output.push_str(&val.to_string());
                        }
                        'u' => {
                            let val: u64 = arg.parse().unwrap_or(0);
                            output.push_str(&val.to_string());
                        }
                        'o' => {
                            let val: u64 = arg.parse().unwrap_or(0);
                            output.push_str(&format!("{:o}", val));
                        }
                        'x' => {
                            let val: u64 = arg.parse().unwrap_or(0);
                            output.push_str(&format!("{:x}", val));
                        }
                        'X' => {
                            let val: u64 = arg.parse().unwrap_or(0);
                            output.push_str(&format!("{:X}", val));
                        }
                        'f' | 'F' | 'e' | 'E' | 'g' | 'G' => {
                            let val: f64 = arg.parse().unwrap_or(0.0);
                            output.push_str(&format!("{}", val));
                        }
                        'c' => {
                            if let Some(ch) = arg.chars().next() {
                                output.push(ch);
                            }
                        }
                        'b' => {
                            // %b - interpret backslash escapes in the argument
                            let mut arg_chars = arg.chars().peekable();
                            while let Some(ac) = arg_chars.next() {
                                if ac == '\\' {
                                    match arg_chars.next() {
                                        Some('n') => output.push('\n'),
                                        Some('t') => output.push('\t'),
                                        Some('r') => output.push('\r'),
                                        Some('\\') => output.push('\\'),
                                        Some(other) => {
                                            output.push('\\');
                                            output.push(other);
                                        }
                                        None => output.push('\\'),
                                    }
                                } else {
                                    output.push(ac);
                                }
                            }
                        }
                        'q' => {
                            // %q - quote the argument for shell reuse
                            output.push('\'');
                            for ch in arg.chars() {
                                if ch == '\'' {
                                    output.push_str("'\\''");
                                } else {
                                    output.push(ch);
                                }
                            }
                            output.push('\'');
                        }
                        _ => {
                            // Unknown specifier, output as-is
                            output.push('%');
                            output.push(specifier);
                        }
                    }
                }
            }
        } else {
            output.push(c);
        }
    }

    print!("{}", output);
    success_result()
}

/// mapfile/readarray builtin - read lines into an array
fn builtin_mapfile(args: &[String], context: &mut rush_expand::Context) -> ExecutionResult {
    use std::io::{self, BufRead, Read};

    let mut delimiter = '\n';
    let mut count: Option<usize> = None;
    let mut origin = 0usize;
    let mut skip = 0usize;
    let mut remove_delimiter = false;
    let mut array_name = String::from("MAPFILE");

    // Parse options
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-d" => {
                i += 1;
                if i < args.len() {
                    delimiter = args[i].chars().next().unwrap_or('\n');
                }
            }
            "-n" => {
                i += 1;
                if i < args.len() {
                    count = args[i].parse().ok();
                }
            }
            "-O" => {
                i += 1;
                if i < args.len() {
                    origin = args[i].parse().unwrap_or(0);
                }
            }
            "-s" => {
                i += 1;
                if i < args.len() {
                    skip = args[i].parse().unwrap_or(0);
                }
            }
            "-t" => {
                remove_delimiter = true;
            }
            arg if !arg.starts_with('-') => {
                array_name = arg.to_string();
            }
            _ => {
                // Unknown option, skip
            }
        }
        i += 1;
    }

    let stdin = io::stdin();
    let reader = stdin.lock();
    let mut lines: Vec<String> = Vec::new();
    let mut lines_read = 0usize;

    if delimiter == '\n' {
        for line_result in reader.lines() {
            match line_result {
                Ok(line) => {
                    if lines_read < skip {
                        lines_read += 1;
                        continue;
                    }

                    if let Some(max) = count {
                        if lines.len() >= max {
                            break;
                        }
                    }

                    let content = if remove_delimiter {
                        line
                    } else {
                        format!("{}\n", line)
                    };
                    lines.push(content);
                    lines_read += 1;
                }
                Err(_) => break,
            }
        }
    } else {
        // Custom delimiter
        let mut buffer = String::new();
        for byte_result in reader.bytes() {
            match byte_result {
                Ok(byte) => {
                    let ch = byte as char;
                    if ch == delimiter {
                        if lines_read < skip {
                            buffer.clear();
                            lines_read += 1;
                            continue;
                        }

                        if let Some(max) = count {
                            if lines.len() >= max {
                                break;
                            }
                        }

                        let content = if remove_delimiter {
                            buffer.clone()
                        } else {
                            format!("{}{}", buffer, delimiter)
                        };
                        lines.push(content);
                        buffer.clear();
                        lines_read += 1;
                    } else {
                        buffer.push(ch);
                    }
                }
                Err(_) => break,
            }
        }
        // Handle remaining content
        if !buffer.is_empty() && (count.is_none() || lines.len() < count.unwrap()) {
            if lines_read >= skip {
                lines.push(buffer);
            }
        }
    }

    // Build array starting at origin index
    let mut array_values: Vec<String> = Vec::new();

    // Preserve existing elements if origin > 0
    if origin > 0 {
        if let Some(rush_expand::context::ArrayType::Indexed(existing)) = context.arrays.get(&array_name) {
            for i in 0..origin {
                array_values.push(existing.get(i).cloned().unwrap_or_default());
            }
        } else {
            for _ in 0..origin {
                array_values.push(String::new());
            }
        }
    }

    array_values.extend(lines);

    context.arrays.insert(array_name, rush_expand::context::ArrayType::Indexed(array_values));

    success_result()
}

/// disown builtin - remove jobs from job table
#[cfg(unix)]
fn builtin_disown(args: &[String], context: &mut rush_expand::Context) -> ExecutionResult {
    let mut _mark_no_sighup = false; // TODO: implement -h to prevent SIGHUP
    let mut all_jobs = false;
    let mut running_only = false;
    let mut job_specs: Vec<String> = Vec::new();

    // Parse options
    for arg in args {
        match arg.as_str() {
            "-h" => _mark_no_sighup = true,
            "-a" => all_jobs = true,
            "-r" => running_only = true,
            _ if arg.starts_with('%') || arg.parse::<u32>().is_ok() => {
                job_specs.push(arg.clone());
            }
            _ => {
                eprintln!("disown: {}: invalid option", arg);
                return error_result();
            }
        }
    }

    // If -a flag, disown all jobs
    if all_jobs {
        context.job_list.disown_all(running_only);
        return success_result();
    }

    // If no job specs and no -a, disown current job
    if job_specs.is_empty() {
        if let Some(current_job) = context.job_list.current_job_id() {
            context.job_list.disown_job(current_job);
        }
        return success_result();
    }

    // Disown specific jobs
    for spec in job_specs {
        let job_id = if spec.starts_with('%') {
            // Job ID format: %n or %+, %-, etc.
            let id_str = &spec[1..];
            if id_str == "+" || id_str == "%" {
                context.job_list.current_job_id()
            } else if id_str == "-" {
                context.job_list.previous_job()
            } else {
                id_str.parse::<u32>().ok()
            }
        } else {
            spec.parse::<u32>().ok()
        };

        if let Some(id) = job_id {
            context.job_list.disown_job(id);
        } else {
            eprintln!("disown: {}: no such job", spec);
        }
    }

    success_result()
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
