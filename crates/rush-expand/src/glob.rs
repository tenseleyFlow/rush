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
use regex::Regex;

#[derive(Debug)]
pub struct GlobOptions {
    /// Match dotfiles (files starting with .)
    pub match_dotfiles: bool,
    /// Allow recursive ** patterns
    pub globstar: bool,
    /// Return empty list if no matches (vs literal pattern)
    pub nullglob: bool,
    /// Enable extended glob patterns: !(pat), ?(pat), *(pat), +(pat), @(pat)
    pub extglob: bool,
}

impl Default for GlobOptions {
    fn default() -> Self {
        Self {
            match_dotfiles: false,
            globstar: true,
            nullglob: false,
            extglob: false,
        }
    }
}

/// Extended glob pattern representation
#[derive(Debug)]
enum ExtGlobPattern {
    /// No extended glob - use standard matching
    Standard(String),
    /// Negation pattern - requires two-stage filtering
    Negation {
        base_pattern: String,
        exclude_regex: Regex,
    },
}

/// Expand a glob pattern into matching file paths
pub fn expand_glob(pattern: &str, options: &GlobOptions) -> Result<Vec<String>, String> {
    // Check for extended glob patterns first (before metacharacter check)
    let is_extglob = options.extglob && (
        pattern.contains("!(") || pattern.contains("?(") || pattern.contains("*(")
        || pattern.contains("+(") || pattern.contains("@(")
    );

    // Check if pattern contains glob metacharacters
    // Skip this check for extglob patterns as they need processing even without metacharacters
    if !is_extglob && !has_glob_chars(pattern) {
        // No glob characters - return as-is
        return Ok(vec![pattern.to_string()]);
    }

    // Parse and convert extended glob patterns (if extglob is enabled)
    let extglob_result = parse_extglob(pattern, options.extglob)?;

    match extglob_result {
        ExtGlobPattern::Standard(base_pattern) => {
            // Standard glob - no negation filtering needed
            expand_standard_glob(&base_pattern, options)
        }
        ExtGlobPattern::Negation { base_pattern, exclude_regex } => {
            // Two-stage filtering for negation patterns
            // Stage 1: Expand base pattern to get candidates
            let candidates = expand_standard_glob(&base_pattern, options)?;

            // Stage 2: Filter out matches of the exclude pattern
            let filtered: Vec<String> = candidates
                .into_iter()
                .filter(|path| {
                    // Extract just the filename for matching
                    let filename = Path::new(path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(path);

                    // Keep if it does NOT match the exclude pattern
                    !exclude_regex.is_match(filename)
                })
                .collect();

            // Handle no matches
            if filtered.is_empty() && !options.nullglob {
                Ok(vec![pattern.to_string()])
            } else {
                Ok(filtered)
            }
        }
    }
}

/// Expand a standard (non-negated) glob pattern
fn expand_standard_glob(pattern: &str, options: &GlobOptions) -> Result<Vec<String>, String> {
    // Build glob matcher
    let glob = GlobBuilder::new(pattern)
        .literal_separator(false) // Allow * to match /
        .build()
        .map_err(|e| format!("Invalid glob pattern: {}", e))?;

    let matcher = glob.compile_matcher();

    // Get base directory and relative pattern
    let (base_dir, rel_pattern) = split_pattern(pattern);

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
            Ok(vec![pattern.to_string()])
        }
    } else {
        Ok(matches)
    }
}

/// Check if a pattern contains glob metacharacters or extended glob patterns
fn has_glob_chars(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[') || s.contains(']')
        || s.contains("!(") || s.contains("?(") || s.contains("*(")
        || s.contains("+(") || s.contains("@(")
}

/// Parse extended glob patterns and convert appropriately
fn parse_extglob(pattern: &str, extglob_enabled: bool) -> Result<ExtGlobPattern, String> {
    // If extglob is disabled, treat everything as standard glob
    if !extglob_enabled {
        return Ok(ExtGlobPattern::Standard(pattern.to_string()));
    }

    // Check for extended glob patterns: !(pat), ?(pat), *(pat), +(pat), @(pat)
    if !pattern.contains("!(") && !pattern.contains("?(") &&
       !pattern.contains("*(") && !pattern.contains("+(") && !pattern.contains("@(") {
        // No extended glob - return as standard
        return Ok(ExtGlobPattern::Standard(pattern.to_string()));
    }

    // Convert extglob - check if it contains negation
    let (converted_pattern, has_negation, exclude_pattern) = convert_extglob(pattern)?;

    if has_negation {
        // Build regex from the exclude pattern
        let regex_pattern = glob_to_regex(&exclude_pattern)?;
        let exclude_regex = Regex::new(&regex_pattern)
            .map_err(|e| format!("Invalid regex pattern: {}", e))?;

        Ok(ExtGlobPattern::Negation {
            base_pattern: converted_pattern,
            exclude_regex,
        })
    } else {
        Ok(ExtGlobPattern::Standard(converted_pattern))
    }
}

/// Convert glob pattern to regex pattern
fn glob_to_regex(pattern: &str) -> Result<String, String> {
    let mut regex = String::from("^");

    for ch in pattern.chars() {
        match ch {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            '.' => regex.push_str("\\."),
            '[' => regex.push('['),
            ']' => regex.push(']'),
            '(' => regex.push_str("\\("),
            ')' => regex.push_str("\\)"),
            '{' => regex.push_str("\\{"),
            '}' => regex.push_str("\\}"),
            '+' => regex.push_str("\\+"),
            '^' => regex.push_str("\\^"),
            '$' => regex.push_str("\\$"),
            '|' => regex.push_str("\\|"),
            '\\' => regex.push_str("\\\\"),
            _ => regex.push(ch),
        }
    }

    regex.push('$');
    Ok(regex)
}

/// Convert extended glob patterns to standard glob
/// Returns (converted_pattern, has_negation, exclude_pattern)
fn convert_extglob(pattern: &str) -> Result<(String, bool, String), String> {
    let mut result = String::new();
    let mut chars = pattern.chars().peekable();
    let mut has_negation = false;
    let mut exclude_pattern = String::new();

    // For simplicity, we only handle a single !(pattern) at the top level
    // Multiple or nested negations are not yet supported

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
                            // Use two-stage filtering: match all (*), then exclude pattern
                            result.push('*');
                            has_negation = true;
                            exclude_pattern = inner;
                        }
                        '?' => {
                            // ?(pattern) - zero or one
                            // In glob: not directly supported, approximate with optional match
                            // For single char patterns, just make it optional
                            if inner.len() == 1 {
                                result.push('[');
                                result.push_str(&inner);
                                result.push(']');
                                result.push('?');
                            } else {
                                // Multi-char: can't express in glob, use pattern or nothing
                                // This is lossy but better than nothing
                                result.push('*');
                            }
                        }
                        '*' => {
                            // *(pattern) - zero or more
                            // In glob: approximate with *
                            result.push('*');
                        }
                        '+' => {
                            // +(pattern) - one or more
                            // In glob: approximate with pattern followed by *
                            result.push_str(&inner);
                            result.push('*');
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

    Ok((result, has_negation, exclude_pattern))
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
