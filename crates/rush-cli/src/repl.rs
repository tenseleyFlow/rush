use reedline::{
    ColumnarMenu, Emacs, FileBackedHistory,
    KeyCode, KeyModifiers, MenuBuilder, Reedline, ReedlineEvent, ReedlineMenu, Signal,
    default_emacs_keybindings,
};
use rush_expand::Context;
use rush_interactive::{RushCompleter, RushHighlighter, RushHinter, RushPrompt};
use std::process::ExitCode;

/// Expand history references in a command line
///
/// Supports:
/// - `!!` - previous command
/// - `!$` - last argument of previous command
/// - `!^` - first argument of previous command
/// - `!*` - all arguments of previous command
/// - `!-n` - nth previous command
/// - `!n` - command at history index n
/// - `!string` - most recent command starting with string
fn expand_history(input: &str, history: &[String]) -> Result<String, String> {
    if !input.contains('!') {
        return Ok(input.to_string());
    }

    let mut result = String::new();
    let mut chars = input.chars().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while let Some(c) = chars.next() {
        match c {
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
                result.push(c);
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
                result.push(c);
            }
            '!' if !in_single_quote => {
                // Check for escaped !
                if let Some(&next) = chars.peek() {
                    match next {
                        '!' => {
                            // !! - previous command
                            chars.next();
                            if history.is_empty() {
                                return Err("!!: event not found".to_string());
                            }
                            result.push_str(&history[history.len() - 1]);
                        }
                        '$' => {
                            // !$ - last argument of previous command
                            chars.next();
                            if history.is_empty() {
                                return Err("!$: event not found".to_string());
                            }
                            let prev = &history[history.len() - 1];
                            let args: Vec<&str> = prev.split_whitespace().collect();
                            if let Some(last) = args.last() {
                                result.push_str(last);
                            }
                        }
                        '^' => {
                            // !^ - first argument of previous command
                            chars.next();
                            if history.is_empty() {
                                return Err("!^: event not found".to_string());
                            }
                            let prev = &history[history.len() - 1];
                            let args: Vec<&str> = prev.split_whitespace().collect();
                            if args.len() > 1 {
                                result.push_str(args[1]);
                            }
                        }
                        '*' => {
                            // !* - all arguments of previous command
                            chars.next();
                            if history.is_empty() {
                                return Err("!*: event not found".to_string());
                            }
                            let prev = &history[history.len() - 1];
                            let args: Vec<&str> = prev.split_whitespace().collect();
                            if args.len() > 1 {
                                result.push_str(&args[1..].join(" "));
                            }
                        }
                        '-' => {
                            // !-n - nth previous command
                            chars.next();
                            let mut num_str = String::new();
                            while let Some(&ch) = chars.peek() {
                                if ch.is_ascii_digit() {
                                    num_str.push(chars.next().unwrap());
                                } else {
                                    break;
                                }
                            }
                            if let Ok(n) = num_str.parse::<usize>() {
                                if n > 0 && n <= history.len() {
                                    result.push_str(&history[history.len() - n]);
                                } else {
                                    return Err(format!("!-{}: event not found", n));
                                }
                            } else {
                                return Err(format!("!-{}: event not found", num_str));
                            }
                        }
                        d if d.is_ascii_digit() => {
                            // !n - command at index n
                            let mut num_str = String::new();
                            while let Some(&ch) = chars.peek() {
                                if ch.is_ascii_digit() {
                                    num_str.push(chars.next().unwrap());
                                } else {
                                    break;
                                }
                            }
                            if let Ok(n) = num_str.parse::<usize>() {
                                if n > 0 && n <= history.len() {
                                    result.push_str(&history[n - 1]);
                                } else {
                                    return Err(format!("!{}: event not found", n));
                                }
                            } else {
                                return Err(format!("!{}: event not found", num_str));
                            }
                        }
                        ch if ch.is_alphanumeric() || ch == '_' => {
                            // !string - most recent command starting with string
                            let mut prefix = String::new();
                            while let Some(&ch) = chars.peek() {
                                if ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
                                    prefix.push(chars.next().unwrap());
                                } else {
                                    break;
                                }
                            }
                            // Find most recent command starting with prefix
                            if let Some(cmd) = history.iter().rev().find(|h| h.starts_with(&prefix)) {
                                result.push_str(cmd);
                            } else {
                                return Err(format!("!{}: event not found", prefix));
                            }
                        }
                        ' ' | '\t' | '\n' => {
                            // Standalone ! followed by whitespace - literal
                            result.push('!');
                        }
                        _ => {
                            // Unknown ! sequence - keep as is
                            result.push('!');
                        }
                    }
                } else {
                    // ! at end of line - literal
                    result.push('!');
                }
            }
            _ => {
                result.push(c);
            }
        }
    }

    Ok(result)
}

/// Get history as a vector of strings from reedline history
fn get_history_strings(line_editor: &Reedline) -> Vec<String> {
    let history = line_editor.history();

    // Try to get all history items (no session filtering)
    match history.search(reedline::SearchQuery::last_with_search(
        reedline::SearchFilter::anything(None),
    )) {
        Ok(items) => items.into_iter().map(|item| item.command_line).collect(),
        Err(_) => Vec::new(),
    }
}

/// Default config file contents
const DEFAULT_CONFIG: &str = r#"# Rush shell configuration
# This file is sourced on interactive shell startup

# Example aliases
# alias ll='ls -la'
# alias la='ls -A'

# Example prompt customization (uncomment to use)
# export PS1='\u@\h:\w\$ '
# export PS1_RIGHT='\D{%m/%d/%Y %I:%M:%S %p}'

# Example environment variables
# export EDITOR=vim
"#;

/// Check if a config file exists
fn config_exists() -> bool {
    let config_paths = [
        dirs::home_dir().map(|p| p.join(".rushrc")),
        dirs::config_dir().map(|p| p.join("rush").join("rushrc")),
    ];

    config_paths.iter().flatten().any(|p| p.exists())
}

/// Prompt user to create default config on first run
fn maybe_create_default_config() {
    if config_exists() {
        return;
    }

    // Check if we've already asked (store a marker file)
    let marker_path = dirs::data_dir().map(|p| p.join("rush").join(".config_prompted"));
    if let Some(ref marker) = marker_path {
        if marker.exists() {
            return;
        }
    }

    println!("Welcome to Rush! No configuration file found.");
    print!("Create default config at ~/.rushrc? [Y/n] ");

    // Flush stdout to ensure prompt is displayed
    use std::io::Write;
    std::io::stdout().flush().ok();

    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_ok() {
        let response = input.trim().to_lowercase();
        if response.is_empty() || response == "y" || response == "yes" {
            // Create the config file
            if let Some(config_path) = dirs::home_dir().map(|p| p.join(".rushrc")) {
                match std::fs::write(&config_path, DEFAULT_CONFIG) {
                    Ok(_) => println!("Created {}. Edit it to customize your shell!", config_path.display()),
                    Err(e) => eprintln!("rush: could not create config: {}", e),
                }
            }
        } else {
            println!("Skipped. You can create ~/.rushrc manually anytime.");
        }
    }

    // Create marker so we don't ask again
    if let Some(marker) = marker_path {
        if let Some(parent) = marker.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(marker, "").ok();
    }

    println!();
}

/// Source config files on interactive shell startup
fn source_config_files(context: &mut Context) {
    let config_paths = [
        dirs::home_dir().map(|p| p.join(".rushrc")),
        dirs::config_dir().map(|p| p.join("rush").join("rushrc")),
    ];

    for path_opt in config_paths {
        if let Some(path) = path_opt {
            if path.exists() {
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        // Filter out comment-only and empty lines, then parse
                        let executable_lines: Vec<&str> = content
                            .lines()
                            .filter(|line| {
                                let trimmed = line.trim();
                                !trimmed.is_empty() && !trimmed.starts_with('#')
                            })
                            .collect();

                        // Skip if no executable content
                        if executable_lines.is_empty() {
                            break;
                        }

                        let filtered_content = executable_lines.join("\n");

                        // Parse and execute the config file
                        use rush_parser::parse_line;
                        match parse_line(&filtered_content) {
                            Ok(statement) => {
                                if let Err(e) = rush_executor::execute_statement(&statement, context) {
                                    eprintln!("rush: error in {:?}: {}", path, e);
                                }
                            }
                            Err(e) => {
                                eprintln!("rush: parse error in {:?}: {}", path, e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("rush: could not read {:?}: {}", path, e);
                    }
                }
                break; // Only source first found config
            }
        }
    }
}

/// Source login shell profile files
fn source_login_profiles(context: &mut Context) {
    let profile_paths = [
        // System-wide profile
        Some(std::path::PathBuf::from("/etc/profile")),
        // User profile (traditional)
        dirs::home_dir().map(|p| p.join(".profile")),
        // Rush-specific login profile
        dirs::home_dir().map(|p| p.join(".rush_profile")),
    ];

    for path_opt in profile_paths {
        if let Some(path) = path_opt {
            if path.exists() {
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        // Filter comments and empty lines
                        let executable_lines: Vec<&str> = content
                            .lines()
                            .filter(|line| {
                                let trimmed = line.trim();
                                !trimmed.is_empty() && !trimmed.starts_with('#')
                            })
                            .collect();

                        if executable_lines.is_empty() {
                            continue;
                        }

                        let filtered_content = executable_lines.join("\n");

                        use rush_parser::parse_line;
                        match parse_line(&filtered_content) {
                            Ok(statement) => {
                                if let Err(e) = rush_executor::execute_statement(&statement, context) {
                                    eprintln!("rush: error in {:?}: {}", path, e);
                                }
                            }
                            Err(e) => {
                                eprintln!("rush: parse error in {:?}: {}", path, e);
                            }
                        }
                    }
                    Err(e) => {
                        // Only warn for user profiles, not system ones
                        if path.starts_with(dirs::home_dir().unwrap_or_default()) {
                            eprintln!("rush: could not read {:?}: {}", path, e);
                        }
                    }
                }
            }
        }
    }
}

pub fn run_interactive(is_login: bool) -> ExitCode {
    // On first run, offer to create default config
    maybe_create_default_config();

    // Set up persistent history
    let history_path = dirs::data_dir()
        .map(|mut path| {
            path.push("rush");
            if let Err(e) = std::fs::create_dir_all(&path) {
                eprintln!("rush: warning: could not create data directory: {}", e);
            }
            path.push("history.txt");
            path
        });

    let history_file = history_path.as_ref().and_then(|path| {
        match FileBackedHistory::with_file(1000, path.clone()) {
            Ok(history) => Some(history),
            Err(e) => {
                eprintln!("rush: warning: could not load history from {:?}: {}", path, e);
                None
            }
        }
    });

    // Create the completion menu with custom marker (instead of default "|")
    let completion_menu = Box::new(
        ColumnarMenu::default()
            .with_name("completion_menu")
            .with_marker("› ")
    );

    // Set up keybindings with Tab completion and arrow key navigation
    let mut keybindings = default_emacs_keybindings();

    // Tab opens menu or cycles through completions
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );

    // Shift+Tab cycles backwards
    keybindings.add_binding(
        KeyModifiers::SHIFT,
        KeyCode::BackTab,
        ReedlineEvent::MenuPrevious,
    );

    // Arrow keys navigate the completion menu (fish-style)
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Down,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::MenuDown,
            ReedlineEvent::Down,
        ]),
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Up,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::MenuUp,
            ReedlineEvent::Up,
        ]),
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Left,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::MenuLeft,
            ReedlineEvent::Left,
        ]),
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Right,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::MenuRight,
            ReedlineEvent::HistoryHintComplete,  // Accept hint if at end of line
            ReedlineEvent::Right,
        ]),
    );

    // Enter executes command
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Enter,
        ReedlineEvent::Submit,
    );

    let mut line_editor = Reedline::create()
        .with_highlighter(Box::new(RushHighlighter::new()))
        .with_hinter(Box::new(RushHinter::new()))
        .with_completer(Box::new(RushCompleter::new()))
        .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
        .with_edit_mode(Box::new(Emacs::new(keybindings)));

    // Add history if we successfully created it
    if let Some(history) = history_file {
        line_editor = line_editor.with_history(Box::new(history));
    }

    let prompt = RushPrompt::new();
    let mut context = crate::create_context();

    // Source login profile files if this is a login shell
    if is_login {
        source_login_profiles(&mut context);
    }

    // Source config files (~/.rushrc or ~/.config/rush/rushrc)
    source_config_files(&mut context);

    loop {
        // Check for SIGHUP (terminal hangup)
        #[cfg(unix)]
        if rush_job::check_sighup() {
            // Send SIGHUP to all jobs and exit
            context.job_list.send_hup_to_all();
            break;
        }

        // Check for completed/stopped background jobs before each prompt
        #[cfg(unix)]
        crate::check_background_jobs(&mut context);

        let sig = line_editor.read_line(&prompt);

        match sig {
            Ok(Signal::Success(buffer)) => {
                if buffer.trim().is_empty() {
                    continue;
                }

                // Perform history expansion if the line contains !
                let expanded_buffer = if buffer.contains('!') {
                    let history = get_history_strings(&line_editor);
                    match expand_history(&buffer, &history) {
                        Ok(expanded) => {
                            // If expansion changed the line, print it (bash behavior)
                            if expanded != buffer {
                                println!("{}", expanded);
                            }
                            expanded
                        }
                        Err(e) => {
                            eprintln!("rush: {}", e);
                            continue;
                        }
                    }
                } else {
                    buffer
                };

                if let Err(e) = crate::execute_interactive_line(&expanded_buffer, &mut context) {
                    eprintln!("rush: {}", e);
                }

                // Check if exit was requested
                if context.exit_requested.is_some() {
                    break;
                }
            }
            Ok(Signal::CtrlD) | Ok(Signal::CtrlC) => {
                break;
            }
            Err(e) => {
                eprintln!("rush: error: {}", e);
                // Sync history before returning on error
                if let Err(e) = line_editor.sync_history() {
                    eprintln!("rush: warning: failed to save history: {}", e);
                }
                // Restore terminal attributes
                #[cfg(unix)]
                if let Err(e) = rush_job::restore_terminal_attrs() {
                    eprintln!("rush: warning: failed to restore terminal: {}", e);
                }
                return ExitCode::from(1);
            }
        }
    }

    // Sync history before exiting
    if let Err(e) = line_editor.sync_history() {
        eprintln!("rush: warning: failed to save history: {}", e);
    }

    // Restore terminal attributes to their original state
    #[cfg(unix)]
    if let Err(e) = rush_job::restore_terminal_attrs() {
        eprintln!("rush: warning: failed to restore terminal: {}", e);
    }

    // Return requested exit code, or SUCCESS
    match context.exit_requested {
        Some(code) => ExitCode::from(code as u8),
        None => ExitCode::SUCCESS,
    }
}
