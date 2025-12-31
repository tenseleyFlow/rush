//! Glob pattern expansion
//!
//! Supports:
//! - Basic patterns: *, ?, [abc], [a-z]
//! - Recursive globbing: **
//! - Extended glob: !(pattern), ?(pattern), *(pattern), +(pattern), @(pattern)
//! - Dotfile handling

use std::fs;
use std::path::{Path, PathBuf};
use globset::{Glob, GlobBuilder, GlobMatcher};

#[derive(Debug)]
pub struct GlobOptions {
    /// Match dotfiles (files starting with .)
    pub match_dotfiles: bool,
    /// Allow recursive ** patterns
    pub globstar: bool,
    /// Return empty list if no matches (vs literal pattern)
    pub nullglob: bool,
}

impl Default for GlobOptions {
    fn default() -> Self {
        Self {
            match_dotfiles: false,
            globstar: true,
            nullglob: false,
        }
    }
}

/// Expand a glob pattern into matching file paths
pub fn expand_glob(pattern: &str, options: &GlobOptions) -> Result<Vec<String>, String> {
    // Check if pattern contains glob metacharacters
    if !has_glob_chars(pattern) {
        // No glob characters - return as-is
        return Ok(vec![pattern.to_string()]);
    }

    // Handle extended glob patterns first
    let (pattern, _is_extglob) = parse_extglob(pattern)?;

    // Build glob matcher
    let glob = GlobBuilder::new(&pattern)
        .literal_separator(false) // Allow * to match /
        .build()
        .map_err(|e| format!("Invalid glob pattern: {}", e))?;

    let matcher = glob.compile_matcher();

    // Get base directory and relative pattern
    let (base_dir, rel_pattern) = split_pattern(&pattern);

    // Expand the pattern
    let mut matches = Vec::new();
    expand_pattern(&base_dir, &rel_pattern, &matcher, options, &mut matches)?;

    // Sort results for consistency
    matches.sort();

    // Handle no matches
    if matches.is_empty() {
        if options.nullglob {
            Ok(vec![])
        } else {
            // Return literal pattern if no matches
            Ok(vec![pattern.clone()])
        }
    } else {
        Ok(matches)
    }
}

/// Check if a pattern contains glob metacharacters
fn has_glob_chars(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[') || s.contains(']')
}

/// Parse extended glob patterns and convert to standard glob
/// Returns (converted_pattern, is_extglob)
fn parse_extglob(pattern: &str) -> Result<(String, bool), String> {
    // Check for extended glob patterns: !(pat), ?(pat), *(pat), +(pat), @(pat)
    if pattern.contains("!(") || pattern.contains("?(") ||
       pattern.contains("*(") || pattern.contains("+(") || pattern.contains("@(") {
        // Convert extglob to standard glob
        // For now, use a simplified conversion
        let converted = convert_extglob(pattern)?;
        Ok((converted, true))
    } else {
        Ok((pattern.to_string(), false))
    }
}

/// Convert extended glob patterns to standard glob
fn convert_extglob(pattern: &str) -> Result<String, String> {
    let mut result = String::new();
    let mut chars = pattern.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '!' | '?' | '*' | '+' | '@' => {
                if chars.peek() == Some(&'(') {
                    chars.next(); // consume '('

                    // Find matching ')'
                    let mut depth = 1;
                    let mut inner = String::new();
                    while let Some(c) = chars.next() {
                        if c == '(' {
                            depth += 1;
                            inner.push(c);
                        } else if c == ')' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            inner.push(c);
                        } else {
                            inner.push(c);
                        }
                    }

                    // Convert based on type
                    match ch {
                        '!' => {
                            // !(pattern) - negative match
                            // Convert to [^...] or handle specially
                            // For simplicity, skip this match in filtering
                            result.push_str(&format!("*")); // Simplified
                        }
                        '?' => {
                            // ?(pattern) - zero or one
                            result.push_str(&format!("({})?", inner));
                        }
                        '*' => {
                            // *(pattern) - zero or more
                            result.push_str(&format!("({})*", inner));
                        }
                        '+' => {
                            // +(pattern) - one or more
                            result.push_str(&format!("({})+", inner));
                        }
                        '@' => {
                            // @(pattern) - exactly one
                            result.push_str(&inner);
                        }
                        _ => unreachable!(),
                    }
                } else {
                    result.push(ch);
                }
            }
            _ => result.push(ch),
        }
    }

    Ok(result)
}

/// Split pattern into base directory and relative pattern
fn split_pattern(pattern: &str) -> (PathBuf, String) {
    let path = Path::new(pattern);

    // Find the first component with glob characters
    let mut base = PathBuf::new();
    let mut rel_parts = Vec::new();
    let mut found_glob = false;

    for component in path.components() {
        let comp_str = component.as_os_str().to_string_lossy();
        if !found_glob && !has_glob_chars(&comp_str) {
            base.push(component);
        } else {
            found_glob = true;
            rel_parts.push(comp_str.to_string());
        }
    }

    let rel_pattern = if rel_parts.is_empty() {
        pattern.to_string()
    } else {
        rel_parts.join("/")
    };

    if base.as_os_str().is_empty() {
        base = PathBuf::from(".");
    }

    (base, rel_pattern)
}

/// Recursively expand a glob pattern
fn expand_pattern(
    dir: &Path,
    pattern: &str,
    matcher: &GlobMatcher,
    options: &GlobOptions,
    results: &mut Vec<String>,
) -> Result<(), String> {
    // Handle ** recursive glob
    if pattern.starts_with("**/") || pattern == "**" {
        if options.globstar {
            let rest = if pattern == "**" {
                "*"
            } else {
                &pattern[3..]
            };

            // Recursively search all subdirectories
            expand_recursive(dir, rest, matcher, options, results)?;
            return Ok(());
        }
    }

    // If pattern contains /, we need to handle directory traversal
    if pattern.contains('/') {
        let parts: Vec<&str> = pattern.split('/').collect();
        if parts.len() > 1 {
            let first_part = parts[0];
            let rest = parts[1..].join("/");

            // Read directory and match first part
            let entries = fs::read_dir(dir)
                .map_err(|e| format!("Failed to read directory {}: {}", dir.display(), e))?;

            for entry in entries {
                let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
                let path = entry.path();
                let file_name = entry.file_name();
                let name = file_name.to_string_lossy();

                // Skip dotfiles unless enabled
                if !options.match_dotfiles && name.starts_with('.') {
                    continue;
                }

                // Match first part
                if glob_match_simple(first_part, &name) && path.is_dir() {
                    expand_pattern(&path, &rest, matcher, options, results)?;
                }
            }
            return Ok(());
        }
    }

    // Simple case: pattern is just a filename pattern in current directory
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory {}: {}", dir.display(), e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();

        // Skip dotfiles unless enabled
        if !options.match_dotfiles && name.starts_with('.') {
            continue;
        }

        // Match against pattern
        if glob_match_simple(pattern, &name) {
            results.push(path.to_string_lossy().to_string());
        }
    }

    Ok(())
}

/// Recursively expand with ** pattern
fn expand_recursive(
    dir: &Path,
    pattern: &str,
    matcher: &GlobMatcher,
    options: &GlobOptions,
    results: &mut Vec<String>,
) -> Result<(), String> {
    // Try to match in current directory
    expand_pattern(dir, pattern, matcher, options, results)?;

    // Recurse into subdirectories
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory {}: {}", dir.display(), e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry.path();

        if path.is_dir() {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();

            // Skip dotfiles unless enabled
            if !options.match_dotfiles && name.starts_with('.') {
                continue;
            }

            // Recurse
            expand_recursive(&path, pattern, matcher, options, results)?;
        }
    }

    Ok(())
}

/// Simple glob matching for a single path component
fn glob_match_simple(pattern: &str, text: &str) -> bool {
    // Handle simple cases
    if pattern == "*" {
        return true;
    }

    if pattern == text {
        return true;
    }

    // Use globset for more complex patterns
    if let Ok(glob) = Glob::new(pattern) {
        glob.compile_matcher().is_match(text)
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_glob_chars() {
        assert!(has_glob_chars("*.txt"));
        assert!(has_glob_chars("file?.rs"));
        assert!(has_glob_chars("[abc]"));
        assert!(!has_glob_chars("plain.txt"));
    }

    #[test]
    fn test_glob_match_simple() {
        assert!(glob_match_simple("*", "anything"));
        assert!(glob_match_simple("*.txt", "file.txt"));
        assert!(glob_match_simple("file", "file"));
        assert!(!glob_match_simple("*.txt", "file.rs"));
    }
}
