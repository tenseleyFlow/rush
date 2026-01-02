use crate::arithmetic::{evaluate_arithmetic, ArithmeticError};
use crate::brace::expand_brace;
use crate::brace_parse::detect_brace_patterns;
use crate::command_subst::{execute_command_substitution, CommandSubstError};
use crate::context::Context;
use rush_parser::{VarExpansion, Word, WordPart};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExpansionError {
    #[error("Command substitution failed: {0}")]
    CommandSubstitutionFailed(#[from] CommandSubstError),

    #[error("Arithmetic error: {0}")]
    ArithmeticError(#[from] ArithmeticError),

    #[error("{0}")]
    ParameterError(String),

    #[error("Expansion error: {0}")]
    Other(String),
}

/// Expand a Word into one or more Strings (due to brace expansion)
pub fn expand_word_with_braces(word: &Word, context: &mut Context) -> Result<Vec<String>, ExpansionError> {
    // Find if there's a brace expansion
    let brace_index = word.parts.iter().position(|p| matches!(p, WordPart::BraceExpansion(_)));

    if let Some(idx) = brace_index {
        // Has brace expansion - expand it into multiple words
        let mut results = Vec::new();

        // Get the brace expansion
        if let WordPart::BraceExpansion(brace) = &word.parts[idx] {
            let expansions = expand_brace(brace);

            // For each expansion, build a complete word
            for expanded_part in expansions {
                let mut parts_copy = word.parts.clone();
                // Replace the brace expansion with a literal
                parts_copy[idx] = WordPart::Literal(expanded_part);

                // Create a new word and expand it (recursively handles nested braces)
                let new_word = Word::new(parts_copy);
                let mut nested_results = expand_word_with_braces(&new_word, context)?;
                results.append(&mut nested_results);
            }
        }

        Ok(results)
    } else {
        // No brace expansion - just expand normally
        let result = expand_word_simple(word, context)?;
        Ok(vec![result])
    }
}

/// Expand a Word into a String by resolving all expansions (no brace expansion)
fn expand_word_simple(word: &Word, context: &mut Context) -> Result<String, ExpansionError> {
    let mut result = String::new();

    for part in &word.parts {
        match part {
            WordPart::Literal(s) => {
                // Keep backslash escapes - they'll be stripped during glob expansion
                result.push_str(s);
            }
            WordPart::VarExpansion(var_exp) => {
                let expanded = expand_var(var_exp, context)?;
                result.push_str(&expanded);
            }
            WordPart::CommandSubstitution(cmd) => {
                let output = execute_command_substitution(cmd, context)?;
                result.push_str(&output);
            }
            WordPart::ArithmeticExpansion(expr) => {
                let value = evaluate_arithmetic(expr, context)?;
                result.push_str(&value.to_string());
            }
            WordPart::BraceExpansion(_) => {
                // Should not happen - braces are handled in expand_word_with_braces
                return Err(ExpansionError::Other(
                    "Unexpected brace expansion in simple expansion".to_string(),
                ));
            }
            WordPart::ArrayLiteral(elements) => {
                // Array literals like (one two three) expand to space-separated values
                let mut values = Vec::new();
                for elem in elements {
                    values.push(expand_word_simple(elem, context)?);
                }
                result.push_str(&values.join(" "));
            }
        }
    }

    Ok(result)
}

/// Expand a Word into a String by resolving all expansions
/// (Kept for backward compatibility - delegates to new function)
pub fn expand_word(word: &Word, context: &mut Context) -> Result<String, ExpansionError> {
    let results = expand_word_with_braces(word, context)?;
    Ok(results.join(" "))
}

/// Expand a variable reference
fn expand_var(var_exp: &VarExpansion, context: &mut Context) -> Result<String, ExpansionError> {
    match var_exp {
        VarExpansion::Simple(name) | VarExpansion::Braced(name) => {
            // Handle special variables
            match name.as_str() {
                "?" => return Ok(context.last_exit_status.to_string()),
                "#" => return Ok(context.positional_params.len().to_string()),
                "@" | "*" => {
                    // $@ and $* expand to all positional parameters
                    // They differ in quoting behavior, but for simple expansion they're the same
                    return Ok(context.positional_params.join(" "));
                }
                "0" => {
                    // $0 is the shell name or script name
                    return Ok(context.get_var("0").unwrap_or("rush").to_string());
                }
                _ => {
                    // Check if it's a positional parameter like $1, $2, etc.
                    if let Ok(index) = name.parse::<usize>() {
                        if index > 0 && index <= context.positional_params.len() {
                            return Ok(context.positional_params[index - 1].clone());
                        } else {
                            return Ok(String::new());
                        }
                    }
                }
            }

            // Simple expansion: $VAR or ${VAR}
            Ok(context.get_var(name).unwrap_or("").to_string())
        }
        VarExpansion::WithDefault { name, default } => {
            // ${VAR:-default} - use default if VAR is unset or empty
            match context.get_var(name) {
                Some(value) if !value.is_empty() => Ok(value.to_string()),
                _ => {
                    // Expand the default value recursively
                    expand_word(default, context)
                }
            }
        }
        VarExpansion::AssignDefault { name, default } => {
            // ${VAR:=default} - assign default if VAR is unset or empty
            match context.get_var(name) {
                Some(value) if !value.is_empty() => Ok(value.to_string()),
                _ => {
                    // Expand the default and assign it to the variable
                    let expanded = expand_word(default, context)?;
                    let _ = context.set_var(name, &expanded);
                    Ok(expanded)
                }
            }
        }
        VarExpansion::UseIfSet { name, alternate } => {
            // ${VAR:+alternate} - use alternate if VAR is set and non-empty
            match context.get_var(name) {
                Some(value) if !value.is_empty() => {
                    // VAR is set and non-empty, use the alternate
                    expand_word(alternate, context)
                }
                _ => {
                    // VAR is unset or empty, return empty string
                    Ok(String::new())
                }
            }
        }
        VarExpansion::ErrorIfUnset { name, message } => {
            // ${VAR:?message} - error if VAR is unset or empty
            match context.get_var(name) {
                Some(value) if !value.is_empty() => Ok(value.to_string()),
                _ => {
                    // Expand the message for the error
                    let msg = expand_word(message, context)?;
                    let err_msg = if msg.is_empty() {
                        format!("{}: parameter null or not set", name)
                    } else {
                        format!("{}: {}", name, msg)
                    };
                    Err(ExpansionError::ParameterError(err_msg))
                }
            }
        }
        // Non-colon variants - only check if unset, empty is OK
        VarExpansion::WithDefaultUnsetOnly { name, default } => {
            // ${VAR-default} - use default only if VAR is unset
            match context.get_var(name) {
                Some(value) => Ok(value.to_string()), // Empty is fine
                None => expand_word(default, context),
            }
        }
        VarExpansion::AssignDefaultUnsetOnly { name, default } => {
            // ${VAR=default} - assign default only if VAR is unset
            match context.get_var(name) {
                Some(value) => Ok(value.to_string()), // Empty is fine
                None => {
                    let expanded = expand_word(default, context)?;
                    let _ = context.set_var(name, &expanded);
                    Ok(expanded)
                }
            }
        }
        VarExpansion::UseIfSetOnly { name, alternate } => {
            // ${VAR+alternate} - use alternate if VAR is set (even if empty)
            match context.get_var(name) {
                Some(_) => expand_word(alternate, context), // Set, even if empty
                None => Ok(String::new()),
            }
        }
        VarExpansion::ErrorIfUnsetOnly { name, message } => {
            // ${VAR?message} - error only if VAR is unset
            match context.get_var(name) {
                Some(value) => Ok(value.to_string()), // Empty is fine
                None => {
                    let msg = expand_word(message, context)?;
                    let err_msg = if msg.is_empty() {
                        format!("{}: parameter not set", name)
                    } else {
                        format!("{}: {}", name, msg)
                    };
                    Err(ExpansionError::ParameterError(err_msg))
                }
            }
        }
        VarExpansion::Length(name) => {
            // ${#VAR} - return the length of the variable value
            let value = context.get_var(name).unwrap_or("");
            Ok(value.len().to_string())
        }
        VarExpansion::RemoveShortestPrefix { name, pattern } => {
            // ${VAR#pattern} - remove shortest matching prefix
            let value = context.get_var(name).unwrap_or("").to_string();
            Ok(remove_prefix(&value, pattern, false))
        }
        VarExpansion::RemoveLongestPrefix { name, pattern } => {
            // ${VAR##pattern} - remove longest matching prefix
            let value = context.get_var(name).unwrap_or("").to_string();
            Ok(remove_prefix(&value, pattern, true))
        }
        VarExpansion::RemoveShortestSuffix { name, pattern } => {
            // ${VAR%pattern} - remove shortest matching suffix
            let value = context.get_var(name).unwrap_or("").to_string();
            Ok(remove_suffix(&value, pattern, false))
        }
        VarExpansion::RemoveLongestSuffix { name, pattern } => {
            // ${VAR%%pattern} - remove longest matching suffix
            let value = context.get_var(name).unwrap_or("").to_string();
            Ok(remove_suffix(&value, pattern, true))
        }
        VarExpansion::ReplaceFirst { name, pattern, replacement } => {
            // ${VAR/pattern/replacement} - replace first occurrence
            let value = context.get_var(name).unwrap_or("").to_string();
            Ok(replace_pattern(&value, pattern, replacement, false))
        }
        VarExpansion::ReplaceAll { name, pattern, replacement } => {
            // ${VAR//pattern/replacement} - replace all occurrences
            let value = context.get_var(name).unwrap_or("").to_string();
            Ok(replace_pattern(&value, pattern, replacement, true))
        }
        VarExpansion::Substring { name, offset, length } => {
            // ${VAR:offset} or ${VAR:offset:length}
            let value = context.get_var(name).unwrap_or("");
            Ok(substring(value, *offset, *length))
        }
        VarExpansion::UppercaseFirst(name) => {
            // ${VAR^} - uppercase first character
            let value = context.get_var(name).unwrap_or("");
            Ok(uppercase_first(value))
        }
        VarExpansion::UppercaseAll(name) => {
            // ${VAR^^} - uppercase all characters
            let value = context.get_var(name).unwrap_or("");
            Ok(value.to_uppercase())
        }
        VarExpansion::LowercaseFirst(name) => {
            // ${VAR,} - lowercase first character
            let value = context.get_var(name).unwrap_or("");
            Ok(lowercase_first(value))
        }
        VarExpansion::LowercaseAll(name) => {
            // ${VAR,,} - lowercase all characters
            let value = context.get_var(name).unwrap_or("");
            Ok(value.to_lowercase())
        }
        VarExpansion::ArrayElement { name, index } => {
            // ${arr[index]} - get array element at index or key
            match context.arrays.get(name) {
                Some(crate::context::ArrayType::Indexed(vec)) => {
                    // Indexed array - parse index as integer
                    let idx = index.parse::<usize>().unwrap_or(0);
                    Ok(vec.get(idx).cloned().unwrap_or_default())
                }
                Some(crate::context::ArrayType::Associative(map)) => {
                    // Associative array - use index as string key
                    Ok(map.get(index).cloned().unwrap_or_default())
                }
                None => Ok(String::new()),
            }
        }
        VarExpansion::ArrayAll(name) => {
            // ${arr[@]} - all elements as separate words
            match context.arrays.get(name) {
                Some(crate::context::ArrayType::Indexed(vec)) => Ok(vec.join(" ")),
                Some(crate::context::ArrayType::Associative(map)) => {
                    // For associative arrays, ${arr[@]} returns all values
                    Ok(map.values().cloned().collect::<Vec<_>>().join(" "))
                }
                None => Ok(String::new()),
            }
        }
        VarExpansion::ArrayStar(name) => {
            // ${arr[*]} - all elements as single word
            match context.arrays.get(name) {
                Some(crate::context::ArrayType::Indexed(vec)) => Ok(vec.join(" ")),
                Some(crate::context::ArrayType::Associative(map)) => {
                    // For associative arrays, ${arr[*]} returns all values
                    Ok(map.values().cloned().collect::<Vec<_>>().join(" "))
                }
                None => Ok(String::new()),
            }
        }
        VarExpansion::ArrayLength(name) => {
            // ${#arr[@]} - number of elements in array
            match context.arrays.get(name) {
                Some(crate::context::ArrayType::Indexed(vec)) => Ok(vec.len().to_string()),
                Some(crate::context::ArrayType::Associative(map)) => Ok(map.len().to_string()),
                None => Ok("0".to_string()),
            }
        }
        VarExpansion::ArrayIndices(name) => {
            // ${!arr[@]} - array indices or keys
            match context.arrays.get(name) {
                Some(crate::context::ArrayType::Indexed(vec)) => {
                    // For indexed arrays, return numeric indices
                    let indices: Vec<String> = (0..vec.len()).map(|i| i.to_string()).collect();
                    Ok(indices.join(" "))
                }
                Some(crate::context::ArrayType::Associative(map)) => {
                    // For associative arrays, return keys
                    Ok(map.keys().cloned().collect::<Vec<_>>().join(" "))
                }
                None => Ok(String::new()),
            }
        }
        VarExpansion::Indirect(name) => {
            // ${!var} - indirect expansion
            // First get the value of the variable, then use that as a variable name
            let indirect_name = context.get_var(name).unwrap_or("");
            if indirect_name.is_empty() {
                Ok(String::new())
            } else {
                Ok(context.get_var(indirect_name).unwrap_or("").to_string())
            }
        }
        VarExpansion::Transform { name, op } => {
            // ${var@op} - transformation operators
            let value = context.get_var(name).unwrap_or("");
            Ok(apply_transform(value, *op))
        }
    }
}

/// Apply transformation operator to a value
fn apply_transform(value: &str, op: char) -> String {
    match op {
        'Q' => {
            // Quote for reuse as input (single-quoted format)
            format!("'{}'", value.replace('\'', "'\\''"))
        }
        'E' => {
            // Expand escape sequences like $'...'
            expand_escapes(value)
        }
        'P' => {
            // Expand as prompt string (simplified - just return value)
            value.to_string()
        }
        'A' => {
            // Assignment statement format (simplified)
            value.to_string()
        }
        'K' => {
            // Quote for reuse, associative array format
            format!("'{}'", value.replace('\'', "'\\''"))
        }
        'a' => {
            // Attributes (simplified - return empty)
            String::new()
        }
        'u' | 'U' => {
            // Uppercase (U = all, u = first)
            if op == 'U' {
                value.to_uppercase()
            } else {
                uppercase_first(value)
            }
        }
        'L' => {
            // Lowercase all
            value.to_lowercase()
        }
        _ => value.to_string(),
    }
}

/// Expand escape sequences like \n, \t, etc.
fn expand_escapes(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(&next) = chars.peek() {
                chars.next();
                match next {
                    'n' => result.push('\n'),
                    't' => result.push('\t'),
                    'r' => result.push('\r'),
                    '\\' => result.push('\\'),
                    '\'' => result.push('\''),
                    '"' => result.push('"'),
                    '0' => result.push('\0'),
                    'a' => result.push('\x07'),  // Bell
                    'b' => result.push('\x08'),  // Backspace
                    'e' | 'E' => result.push('\x1b'),  // Escape
                    'f' => result.push('\x0c'),  // Form feed
                    'v' => result.push('\x0b'),  // Vertical tab
                    _ => {
                        result.push('\\');
                        result.push(next);
                    }
                }
            } else {
                result.push('\\');
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Remove prefix from string based on pattern
/// If greedy is true, removes the longest match; otherwise shortest
fn remove_prefix(value: &str, pattern: &str, greedy: bool) -> String {
    // Simple glob pattern matching with * wildcard
    if pattern.contains('*') {
        // For patterns like "a*b", we match from start
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            let prefix = parts[0];
            let suffix = parts[1];

            if value.starts_with(prefix) {
                if greedy {
                    // Find the last occurrence of suffix
                    if let Some(pos) = value.rfind(suffix) {
                        return value[pos + suffix.len()..].to_string();
                    }
                } else {
                    // Find the first occurrence of suffix after prefix
                    if let Some(pos) = value[prefix.len()..].find(suffix) {
                        return value[prefix.len() + pos + suffix.len()..].to_string();
                    }
                }
            }
        }
    } else {
        // Literal pattern - just remove if it's a prefix
        if value.starts_with(pattern) {
            return value[pattern.len()..].to_string();
        }
    }
    value.to_string()
}

/// Remove suffix from string based on pattern
/// If greedy is true, removes the longest match; otherwise shortest
fn remove_suffix(value: &str, pattern: &str, greedy: bool) -> String {
    // Simple glob pattern matching with * wildcard
    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            let prefix = parts[0];
            let suffix = parts[1];

            if value.ends_with(suffix) {
                if greedy {
                    // Find the first occurrence of prefix
                    if let Some(pos) = value.find(prefix) {
                        return value[..pos].to_string();
                    }
                } else {
                    // Find the last occurrence of prefix before suffix
                    let end_without_suffix = value.len() - suffix.len();
                    if let Some(pos) = value[..end_without_suffix].rfind(prefix) {
                        return value[..pos].to_string();
                    }
                }
            }
        }
    } else {
        // Literal pattern - just remove if it's a suffix
        if value.ends_with(pattern) {
            return value[..value.len() - pattern.len()].to_string();
        }
    }
    value.to_string()
}

/// Replace pattern in string
/// If all is true, replaces all occurrences; otherwise just first
fn replace_pattern(value: &str, pattern: &str, replacement: &str, all: bool) -> String {
    // For now, treat pattern as literal (can be extended to glob patterns)
    if all {
        value.replace(pattern, replacement)
    } else {
        value.replacen(pattern, replacement, 1)
    }
}

/// Extract substring from value
/// Offset can be negative (from end), length is optional
fn substring(value: &str, offset: i32, length: Option<usize>) -> String {
    let len = value.len() as i32;

    // Calculate actual start position
    let start = if offset < 0 {
        // Negative offset: count from end
        let pos = len + offset;
        if pos < 0 {
            0
        } else {
            pos as usize
        }
    } else {
        // Positive offset: from start
        if offset as usize > value.len() {
            return String::new();
        }
        offset as usize
    };

    // Calculate end position
    match length {
        Some(len) => {
            let end = (start + len).min(value.len());
            value[start..end].to_string()
        }
        None => {
            value[start..].to_string()
        }
    }
}

/// Uppercase first character
fn uppercase_first(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => {
            let mut result = first.to_uppercase().to_string();
            result.push_str(chars.as_str());
            result
        }
        None => String::new(),
    }
}

/// Lowercase first character
fn lowercase_first(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => {
            let mut result = first.to_lowercase().to_string();
            result.push_str(chars.as_str());
            result
        }
        None => String::new(),
    }
}

/// Expand multiple words (e.g., command arguments)
/// Each word may expand into multiple words due to brace expansion and glob expansion
pub fn expand_words(words: &[Word], context: &mut Context) -> Result<Vec<String>, ExpansionError> {
    let mut results = Vec::new();
    for word in words {
        // First, detect and parse any brace patterns in literals
        let word_with_braces = detect_brace_patterns(word);

        // Then expand the word (which may produce multiple results due to braces)
        let expanded = expand_word_with_braces(&word_with_braces, context)?;

        // Finally, apply glob expansion to each expanded word
        for expanded_word in expanded {
            let glob_options = crate::glob::GlobOptions {
                match_dotfiles: context.options.dotglob,
                globstar: true,
                nullglob: context.options.nullglob,
                extglob: context.options.extglob,
            };
            match crate::glob::expand_glob(&expanded_word, &glob_options) {
                Ok(mut glob_results) => {
                    results.append(&mut glob_results);
                }
                Err(_) => {
                    // If glob expansion fails, use the literal word
                    results.push(expanded_word);
                }
            }
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rush_parser::Word;

    #[test]
    fn test_expand_literal() {
        let mut ctx = Context::empty();
        let word = Word::from_literal("hello");
        let result = expand_word(&word, &mut ctx).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_expand_simple_var() {
        let mut ctx = Context::empty();
        ctx.set_var("USER", "alice").unwrap();

        let word = Word::new(vec![WordPart::VarExpansion(VarExpansion::Simple(
            "USER".to_string(),
        ))]);

        let result = expand_word(&word, &mut ctx).unwrap();
        assert_eq!(result, "alice");
    }

    #[test]
    fn test_expand_undefined_var() {
        let mut ctx = Context::empty();
        let word = Word::new(vec![WordPart::VarExpansion(VarExpansion::Simple(
            "UNDEFINED".to_string(),
        ))]);

        let result = expand_word(&word, &mut ctx).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_expand_var_with_default() {
        let mut ctx = Context::empty();
        let word = Word::new(vec![WordPart::VarExpansion(VarExpansion::WithDefault {
            name: "UNDEFINED".to_string(),
            default: Box::new(Word::from_literal("default_value")),
        })]);

        let result = expand_word(&word, &mut ctx).unwrap();
        assert_eq!(result, "default_value");
    }

    #[test]
    fn test_expand_mixed_word() {
        let mut ctx = Context::empty();
        ctx.set_var("NAME", "world").unwrap();

        let word = Word::new(vec![
            WordPart::Literal("Hello ".to_string()),
            WordPart::VarExpansion(VarExpansion::Simple("NAME".to_string())),
            WordPart::Literal("!".to_string()),
        ]);

        let result = expand_word(&word, &mut ctx).unwrap();
        assert_eq!(result, "Hello world!");
    }

    #[test]
    fn test_expand_multiple_words() {
        let mut ctx = Context::empty();
        ctx.set_var("CMD", "ls").unwrap();

        let words = vec![
            Word::new(vec![WordPart::VarExpansion(VarExpansion::Simple(
                "CMD".to_string(),
            ))]),
            Word::from_literal("-la"),
        ];

        let result = expand_words(&words, &mut ctx).unwrap();
        assert_eq!(result, vec!["ls", "-la"]);
    }

    #[test]
    fn test_expand_command_substitution() {
        let mut ctx = Context::empty();
        let word = Word::new(vec![WordPart::CommandSubstitution("echo hello".to_string())]);

        let result = expand_word(&word, &mut ctx).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_expand_mixed_with_command_subst() {
        let mut ctx = Context::empty();
        let word = Word::new(vec![
            WordPart::Literal("Result: ".to_string()),
            WordPart::CommandSubstitution("echo success".to_string()),
        ]);

        let result = expand_word(&word, &mut ctx).unwrap();
        assert_eq!(result, "Result: success");
    }

    #[test]
    fn test_indirect_expansion() {
        let mut ctx = Context::empty();
        ctx.set_var("ptr", "target").unwrap();
        ctx.set_var("target", "hello world").unwrap();

        let word = Word::new(vec![WordPart::VarExpansion(VarExpansion::Indirect(
            "ptr".to_string(),
        ))]);

        let result = expand_word(&word, &mut ctx).unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_indirect_expansion_missing() {
        let mut ctx = Context::empty();
        ctx.set_var("ptr", "nonexistent").unwrap();

        let word = Word::new(vec![WordPart::VarExpansion(VarExpansion::Indirect(
            "ptr".to_string(),
        ))]);

        let result = expand_word(&word, &mut ctx).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_transform_quote() {
        let mut ctx = Context::empty();
        ctx.set_var("msg", "hello world").unwrap();

        let word = Word::new(vec![WordPart::VarExpansion(VarExpansion::Transform {
            name: "msg".to_string(),
            op: 'Q',
        })]);

        let result = expand_word(&word, &mut ctx).unwrap();
        assert_eq!(result, "'hello world'");
    }

    #[test]
    fn test_transform_quote_with_apostrophe() {
        let mut ctx = Context::empty();
        ctx.set_var("msg", "don't stop").unwrap();

        let word = Word::new(vec![WordPart::VarExpansion(VarExpansion::Transform {
            name: "msg".to_string(),
            op: 'Q',
        })]);

        let result = expand_word(&word, &mut ctx).unwrap();
        assert_eq!(result, "'don'\\''t stop'");
    }

    #[test]
    fn test_transform_uppercase() {
        let mut ctx = Context::empty();
        ctx.set_var("name", "hello").unwrap();

        let word = Word::new(vec![WordPart::VarExpansion(VarExpansion::Transform {
            name: "name".to_string(),
            op: 'U',
        })]);

        let result = expand_word(&word, &mut ctx).unwrap();
        assert_eq!(result, "HELLO");
    }

    #[test]
    fn test_transform_lowercase() {
        let mut ctx = Context::empty();
        ctx.set_var("name", "HELLO").unwrap();

        let word = Word::new(vec![WordPart::VarExpansion(VarExpansion::Transform {
            name: "name".to_string(),
            op: 'L',
        })]);

        let result = expand_word(&word, &mut ctx).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_transform_escape() {
        let mut ctx = Context::empty();
        ctx.set_var("text", "hello\\nworld").unwrap();

        let word = Word::new(vec![WordPart::VarExpansion(VarExpansion::Transform {
            name: "text".to_string(),
            op: 'E',
        })]);

        let result = expand_word(&word, &mut ctx).unwrap();
        assert_eq!(result, "hello\nworld");
    }
}
