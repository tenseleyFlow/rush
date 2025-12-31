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

    #[error("Expansion error: {0}")]
    Other(String),
}

/// Expand a Word into one or more Strings (due to brace expansion)
pub fn expand_word_with_braces(word: &Word, context: &Context) -> Result<Vec<String>, ExpansionError> {
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
fn expand_word_simple(word: &Word, context: &Context) -> Result<String, ExpansionError> {
    let mut result = String::new();

    for part in &word.parts {
        match part {
            WordPart::Literal(s) => {
                result.push_str(s);
            }
            WordPart::VarExpansion(var_exp) => {
                let expanded = expand_var(var_exp, context)?;
                result.push_str(&expanded);
            }
            WordPart::CommandSubstitution(cmd) => {
                let output = execute_command_substitution(cmd)?;
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
        }
    }

    Ok(result)
}

/// Expand a Word into a String by resolving all expansions
/// (Kept for backward compatibility - delegates to new function)
pub fn expand_word(word: &Word, context: &Context) -> Result<String, ExpansionError> {
    let results = expand_word_with_braces(word, context)?;
    Ok(results.join(" "))
}

/// Expand a variable reference
fn expand_var(var_exp: &VarExpansion, context: &Context) -> Result<String, ExpansionError> {
    match var_exp {
        VarExpansion::Simple(name) | VarExpansion::Braced(name) => {
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
    }
}

/// Expand multiple words (e.g., command arguments)
/// Each word may expand into multiple words due to brace expansion
pub fn expand_words(words: &[Word], context: &Context) -> Result<Vec<String>, ExpansionError> {
    let mut results = Vec::new();
    for word in words {
        // First, detect and parse any brace patterns in literals
        let word_with_braces = detect_brace_patterns(word);

        // Then expand the word (which may produce multiple results due to braces)
        let mut expanded = expand_word_with_braces(&word_with_braces, context)?;
        results.append(&mut expanded);
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rush_parser::Word;

    #[test]
    fn test_expand_literal() {
        let ctx = Context::empty();
        let word = Word::from_literal("hello");
        let result = expand_word(&word, &ctx).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_expand_simple_var() {
        let mut ctx = Context::empty();
        ctx.set_var("USER", "alice");

        let word = Word::new(vec![WordPart::VarExpansion(VarExpansion::Simple(
            "USER".to_string(),
        ))]);

        let result = expand_word(&word, &ctx).unwrap();
        assert_eq!(result, "alice");
    }

    #[test]
    fn test_expand_undefined_var() {
        let ctx = Context::empty();
        let word = Word::new(vec![WordPart::VarExpansion(VarExpansion::Simple(
            "UNDEFINED".to_string(),
        ))]);

        let result = expand_word(&word, &ctx).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_expand_var_with_default() {
        let ctx = Context::empty();
        let word = Word::new(vec![WordPart::VarExpansion(VarExpansion::WithDefault {
            name: "UNDEFINED".to_string(),
            default: Box::new(Word::from_literal("default_value")),
        })]);

        let result = expand_word(&word, &ctx).unwrap();
        assert_eq!(result, "default_value");
    }

    #[test]
    fn test_expand_mixed_word() {
        let mut ctx = Context::empty();
        ctx.set_var("NAME", "world");

        let word = Word::new(vec![
            WordPart::Literal("Hello ".to_string()),
            WordPart::VarExpansion(VarExpansion::Simple("NAME".to_string())),
            WordPart::Literal("!".to_string()),
        ]);

        let result = expand_word(&word, &ctx).unwrap();
        assert_eq!(result, "Hello world!");
    }

    #[test]
    fn test_expand_multiple_words() {
        let mut ctx = Context::empty();
        ctx.set_var("CMD", "ls");

        let words = vec![
            Word::new(vec![WordPart::VarExpansion(VarExpansion::Simple(
                "CMD".to_string(),
            ))]),
            Word::from_literal("-la"),
        ];

        let result = expand_words(&words, &ctx).unwrap();
        assert_eq!(result, vec!["ls", "-la"]);
    }

    #[test]
    fn test_expand_command_substitution() {
        let ctx = Context::empty();
        let word = Word::new(vec![WordPart::CommandSubstitution("echo hello".to_string())]);

        let result = expand_word(&word, &ctx).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_expand_mixed_with_command_subst() {
        let ctx = Context::empty();
        let word = Word::new(vec![
            WordPart::Literal("Result: ".to_string()),
            WordPart::CommandSubstitution("echo success".to_string()),
        ]);

        let result = expand_word(&word, &ctx).unwrap();
        assert_eq!(result, "Result: success");
    }
}
