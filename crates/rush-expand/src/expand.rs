use crate::context::Context;
use rush_parser::{VarExpansion, Word, WordPart};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExpansionError {
    #[error("Command substitution not yet implemented: {0}")]
    CommandSubstitutionNotImplemented(String),

    #[error("Expansion error: {0}")]
    Other(String),
}

/// Expand a Word into a String by resolving all expansions
pub fn expand_word(word: &Word, context: &Context) -> Result<String, ExpansionError> {
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
                // Will implement this next
                return Err(ExpansionError::CommandSubstitutionNotImplemented(
                    cmd.clone(),
                ));
            }
        }
    }

    Ok(result)
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
pub fn expand_words(words: &[Word], context: &Context) -> Result<Vec<String>, ExpansionError> {
    words.iter().map(|w| expand_word(w, context)).collect()
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
}
