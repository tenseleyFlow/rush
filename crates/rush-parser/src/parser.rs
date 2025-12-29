use pest::Parser;
use pest_derive::Parser;
use thiserror::Error;

use crate::ast::{SimpleCommand, Statement};

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
    let mut args = Vec::new();

    for word_pair in pair.into_inner() {
        match word_pair.as_rule() {
            Rule::word => {
                let arg = parse_word(word_pair)?;
                args.push(arg);
            }
            _ => return Err(ParseError::UnexpectedRule(word_pair.as_rule())),
        }
    }

    Ok(SimpleCommand::new(args))
}

fn parse_word(pair: pest::iterators::Pair<Rule>) -> Result<String, ParseError> {
    let inner = pair.into_inner().next().ok_or_else(|| {
        ParseError::UnexpectedRule(Rule::word)
    })?;

    match inner.as_rule() {
        Rule::bare_word => Ok(inner.as_str().to_string()),
        Rule::quoted_string => parse_quoted_string(inner),
        _ => Err(ParseError::UnexpectedRule(inner.as_rule())),
    }
}

fn parse_quoted_string(pair: pest::iterators::Pair<Rule>) -> Result<String, ParseError> {
    let inner = pair.into_inner().next().ok_or_else(|| {
        ParseError::UnexpectedRule(Rule::quoted_string)
    })?;

    let content = match inner.as_rule() {
        Rule::double_quoted => {
            let s = inner.as_str();
            // Remove surrounding quotes
            s[1..s.len() - 1].to_string()
        }
        Rule::single_quoted => {
            let s = inner.as_str();
            // Remove surrounding quotes
            s[1..s.len() - 1].to_string()
        }
        _ => return Err(ParseError::UnexpectedRule(inner.as_rule())),
    };

    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_command() {
        let result = parse_line("ls").unwrap();
        match result {
            Statement::Simple(cmd) => {
                assert_eq!(cmd.args, vec!["ls"]);
            }
            _ => panic!("Expected Simple command"),
        }
    }

    #[test]
    fn test_parse_command_with_args() {
        let result = parse_line("echo hello world").unwrap();
        match result {
            Statement::Simple(cmd) => {
                assert_eq!(cmd.args, vec!["echo", "hello", "world"]);
            }
            _ => panic!("Expected Simple command"),
        }
    }

    #[test]
    fn test_parse_quoted_string() {
        let result = parse_line(r#"echo "hello world""#).unwrap();
        match result {
            Statement::Simple(cmd) => {
                assert_eq!(cmd.args, vec!["echo", "hello world"]);
            }
            _ => panic!("Expected Simple command"),
        }
    }

    #[test]
    fn test_parse_empty() {
        let result = parse_line("").unwrap();
        assert_eq!(result, Statement::Empty);
    }

    #[test]
    fn test_parse_comment() {
        let result = parse_line("# this is a comment").unwrap();
        assert_eq!(result, Statement::Empty);
    }
}
