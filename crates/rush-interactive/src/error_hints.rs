use std::env;
use std::fs;
use std::path::PathBuf;

/// Provides helpful error messages and suggestions
///
/// Features:
/// - "Did you mean?" suggestions for command-not-found errors
/// - Common typo detection
/// - Helpful hints for common mistakes
pub struct ErrorHints;

impl ErrorHints {
    /// Get a helpful error message when a command is not found
    pub fn command_not_found(command: &str) -> String {
        let mut message = format!("command not found: {}", command);

        // Try to find similar commands
        if let Some(suggestion) = Self::find_similar_command(command) {
            message.push_str(&format!("\n\nDid you mean '{}'?", suggestion));
        }

        // Check for common typos/mistakes
        if let Some(hint) = Self::get_common_hint(command) {
            message.push_str(&format!("\n\nHint: {}", hint));
        }

        message
    }

    /// Find a similar command in PATH using string distance
    fn find_similar_command(command: &str) -> Option<String> {
        let mut best_match: Option<(String, usize)> = None;
        const MAX_DISTANCE: usize = 3; // Maximum Levenshtein distance to consider

        // Get all commands from PATH
        let commands = Self::get_all_commands();

        for cmd in commands {
            let distance = Self::levenshtein_distance(command, &cmd);

            // Only consider commands with reasonable similarity
            if distance <= MAX_DISTANCE {
                if let Some((_, best_dist)) = &best_match {
                    if distance < *best_dist {
                        best_match = Some((cmd, distance));
                    }
                } else {
                    best_match = Some((cmd, distance));
                }
            }
        }

        best_match.map(|(cmd, _)| cmd)
    }

    /// Get all available commands from PATH and built-ins
    fn get_all_commands() -> Vec<String> {
        let mut commands = Vec::new();

        // Add built-ins
        commands.extend_from_slice(&[
            "cd".to_string(),
            "pwd".to_string(),
            "exit".to_string(),
            "jobs".to_string(),
            "fg".to_string(),
            "bg".to_string(),
            "test".to_string(),
        ]);

        // Add commands from PATH
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

        commands
    }

    /// Calculate Levenshtein distance between two strings
    fn levenshtein_distance(s1: &str, s2: &str) -> usize {
        let len1 = s1.len();
        let len2 = s2.len();

        if len1 == 0 {
            return len2;
        }
        if len2 == 0 {
            return len1;
        }

        let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

        // Initialize first column and row
        for i in 0..=len1 {
            matrix[i][0] = i;
        }
        for j in 0..=len2 {
            matrix[0][j] = j;
        }

        // Fill the matrix
        for (i, c1) in s1.chars().enumerate() {
            for (j, c2) in s2.chars().enumerate() {
                let cost = if c1 == c2 { 0 } else { 1 };
                matrix[i + 1][j + 1] = std::cmp::min(
                    std::cmp::min(
                        matrix[i][j + 1] + 1,     // deletion
                        matrix[i + 1][j] + 1,     // insertion
                    ),
                    matrix[i][j] + cost,           // substitution
                );
            }
        }

        matrix[len1][len2]
    }

    /// Get helpful hints for common mistakes
    fn get_common_hint(command: &str) -> Option<String> {
        // Common typos and hints
        match command {
            "sl" => Some("Did you mean 'ls'? (Common typo)".to_string()),
            "gti" => Some("Did you mean 'git'? (Common typo)".to_string()),
            "clea" | "claer" => Some("Did you mean 'clear'? (Common typo)".to_string()),
            "exti" => Some("Did you mean 'exit'? (Common typo)".to_string()),
            "grpe" => Some("Did you mean 'grep'? (Common typo)".to_string()),
            _ if command.ends_with(".sh") && PathBuf::from(command).exists() => {
                Some(format!("File exists. Try: rush {} or ./{}",command, command))
            }
            _ if PathBuf::from(format!("./{}", command)).exists() => {
                Some(format!("File exists in current directory. Try: ./{}", command))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(ErrorHints::levenshtein_distance("cat", "cat"), 0);
        assert_eq!(ErrorHints::levenshtein_distance("cat", "bat"), 1);
        assert_eq!(ErrorHints::levenshtein_distance("cat", "cats"), 1);
        assert_eq!(ErrorHints::levenshtein_distance("cat", "dog"), 3);
    }

    #[test]
    fn test_common_hints() {
        assert!(ErrorHints::get_common_hint("sl").is_some());
        assert!(ErrorHints::get_common_hint("gti").is_some());
        assert!(ErrorHints::get_common_hint("unknown_cmd_xyz").is_none());
    }
}
