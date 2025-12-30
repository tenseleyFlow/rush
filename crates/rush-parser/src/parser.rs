use pest::Parser;
use pest_derive::Parser;
use thiserror::Error;

use crate::ast::{Assignment, SimpleCommand, Statement, VarExpansion, Word, WordPart};

#[derive(Parser)]
#[grammar = "grammar.pest"]
struct RushParser;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Parse error: {0}")]
    PestError(#[from] Box<pest::error::Error<Rule>>),

    #[error("Unexpected rule: {0:?}")]
    UnexpectedRule(Rule),
}

/// Parse a line of shell input into a Statement
pub fn parse_line(input: &str) -> Result<Statement, ParseError> {
    // Handle empty input or whitespace-only
    if input.trim().is_empty() {
        return Ok(Statement::Empty);
    }

    let mut pairs = RushParser::parse(Rule::input, input)
        .map_err(Box::new)?;

    let input_pair = pairs.next().ok_or_else(|| {
        ParseError::PestError(Box::new(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError {
                message: "No input parsed".to_string(),
            },
            pest::Span::new(input, 0, 0).unwrap(),
        )))
    })?;

    for pair in input_pair.into_inner() {
        match pair.as_rule() {
            Rule::command_line => {
                return parse_command_line(pair);
            }
            Rule::EOI => {}
            _ => return Err(ParseError::UnexpectedRule(pair.as_rule())),
        }
    }

    Ok(Statement::Empty)
}

fn parse_command_line(pair: pest::iterators::Pair<Rule>) -> Result<Statement, ParseError> {
    for inner_pair in pair.into_inner() {
        match inner_pair.as_rule() {
            Rule::simple_command => {
                return Ok(Statement::Simple(parse_simple_command(inner_pair)?));
            }
            _ => return Err(ParseError::UnexpectedRule(inner_pair.as_rule())),
        }
    }
    Ok(Statement::Empty)
}

fn parse_simple_command(pair: pest::iterators::Pair<Rule>) -> Result<SimpleCommand, ParseError> {
    let mut assignments = Vec::new();
    let mut words = Vec::new();

    for inner_pair in pair.into_inner() {
        match inner_pair.as_rule() {
            Rule::assignment => {
                assignments.push(parse_assignment(inner_pair)?);
            }
            Rule::word => {
                words.push(parse_word(inner_pair)?);
            }
            _ => return Err(ParseError::UnexpectedRule(inner_pair.as_rule())),
        }
    }

    Ok(SimpleCommand::new(assignments, words))
}

fn parse_assignment(pair: pest::iterators::Pair<Rule>) -> Result<Assignment, ParseError> {
    let mut name = String::new();
    let mut value = Word::new(vec![]);

    for inner_pair in pair.into_inner() {
        match inner_pair.as_rule() {
            Rule::var_name => {
                name = inner_pair.as_str().to_string();
            }
            Rule::word => {
                value = parse_word(inner_pair)?;
            }
            _ => return Err(ParseError::UnexpectedRule(inner_pair.as_rule())),
        }
    }

    Ok(Assignment::new(name, value))
}

fn parse_word(pair: pest::iterators::Pair<Rule>) -> Result<Word, ParseError> {
    let mut parts = Vec::new();

    for part_pair in pair.into_inner() {
        match part_pair.as_rule() {
            Rule::word_part => {
                parts.extend(parse_word_part(part_pair)?);
            }
            _ => return Err(ParseError::UnexpectedRule(part_pair.as_rule())),
        }
    }

    Ok(Word::new(parts))
}

fn parse_word_part(pair: pest::iterators::Pair<Rule>) -> Result<Vec<WordPart>, ParseError> {
    let inner = pair.into_inner().next().ok_or_else(|| {
        ParseError::UnexpectedRule(Rule::word_part)
    })?;

    match inner.as_rule() {
        Rule::bare_word_part => {
            Ok(vec![WordPart::Literal(inner.as_str().to_string())])
        }
        Rule::var_expansion => {
            Ok(vec![WordPart::VarExpansion(parse_var_expansion(inner)?)])
        }
        Rule::command_substitution => {
            let content = inner.into_inner().next()
                .ok_or_else(|| ParseError::UnexpectedRule(Rule::command_substitution))?
                .as_str()
                .to_string();
            Ok(vec![WordPart::CommandSubstitution(content)])
        }
        Rule::quoted_string => {
            parse_quoted_string(inner)
        }
        _ => Err(ParseError::UnexpectedRule(inner.as_rule())),
    }
}

fn parse_var_expansion(pair: pest::iterators::Pair<Rule>) -> Result<VarExpansion, ParseError> {
    // Get the original text before consuming the pair
    let original = pair.as_str();
    let is_braced = original.starts_with("${");

    let mut inner = pair.into_inner();
    let first = inner.next().ok_or_else(|| {
        ParseError::UnexpectedRule(Rule::var_expansion)
    })?;

    match first.as_rule() {
        Rule::var_name => {
            // Check if there's a modifier
            if let Some(modifier_pair) = inner.next() {
                if modifier_pair.as_rule() == Rule::var_modifier {
                    // ${VAR:-default}
                    let default_word = modifier_pair.into_inner().next()
                        .ok_or_else(|| ParseError::UnexpectedRule(Rule::var_modifier))?;
                    let default = parse_word(default_word)?;
                    return Ok(VarExpansion::WithDefault {
                        name: first.as_str().to_string(),
                        default: Box::new(default),
                    });
                }
            }

            // Determine if it's simple or braced
            if is_braced {
                Ok(VarExpansion::Braced(first.as_str().to_string()))
            } else {
                Ok(VarExpansion::Simple(first.as_str().to_string()))
            }
        }
        _ => Err(ParseError::UnexpectedRule(first.as_rule())),
    }
}

fn parse_quoted_string(pair: pest::iterators::Pair<Rule>) -> Result<Vec<WordPart>, ParseError> {
    let inner = pair.into_inner().next().ok_or_else(|| {
        ParseError::UnexpectedRule(Rule::quoted_string)
    })?;

    match inner.as_rule() {
        Rule::single_quoted => {
            // Single quotes: literal (strip quotes)
            let s = inner.as_str();
            let content = &s[1..s.len() - 1];
            Ok(vec![WordPart::Literal(content.to_string())])
        }
        Rule::double_quoted => {
            // Double quotes: can contain expansions
            parse_double_quoted(inner)
        }
        _ => Err(ParseError::UnexpectedRule(inner.as_rule())),
    }
}

fn parse_double_quoted(pair: pest::iterators::Pair<Rule>) -> Result<Vec<WordPart>, ParseError> {
    let mut parts = Vec::new();

    for inner_pair in pair.into_inner() {
        match inner_pair.as_rule() {
            Rule::double_quoted_content => {
                for content_part in inner_pair.into_inner() {
                    match content_part.as_rule() {
                        Rule::double_quoted_part => {
                            parts.extend(parse_double_quoted_part(content_part)?);
                        }
                        _ => return Err(ParseError::UnexpectedRule(content_part.as_rule())),
                    }
                }
            }
            _ => return Err(ParseError::UnexpectedRule(inner_pair.as_rule())),
        }
    }

    Ok(parts)
}

fn parse_double_quoted_part(pair: pest::iterators::Pair<Rule>) -> Result<Vec<WordPart>, ParseError> {
    let inner = pair.into_inner().next().ok_or_else(|| {
        ParseError::UnexpectedRule(Rule::double_quoted_part)
    })?;

    match inner.as_rule() {
        Rule::double_quoted_text => {
            Ok(vec![WordPart::Literal(inner.as_str().to_string())])
        }
        Rule::var_expansion => {
            Ok(vec![WordPart::VarExpansion(parse_var_expansion(inner)?)])
        }
        Rule::command_substitution => {
            let content = inner.into_inner().next()
                .ok_or_else(|| ParseError::UnexpectedRule(Rule::command_substitution))?
                .as_str()
                .to_string();
            Ok(vec![WordPart::CommandSubstitution(content)])
        }
        _ => Err(ParseError::UnexpectedRule(inner.as_rule())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_command() {
        let result = parse_line("ls").unwrap();
        match result {
            Statement::Simple(cmd) => {
                assert_eq!(cmd.assignments.len(), 0);
                assert_eq!(cmd.words.len(), 1);
                assert!(cmd.words[0].is_literal());
            }
            _ => panic!("Expected Simple command"),
        }
    }

    #[test]
    fn test_parse_with_variable() {
        let result = parse_line("echo $USER").unwrap();
        match result {
            Statement::Simple(cmd) => {
                assert_eq!(cmd.words.len(), 2);
                // First word: "echo"
                assert!(cmd.words[0].is_literal());
                // Second word: $USER
                assert_eq!(cmd.words[1].parts.len(), 1);
                match &cmd.words[1].parts[0] {
                    WordPart::VarExpansion(VarExpansion::Simple(name)) => {
                        assert_eq!(name, "USER");
                    }
                    _ => panic!("Expected variable expansion"),
                }
            }
            _ => panic!("Expected Simple command"),
        }
    }

    #[test]
    fn test_parse_assignment() {
        let result = parse_line("FOO=bar").unwrap();
        match result {
            Statement::Simple(cmd) => {
                assert_eq!(cmd.assignments.len(), 1);
                assert_eq!(cmd.assignments[0].name, "FOO");
                assert!(cmd.assignments[0].value.is_literal());
            }
            _ => panic!("Expected Simple command"),
        }
    }

    #[test]
    fn test_parse_assignment_with_command() {
        let result = parse_line("FOO=bar echo test").unwrap();
        match result {
            Statement::Simple(cmd) => {
                assert_eq!(cmd.assignments.len(), 1);
                assert_eq!(cmd.words.len(), 2);
            }
            _ => panic!("Expected Simple command"),
        }
    }

    #[test]
    fn test_parse_braced_var() {
        let result = parse_line("echo ${VAR}").unwrap();
        match result {
            Statement::Simple(cmd) => {
                match &cmd.words[1].parts[0] {
                    WordPart::VarExpansion(VarExpansion::Braced(name)) => {
                        assert_eq!(name, "VAR");
                    }
                    _ => panic!("Expected braced variable expansion"),
                }
            }
            _ => panic!("Expected Simple command"),
        }
    }

    #[test]
    fn test_parse_command_substitution() {
        let result = parse_line("echo $(pwd)").unwrap();
        match result {
            Statement::Simple(cmd) => {
                match &cmd.words[1].parts[0] {
                    WordPart::CommandSubstitution(content) => {
                        assert_eq!(content, "pwd");
                    }
                    _ => panic!("Expected command substitution"),
                }
            }
            _ => panic!("Expected Simple command"),
        }
    }
}
