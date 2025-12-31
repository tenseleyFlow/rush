use reedline::{Completer, Span, Suggestion};
use std::env;
use std::fs;
use std::path::PathBuf;

/// Smart tab completer for Rush shell
///
/// Provides context-aware completions:
/// - Command names from PATH (when first word)
/// - File/directory names (for arguments)
/// - Variable names (when typing $VAR)
pub struct RushCompleter;

impl RushCompleter {
    pub fn new() -> Self {
        Self
    }

    /// Get all executable commands from PATH
    fn get_commands_from_path() -> Vec<String> {
        let mut commands = Vec::new();

        if let Some(path_var) = env::var_os("PATH") {
            for dir in env::split_paths(&path_var) {
                if let Ok(entries) = fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        if let Ok(file_type) = entry.file_type() {
                            if file_type.is_file() {
                                if let Ok(metadata) = entry.metadata() {
                                    #[cfg(unix)]
                                    {
                                        use std::os::unix::fs::PermissionsExt;
                                        if metadata.permissions().mode() & 0o111 != 0 {
                                            if let Some(name) = entry.file_name().to_str() {
                                                commands.push(name.to_string());
                                            }
                                        }
                                    }
                                    #[cfg(not(unix))]
                                    {
                                        if let Some(name) = entry.file_name().to_str() {
                                            commands.push(name.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Add built-in commands
        commands.extend_from_slice(&[
            "cd".to_string(),
            "pwd".to_string(),
            "exit".to_string(),
            "jobs".to_string(),
            "fg".to_string(),
            "bg".to_string(),
            "test".to_string(),
        ]);

        // Sort and deduplicate
        commands.sort();
        commands.dedup();
        commands
    }

    /// Get file/directory completions for a partial path
    fn get_file_completions(partial: &str) -> Vec<String> {
        let mut completions = Vec::new();

        // Determine the directory to search and the prefix to match
        let (search_dir, prefix) = if partial.contains('/') {
            let path = PathBuf::from(partial);
            let parent = path.parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .to_path_buf();
            let file_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            (parent, file_name)
        } else {
            (PathBuf::from("."), partial.to_string())
        };

        // Read directory and find matches
        if let Ok(entries) = fs::read_dir(&search_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with(&prefix) {
                        // Build the full completion
                        let mut completion = if partial.contains('/') {
                            let parent_path = PathBuf::from(partial);
                            let parent = parent_path
                                .parent()
                                .unwrap_or_else(|| std::path::Path::new("."));
                            parent.join(name)
                                .to_string_lossy()
                                .to_string()
                        } else {
                            name.to_string()
                        };

                        // Add trailing slash for directories
                        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                            completion.push('/');
                        }

                        completions.push(completion);
                    }
                }
            }
        }

        completions.sort();
        completions
    }

    /// Check if we're completing the first word (command name)
    fn is_first_word(line: &str, pos: usize) -> bool {
        let before_cursor = &line[..pos];
        !before_cursor.contains(char::is_whitespace)
    }

    /// Get the partial word being completed
    fn get_partial_word(line: &str, pos: usize) -> (usize, &str) {
        let before_cursor = &line[..pos];
        let start = before_cursor
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);
        (start, &line[start..pos])
    }
}

impl Default for RushCompleter {
    fn default() -> Self {
        Self::new()
    }
}

impl Completer for RushCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let (start, partial) = Self::get_partial_word(line, pos);

        // Skip empty completions
        if partial.is_empty() {
            return vec![];
        }

        let span = Span::new(start, pos);
        let mut suggestions = Vec::new();

        if Self::is_first_word(line, pos) {
            // Complete command names
            for cmd in Self::get_commands_from_path() {
                if cmd.starts_with(partial) {
                    suggestions.push(Suggestion {
                        value: cmd.clone(),
                        description: None,
                        style: None,
                        extra: None,
                        span,
                        append_whitespace: true,
                    });
                }
            }
        } else {
            // Complete file/directory names
            for file in Self::get_file_completions(partial) {
                suggestions.push(Suggestion {
                    value: file.clone(),
                    description: None,
                    style: None,
                    extra: None,
                    span,
                    append_whitespace: !file.ends_with('/'),
                });
            }
        }

        suggestions
    }
}
