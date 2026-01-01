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
                            // Include both regular files and symlinks (many executables are symlinks)
                            if file_type.is_file() || file_type.is_symlink() {
                                // Use fs::metadata(path) which follows symlinks, NOT entry.metadata()
                                // entry.metadata() does NOT follow symlinks (like lstat)
                                if let Ok(metadata) = fs::metadata(entry.path()) {
                                    // Only include if target is a file (not a directory symlink)
                                    if !metadata.is_file() {
                                        continue;
                                    }
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
        let builtins = [
            "cd", "pwd", "exit", "true", "false", "test", "[",
            "eval", "alias", "unalias", "trap", "set", "shopt",
            "export", "unset", "readonly", "declare", "typeset", "local",
            "read", "shift", "wait", "kill", "times", "umask", "hash",
            "getopts", "exec", "command", "jobs", "fg", "bg",
            "coproc", "disown", "printf", "mapfile", "readarray",
            "break", "continue", "return", "source", ".",
        ];
        commands.extend(builtins.iter().map(|s| s.to_string()));

        // Sort and deduplicate
        commands.sort();
        commands.dedup();
        commands
    }

    /// Get file/directory completions for a partial path
    fn get_file_completions(partial: &str) -> Vec<String> {
        let mut completions = Vec::new();

        // Determine the directory to search and the prefix to match
        let (search_dir, prefix) = if partial.is_empty() {
            // Empty partial - list current directory
            (PathBuf::from("."), String::new())
        } else if partial.ends_with('/') {
            // Path ends with / - list contents of that directory
            (PathBuf::from(partial), String::new())
        } else if partial.contains('/') {
            // Path contains / but doesn't end with it - split into dir and partial filename
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
            // No / - search current directory
            (PathBuf::from("."), partial.to_string())
        };

        // Should we show hidden files? Only if partial starts with '.'
        let show_hidden = prefix.starts_with('.');

        // Read directory and find matches
        if let Ok(entries) = fs::read_dir(&search_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    // Skip hidden files unless explicitly requested
                    if name.starts_with('.') && !show_hidden {
                        continue;
                    }

                    if name.starts_with(&prefix) {
                        // Build the full completion path
                        let mut completion = if partial.is_empty() {
                            // Empty partial - just the name
                            name.to_string()
                        } else if partial.ends_with('/') {
                            // partial is "dir/" - completion is "dir/name"
                            format!("{}{}", partial, name)
                        } else if partial.contains('/') {
                            // partial is "dir/partial" - completion is "dir/name"
                            let parent_path = PathBuf::from(partial);
                            let parent = parent_path
                                .parent()
                                .unwrap_or_else(|| std::path::Path::new("."));
                            parent.join(name)
                                .to_string_lossy()
                                .to_string()
                        } else {
                            // partial is just "name" - completion is "name"
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

        let span = Span::new(start, pos);
        let mut suggestions = Vec::new();

        if Self::is_first_word(line, pos) {
            // Complete command names - skip if nothing typed yet
            if partial.is_empty() {
                return vec![];
            }
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
            // When partial is empty (e.g., "ls "), show all non-hidden files
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
