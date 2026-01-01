use nu_ansi_term::{Color, Style};
use reedline::{Highlighter, StyledText};
use std::env;
use std::path::PathBuf;

/// Syntax highlighter for Rush shell
///
/// Provides fish-like syntax highlighting:
/// - Valid commands in green
/// - Invalid commands in red
/// - Keywords (if, while, for, etc.) in bold cyan
/// - Operators (|, &, &&, ||, etc.) in yellow
/// - Strings in green
/// - Variables in cyan
pub struct RushHighlighter;

impl RushHighlighter {
    pub fn new() -> Self {
        Self
    }

    /// Check if a command exists in PATH
    fn command_exists(&self, command: &str) -> bool {
        // Built-in commands always exist
        if Self::is_builtin(command) {
            return true;
        }

        // Check if it's a path (contains /)
        if command.contains('/') {
            let path = PathBuf::from(command);
            return path.exists() && Self::is_executable(&path);
        }

        // Search in PATH
        if let Some(path_var) = env::var_os("PATH") {
            for dir in env::split_paths(&path_var) {
                let candidate = dir.join(command);
                if candidate.exists() && Self::is_executable(&candidate) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a command is a shell built-in
    fn is_builtin(command: &str) -> bool {
        matches!(
            command,
            "cd" | "pwd" | "exit" | "true" | "false" | "test" | "[" | ":"
                | "eval" | "alias" | "unalias" | "trap" | "set" | "shopt"
                | "export" | "unset" | "readonly" | "declare" | "typeset" | "local"
                | "read" | "shift" | "wait" | "kill" | "times" | "umask" | "hash"
                | "getopts" | "exec" | "command" | "jobs" | "fg" | "bg"
                | "coproc" | "disown" | "printf" | "mapfile" | "readarray"
                | "break" | "continue" | "return" | "source" | "."
                | "complete" | "pushd" | "popd" | "dirs"
        )
    }

    /// Check if a command is a shell keyword
    fn is_keyword(word: &str) -> bool {
        matches!(
            word,
            "if" | "then" | "else" | "elif" | "fi"
                | "while" | "do" | "done"
                | "for" | "in"
                | "case" | "esac"
        )
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
        true
    }
}

impl Default for RushHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter for RushHighlighter {
    fn highlight(&self, line: &str, _cursor: usize) -> StyledText {
        let mut styled = StyledText::new();

        // Simple tokenization for now
        // This is a basic implementation - we'll improve it later
        let mut current_word = String::new();
        let mut is_first_word = true;
        let mut in_string = false;
        let mut string_char = ' ';

        for ch in line.chars() {
            // Handle strings
            if (ch == '"' || ch == '\'') && !in_string {
                // Start of string
                if !current_word.is_empty() {
                    Self::push_word(&mut styled, &current_word, is_first_word, self);
                    current_word.clear();
                    is_first_word = false;
                }
                in_string = true;
                string_char = ch;
                current_word.push(ch);
            } else if in_string && ch == string_char {
                // End of string
                current_word.push(ch);
                styled.push((Style::new().fg(Color::Green), current_word.clone()));
                current_word.clear();
                in_string = false;
            } else if in_string {
                // Inside string
                current_word.push(ch);
            } else if ch.is_whitespace() {
                // Whitespace - flush current word
                if !current_word.is_empty() {
                    Self::push_word(&mut styled, &current_word, is_first_word, self);
                    current_word.clear();
                    is_first_word = false;
                }
                styled.push((Style::default(), ch.to_string()));
            } else if ch == '|' || ch == '&' || ch == '>' || ch == '<' || ch == ';' {
                // Operators
                if !current_word.is_empty() {
                    Self::push_word(&mut styled, &current_word, is_first_word, self);
                    current_word.clear();
                    is_first_word = false;
                }
                current_word.push(ch);
                // Check for multi-char operators (||, &&, >>, etc.)
            } else if current_word.chars().all(|c| c == '|' || c == '&' || c == '>' || c == '<') {
                // Continue operator
                current_word.push(ch);
            } else {
                // Check if we need to flush an operator
                if current_word.chars().all(|c| c == '|' || c == '&' || c == '>' || c == '<') && !current_word.is_empty() {
                    styled.push((Style::new().fg(Color::Yellow), current_word.clone()));
                    current_word.clear();
                }
                current_word.push(ch);
            }
        }

        // Flush any remaining content
        if !current_word.is_empty() {
            if in_string {
                // Unclosed string
                styled.push((Style::new().fg(Color::Red), current_word));
            } else if current_word.chars().all(|c| c == '|' || c == '&' || c == '>' || c == '<') {
                styled.push((Style::new().fg(Color::Yellow), current_word));
            } else {
                Self::push_word(&mut styled, &current_word, is_first_word, self);
            }
        }

        styled
    }
}

impl RushHighlighter {
    /// Push a word with appropriate styling
    fn push_word(styled: &mut StyledText, word: &str, is_command: bool, highlighter: &RushHighlighter) {
        // Check for variables
        if word.starts_with('$') {
            styled.push((Style::new().fg(Color::Cyan), word.to_string()));
            return;
        }

        // Check for keywords
        if Self::is_keyword(word) {
            styled.push((Style::new().fg(Color::Cyan).bold(), word.to_string()));
            return;
        }

        // Check if it's a command (first word)
        if is_command {
            if highlighter.command_exists(word) {
                styled.push((Style::new().fg(Color::Green), word.to_string()));
            } else {
                styled.push((Style::new().fg(Color::Red), word.to_string()));
            }
            return;
        }

        // Default: no styling
        styled.push((Style::default(), word.to_string()));
    }
}
