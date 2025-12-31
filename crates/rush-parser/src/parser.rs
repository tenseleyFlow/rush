use pest::Parser;
use pest_derive::Parser;
use thiserror::Error;

use crate::ast::{
    AndOrList, AndOrOp, Assignment, CaseClause, CaseStatement, CommandType, CompleteCommand,
    ElifClause, ForStatement, IfStatement, Pipeline, Redirect, SimpleCommand, Statement,
    VarExpansion, WhileStatement, Word, WordPart,
};

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
    let mut commands = Vec::new();

    for inner_pair in pair.into_inner() {
        match inner_pair.as_rule() {
            Rule::complete_command => {
                commands.push(parse_complete_command(inner_pair)?);
            }
            _ => return Err(ParseError::UnexpectedRule(inner_pair.as_rule())),
        }
    }

    match commands.len() {
        0 => Ok(Statement::Empty),
        1 => Ok(Statement::Complete(commands.into_iter().next().unwrap())),
        _ => Ok(Statement::Script(commands)),
    }
}

fn parse_complete_command(pair: pest::iterators::Pair<Rule>) -> Result<CompleteCommand, ParseError> {
    let mut inner_pairs = pair.into_inner();
    let command_pair = inner_pairs.next().ok_or_else(|| {
        ParseError::UnexpectedRule(Rule::complete_command)
    })?;

    // Check for background marker
    let background = inner_pairs
        .next()
        .map(|p| p.as_rule() == Rule::background_marker)
        .unwrap_or(false);

    let command = match command_pair.as_rule() {
        Rule::if_statement => CommandType::If(parse_if_statement(command_pair)?),
        Rule::while_statement => CommandType::While(parse_while_statement(command_pair)?),
        Rule::for_statement => CommandType::For(parse_for_statement(command_pair)?),
        Rule::case_statement => CommandType::Case(parse_case_statement(command_pair)?),
        Rule::and_or_list => parse_and_or_list_type(command_pair)?,
        _ => return Err(ParseError::UnexpectedRule(command_pair.as_rule())),
    };

    Ok(CompleteCommand::new(command, background))
}

fn parse_and_or_list_type(pair: pest::iterators::Pair<Rule>) -> Result<CommandType, ParseError> {
    let mut pipelines = Vec::new();
    let mut operators = Vec::new();

    for inner_pair in pair.into_inner() {
        match inner_pair.as_rule() {
            Rule::pipeline => {
                pipelines.push(parse_pipeline(inner_pair)?);
            }
            Rule::and_or_op => {
                let op_str = inner_pair.as_str();
                operators.push(match op_str {
                    "&&" => AndOrOp::And,
                    "||" => AndOrOp::Or,
                    _ => return Err(ParseError::UnexpectedRule(inner_pair.as_rule())),
                });
            }
            _ => return Err(ParseError::UnexpectedRule(inner_pair.as_rule())),
        }
    }

    // If there's only one pipeline with no operators, return it directly
    if pipelines.len() == 1 && operators.is_empty() {
        let pipeline = pipelines.into_iter().next().unwrap();
        // If it's a single-command pipeline, return as Simple
        if pipeline.commands.len() == 1 {
            return Ok(CommandType::Simple(
                pipeline.commands.into_iter().next().unwrap()
            ));
        } else {
            return Ok(CommandType::Pipeline(pipeline));
        }
    }

    // Build the AndOrList
    let mut pipelines_iter = pipelines.into_iter();
    let first = pipelines_iter.next().unwrap();
    let rest: Vec<(AndOrOp, Pipeline)> = operators
        .into_iter()
        .zip(pipelines_iter)
        .collect();

    Ok(CommandType::AndOrList(AndOrList::new(first, rest)))
}

fn parse_pipeline(pair: pest::iterators::Pair<Rule>) -> Result<Pipeline, ParseError> {
    let mut commands = Vec::new();

    for inner_pair in pair.into_inner() {
        match inner_pair.as_rule() {
            Rule::simple_command => {
                commands.push(parse_simple_command(inner_pair)?);
            }
            _ => return Err(ParseError::UnexpectedRule(inner_pair.as_rule())),
        }
    }

    Ok(Pipeline::new(commands))
}

fn parse_simple_command(pair: pest::iterators::Pair<Rule>) -> Result<SimpleCommand, ParseError> {
    let mut assignments = Vec::new();
    let mut words = Vec::new();
    let mut redirects = Vec::new();

    for inner_pair in pair.into_inner() {
        match inner_pair.as_rule() {
            Rule::assignment => {
                assignments.push(parse_assignment(inner_pair)?);
            }
            Rule::word => {
                words.push(parse_word(inner_pair)?);
            }
            Rule::redirect => {
                redirects.push(parse_redirect(inner_pair)?);
            }
            _ => return Err(ParseError::UnexpectedRule(inner_pair.as_rule())),
        }
    }

    if redirects.is_empty() {
        Ok(SimpleCommand::new(assignments, words))
    } else {
        Ok(SimpleCommand::with_redirects(assignments, words, redirects))
    }
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
        Rule::arithmetic_expansion => {
            let content = inner.into_inner().next()
                .ok_or_else(|| ParseError::UnexpectedRule(Rule::arithmetic_expansion))?
                .as_str()
                .to_string();
            Ok(vec![WordPart::ArithmeticExpansion(content)])
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
        Rule::arithmetic_expansion => {
            let content = inner.into_inner().next()
                .ok_or_else(|| ParseError::UnexpectedRule(Rule::arithmetic_expansion))?
                .as_str()
                .to_string();
            Ok(vec![WordPart::ArithmeticExpansion(content)])
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

fn parse_redirect(pair: pest::iterators::Pair<Rule>) -> Result<Redirect, ParseError> {
    let inner = pair.into_inner().next().ok_or_else(|| {
        ParseError::UnexpectedRule(Rule::redirect)
    })?;

    match inner.as_rule() {
        Rule::redirect_input => {
            let word = inner.into_inner().next()
                .ok_or_else(|| ParseError::UnexpectedRule(Rule::redirect_input))?;
            Ok(Redirect::Input {
                file: parse_word(word)?,
            })
        }
        Rule::redirect_output => {
            let mut fd = None;
            let mut file_word = None;

            for inner_pair in inner.into_inner() {
                match inner_pair.as_rule() {
                    Rule::fd_number => {
                        fd = Some(inner_pair.as_str().parse::<u32>().map_err(|_| {
                            ParseError::UnexpectedRule(Rule::fd_number)
                        })?);
                    }
                    Rule::word => {
                        file_word = Some(parse_word(inner_pair)?);
                    }
                    _ => return Err(ParseError::UnexpectedRule(inner_pair.as_rule())),
                }
            }

            Ok(Redirect::Output {
                fd,
                file: file_word.ok_or_else(|| ParseError::UnexpectedRule(Rule::redirect_output))?,
            })
        }
        Rule::redirect_output_append => {
            let mut fd = None;
            let mut file_word = None;

            for inner_pair in inner.into_inner() {
                match inner_pair.as_rule() {
                    Rule::fd_number => {
                        fd = Some(inner_pair.as_str().parse::<u32>().map_err(|_| {
                            ParseError::UnexpectedRule(Rule::fd_number)
                        })?);
                    }
                    Rule::word => {
                        file_word = Some(parse_word(inner_pair)?);
                    }
                    _ => return Err(ParseError::UnexpectedRule(inner_pair.as_rule())),
                }
            }

            Ok(Redirect::OutputAppend {
                fd,
                file: file_word.ok_or_else(|| ParseError::UnexpectedRule(Rule::redirect_output_append))?,
            })
        }
        Rule::redirect_stderr_to_stdout => {
            Ok(Redirect::StderrToStdout)
        }
        Rule::redirect_all_output => {
            // Check if it's &>> (append) or &> (truncate)
            let text = inner.as_str();
            let append = text.starts_with("&>>");

            let word = inner.into_inner().next()
                .ok_or_else(|| ParseError::UnexpectedRule(Rule::redirect_all_output))?;

            Ok(Redirect::AllOutput {
                file: parse_word(word)?,
                append,
            })
        }
        _ => Err(ParseError::UnexpectedRule(inner.as_rule())),
    }
}

fn parse_if_statement(pair: pest::iterators::Pair<Rule>) -> Result<IfStatement, ParseError> {
    let mut condition = None;
    let mut then_body = Vec::new();
    let mut elif_clauses = Vec::new();
    let mut else_body = None;

    for inner_pair in pair.into_inner() {
        match inner_pair.as_rule() {
            Rule::complete_command => {
                if condition.is_none() {
                    condition = Some(Box::new(parse_complete_command(inner_pair)?));
                }
            }
            Rule::command_list => {
                if condition.is_some() && then_body.is_empty() {
                    then_body = parse_command_list(inner_pair)?;
                }
            }
            Rule::elif_clause => {
                elif_clauses.push(parse_elif_clause(inner_pair)?);
            }
            Rule::else_clause => {
                else_body = Some(parse_else_clause(inner_pair)?);
            }
            Rule::NEWLINE => {}, // Ignore newlines
            _ => {}, // Ignore keywords like "if", "then", "fi"
        }
    }

    Ok(IfStatement::new(
        condition.ok_or_else(|| ParseError::UnexpectedRule(Rule::if_statement))?,
        then_body,
        elif_clauses,
        else_body,
    ))
}

fn parse_elif_clause(pair: pest::iterators::Pair<Rule>) -> Result<ElifClause, ParseError> {
    let mut condition = None;
    let mut then_body = Vec::new();

    for inner_pair in pair.into_inner() {
        match inner_pair.as_rule() {
            Rule::complete_command => {
                condition = Some(Box::new(parse_complete_command(inner_pair)?));
            }
            Rule::command_list => {
                then_body = parse_command_list(inner_pair)?;
            }
            Rule::NEWLINE => {},
            _ => {},
        }
    }

    Ok(ElifClause::new(
        condition.ok_or_else(|| ParseError::UnexpectedRule(Rule::elif_clause))?,
        then_body,
    ))
}

fn parse_else_clause(pair: pest::iterators::Pair<Rule>) -> Result<Vec<CompleteCommand>, ParseError> {
    for inner_pair in pair.into_inner() {
        match inner_pair.as_rule() {
            Rule::command_list => {
                return parse_command_list(inner_pair);
            }
            Rule::NEWLINE => {},
            _ => {},
        }
    }
    Ok(Vec::new())
}

fn parse_while_statement(pair: pest::iterators::Pair<Rule>) -> Result<WhileStatement, ParseError> {
    let mut condition = None;
    let mut body = Vec::new();

    for inner_pair in pair.into_inner() {
        match inner_pair.as_rule() {
            Rule::complete_command => {
                condition = Some(Box::new(parse_complete_command(inner_pair)?));
            }
            Rule::command_list => {
                body = parse_command_list(inner_pair)?;
            }
            Rule::NEWLINE => {},
            _ => {},
        }
    }

    Ok(WhileStatement::new(
        condition.ok_or_else(|| ParseError::UnexpectedRule(Rule::while_statement))?,
        body,
    ))
}

fn parse_for_statement(pair: pest::iterators::Pair<Rule>) -> Result<ForStatement, ParseError> {
    let mut var_name = String::new();
    let mut words = Vec::new();
    let mut body = Vec::new();

    for inner_pair in pair.into_inner() {
        match inner_pair.as_rule() {
            Rule::var_name => {
                var_name = inner_pair.as_str().to_string();
            }
            Rule::word => {
                words.push(parse_word(inner_pair)?);
            }
            Rule::command_list => {
                body = parse_command_list(inner_pair)?;
            }
            Rule::NEWLINE => {},
            _ => {},
        }
    }

    Ok(ForStatement::new(var_name, words, body))
}

fn parse_case_statement(pair: pest::iterators::Pair<Rule>) -> Result<CaseStatement, ParseError> {
    let mut word = None;
    let mut clauses = Vec::new();

    for inner_pair in pair.into_inner() {
        match inner_pair.as_rule() {
            Rule::word => {
                if word.is_none() {
                    word = Some(parse_word(inner_pair)?);
                }
            }
            Rule::case_clause => {
                clauses.push(parse_case_clause(inner_pair)?);
            }
            Rule::NEWLINE => {},
            _ => {},
        }
    }

    Ok(CaseStatement::new(
        word.ok_or_else(|| ParseError::UnexpectedRule(Rule::case_statement))?,
        clauses,
    ))
}

fn parse_case_clause(pair: pest::iterators::Pair<Rule>) -> Result<CaseClause, ParseError> {
    let mut patterns = Vec::new();
    let mut body = Vec::new();

    for inner_pair in pair.into_inner() {
        match inner_pair.as_rule() {
            Rule::pattern => {
                // Pattern contains a word
                for pattern_part in inner_pair.into_inner() {
                    if pattern_part.as_rule() == Rule::word {
                        patterns.push(parse_word(pattern_part)?);
                    }
                }
            }
            Rule::command_list => {
                body = parse_command_list(inner_pair)?;
            }
            Rule::NEWLINE => {},
            _ => {},
        }
    }

    Ok(CaseClause::new(patterns, body))
}

fn parse_command_list(pair: pest::iterators::Pair<Rule>) -> Result<Vec<CompleteCommand>, ParseError> {
    let mut commands = Vec::new();

    for inner_pair in pair.into_inner() {
        match inner_pair.as_rule() {
            Rule::complete_command => {
                commands.push(parse_complete_command(inner_pair)?);
            }
            Rule::NEWLINE => {},
            _ => {},
        }
    }

    Ok(commands)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_command() {
        let result = parse_line("ls").unwrap();
        match result {
            Statement::Complete(complete_cmd) => {
                if let CommandType::Simple(cmd) = &complete_cmd.command {
                assert_eq!(cmd.assignments.len(), 0);
                assert_eq!(cmd.words.len(), 1);
                assert!(cmd.words[0].is_literal());
                } else {
                    panic!("Expected Simple or Pipeline command");
                }
            }
            _ => panic!("Expected Simple command"),
        }
    }

    #[test]
    fn test_parse_with_variable() {
        let result = parse_line("echo $USER").unwrap();
        match result {
            Statement::Complete(complete_cmd) => {
                if let CommandType::Simple(cmd) = &complete_cmd.command {
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
                } else {
                    panic!("Expected Simple or Pipeline command");
                }
            }
            _ => panic!("Expected Simple command"),
        }
    }

    #[test]
    fn test_parse_assignment() {
        let result = parse_line("FOO=bar").unwrap();
        match result {
            Statement::Complete(complete_cmd) => {
                if let CommandType::Simple(cmd) = &complete_cmd.command {
                assert_eq!(cmd.assignments.len(), 1);
                assert_eq!(cmd.assignments[0].name, "FOO");
                assert!(cmd.assignments[0].value.is_literal());
                } else {
                    panic!("Expected Simple or Pipeline command");
                }
            }
            _ => panic!("Expected Simple command"),
        }
    }

    #[test]
    fn test_parse_assignment_with_command() {
        let result = parse_line("FOO=bar echo test").unwrap();
        match result {
            Statement::Complete(complete_cmd) => {
                if let CommandType::Simple(cmd) = &complete_cmd.command {
                assert_eq!(cmd.assignments.len(), 1);
                assert_eq!(cmd.words.len(), 2);
                } else {
                    panic!("Expected Simple or Pipeline command");
                }
            }
            _ => panic!("Expected Simple command"),
        }
    }

    #[test]
    fn test_parse_braced_var() {
        let result = parse_line("echo ${VAR}").unwrap();
        match result {
            Statement::Complete(complete_cmd) => {
                if let CommandType::Simple(cmd) = &complete_cmd.command {
                match &cmd.words[1].parts[0] {
                    WordPart::VarExpansion(VarExpansion::Braced(name)) => {
                        assert_eq!(name, "VAR");
                    }
                    _ => panic!("Expected braced variable expansion"),
                }
                } else {
                    panic!("Expected Simple or Pipeline command");
                }
            }
            _ => panic!("Expected Simple command"),
        }
    }

    #[test]
    fn test_parse_command_substitution() {
        let result = parse_line("echo $(pwd)").unwrap();
        match result {
            Statement::Complete(complete_cmd) => {
                if let CommandType::Simple(cmd) = &complete_cmd.command {
                match &cmd.words[1].parts[0] {
                    WordPart::CommandSubstitution(content) => {
                        assert_eq!(content, "pwd");
                    }
                    _ => panic!("Expected command substitution"),
                }
                } else {
                    panic!("Expected Simple or Pipeline command");
                }
            }
            _ => panic!("Expected Simple command"),
        }
    }

    #[test]
    fn test_parse_simple_pipeline() {
        let result = parse_line("ls | grep test").unwrap();
        match result {
            Statement::Complete(complete_cmd) => {
                if let CommandType::Pipeline(pipeline) = &complete_cmd.command {
                assert_eq!(pipeline.commands.len(), 2);
                assert_eq!(pipeline.commands[0].words.len(), 1);
                assert_eq!(pipeline.commands[1].words.len(), 2);
                } else {
                    panic!("Expected Simple or Pipeline command");
                }
            }
            _ => panic!("Expected Pipeline"),
        }
    }

    #[test]
    fn test_parse_three_command_pipeline() {
        let result = parse_line("ls -la | grep rush | wc -l").unwrap();
        match result {
            Statement::Complete(complete_cmd) => {
                if let CommandType::Pipeline(pipeline) = &complete_cmd.command {
                assert_eq!(pipeline.commands.len(), 3);
                } else {
                    panic!("Expected Simple or Pipeline command");
                }
            }
            _ => panic!("Expected Pipeline"),
        }
    }

    #[test]
    fn test_parse_input_redirect() {
        let result = parse_line("cat <file.txt").unwrap();
        match result {
            Statement::Complete(complete_cmd) => {
                if let CommandType::Simple(cmd) = &complete_cmd.command {
                assert_eq!(cmd.redirects.len(), 1);
                match &cmd.redirects[0] {
                    Redirect::Input { file } => {
                        assert!(file.is_literal());
                    }
                    _ => panic!("Expected Input redirect"),
                }
                } else {
                    panic!("Expected Simple or Pipeline command");
                }
            }
            _ => panic!("Expected Simple command"),
        }
    }

    #[test]
    fn test_parse_output_redirect() {
        let result = parse_line("echo hello >output.txt").unwrap();
        match result {
            Statement::Complete(complete_cmd) => {
                if let CommandType::Simple(cmd) = &complete_cmd.command {
                assert_eq!(cmd.redirects.len(), 1);
                match &cmd.redirects[0] {
                    Redirect::Output { fd, file } => {
                        assert_eq!(*fd, None);
                        assert!(file.is_literal());
                    }
                    _ => panic!("Expected Output redirect"),
                }
                } else {
                    panic!("Expected Simple or Pipeline command");
                }
            }
            _ => panic!("Expected Simple command"),
        }
    }

    #[test]
    fn test_parse_append_redirect() {
        let result = parse_line("echo hello >>output.txt").unwrap();
        match result {
            Statement::Complete(complete_cmd) => {
                if let CommandType::Simple(cmd) = &complete_cmd.command {
                assert_eq!(cmd.redirects.len(), 1);
                match &cmd.redirects[0] {
                    Redirect::OutputAppend { fd, file } => {
                        assert_eq!(*fd, None);
                        assert!(file.is_literal());
                    }
                    _ => panic!("Expected OutputAppend redirect"),
                }
                } else {
                    panic!("Expected Simple or Pipeline command");
                }
            }
            _ => panic!("Expected Simple command"),
        }
    }

    #[test]
    fn test_parse_stderr_redirect() {
        let result = parse_line("command 2>error.log").unwrap();
        match result {
            Statement::Complete(complete_cmd) => {
                if let CommandType::Simple(cmd) = &complete_cmd.command {
                assert_eq!(cmd.redirects.len(), 1);
                match &cmd.redirects[0] {
                    Redirect::Output { fd, file } => {
                        assert_eq!(*fd, Some(2));
                        assert!(file.is_literal());
                    }
                    _ => panic!("Expected Output redirect with fd 2"),
                }
                } else {
                    panic!("Expected Simple or Pipeline command");
                }
            }
            _ => panic!("Expected Simple command"),
        }
    }

    #[test]
    fn test_parse_stderr_to_stdout() {
        let result = parse_line("command 2>&1").unwrap();
        match result {
            Statement::Complete(complete_cmd) => {
                if let CommandType::Simple(cmd) = &complete_cmd.command {
                assert_eq!(cmd.redirects.len(), 1);
                match &cmd.redirects[0] {
                    Redirect::StderrToStdout => {}
                    _ => panic!("Expected StderrToStdout redirect"),
                }
                } else {
                    panic!("Expected Simple or Pipeline command");
                }
            }
            _ => panic!("Expected Simple command"),
        }
    }

    #[test]
    fn test_parse_all_output_redirect() {
        let result = parse_line("command &>output.txt").unwrap();
        match result {
            Statement::Complete(complete_cmd) => {
                if let CommandType::Simple(cmd) = &complete_cmd.command {
                assert_eq!(cmd.redirects.len(), 1);
                match &cmd.redirects[0] {
                    Redirect::AllOutput { file, append } => {
                        assert!(!append);
                        assert!(file.is_literal());
                    }
                    _ => panic!("Expected AllOutput redirect"),
                }
                } else {
                    panic!("Expected Simple or Pipeline command");
                }
            }
            _ => panic!("Expected Simple command"),
        }
    }

    #[test]
    fn test_parse_pipeline_with_redirects() {
        let result = parse_line("ls >list.txt | grep test").unwrap();
        match result {
            Statement::Complete(complete_cmd) => {
                if let CommandType::Pipeline(pipeline) = &complete_cmd.command {
                assert_eq!(pipeline.commands.len(), 2);
                assert_eq!(pipeline.commands[0].redirects.len(), 1);
                } else {
                    panic!("Expected Simple or Pipeline command");
                }
            }
            _ => panic!("Expected Pipeline"),
        }
    }

    #[test]
    fn test_parse_if_statement_newlines() {
        let input = "if test 5 -eq 5\nthen\necho equal\nfi";
        let result = parse_line(input);
        match result {
            Ok(Statement::Complete(cmd)) if matches!(cmd.command, CommandType::If(_)) => {}
            Ok(other) => panic!("Expected If, got: {:?}", other),
            Err(e) => panic!("Parse error: {}", e),
        }
    }

    #[test]
    fn test_parse_if_statement() {
        let input = "if test 5 -eq 5; then echo equal; fi";
        let result = parse_line(input);
        match result {
            Ok(Statement::Complete(cmd)) if matches!(cmd.command, CommandType::If(_)) => {}
            Ok(other) => panic!("Expected If, got: {:?}", other),
            Err(e) => panic!("Parse error: {}", e),
        }
    }

    #[test]
    fn test_parse_multiline_script() {
        let input = "echo hello\necho world\n";
        let result = parse_line(input);
        match result {
            Ok(Statement::Script(cmds)) => {
                assert_eq!(cmds.len(), 2);
            }
            Ok(other) => panic!("Expected Script, got: {:?}", other),
            Err(e) => panic!("Parse error: {}", e),
        }
    }

    #[test]
    fn test_parse_while_loop() {
        let input = "while false; do echo test; done";
        let result = parse_line(input);
        match result {
            Ok(Statement::Complete(cmd)) if matches!(cmd.command, CommandType::While(_)) => {}
            Ok(other) => panic!("Expected While, got: {:?}", other),
            Err(e) => panic!("Parse error: {}", e),
        }
    }

    #[test]
    fn test_parse_while_loop_multiline() {
        let input = "while false\ndo\necho test\ndone";
        let result = parse_line(input);
        match result {
            Ok(Statement::Complete(cmd)) if matches!(cmd.command, CommandType::While(_)) => {}
            Ok(other) => panic!("Expected While, got: {:?}", other),
            Err(e) => panic!("Parse error: {}", e),
        }
    }

    #[test]
    fn test_parse_case_simple() {
        let input = "case x in a) echo matched;; esac";
        let result = parse_line(input);
        match result {
            Ok(Statement::Complete(cmd)) if matches!(cmd.command, CommandType::Case(_)) => {}
            Ok(other) => panic!("Expected Case, got: {:?}", other),
            Err(e) => panic!("Parse error: {}", e),
        }
    }

    #[test]
    fn test_parse_case_two_clauses() {
        let input = "case x in a) echo first;; b) echo second;; esac";
        let result = parse_line(input);
        match result {
            Ok(Statement::Complete(cmd)) if matches!(cmd.command, CommandType::Case(_)) => {}
            Ok(other) => panic!("Expected Case, got: {:?}", other),
            Err(e) => panic!("Parse error: {}", e),
        }
    }

    #[test]
    fn test_parse_background_execution() {
        let input = "sleep 10 &";
        let result = parse_line(input);
        match result {
            Ok(Statement::Complete(cmd)) => {
                assert!(cmd.background, "Command should be marked as background");
                assert!(matches!(cmd.command, CommandType::Simple(_)));
            }
            Ok(other) => panic!("Expected Complete, got: {:?}", other),
            Err(e) => panic!("Parse error: {}", e),
        }
    }

    #[test]
    fn test_parse_foreground_execution() {
        let input = "sleep 10";
        let result = parse_line(input);
        match result {
            Ok(Statement::Complete(cmd)) => {
                assert!(!cmd.background, "Command should not be marked as background");
            }
            Ok(other) => panic!("Expected Complete, got: {:?}", other),
            Err(e) => panic!("Parse error: {}", e),
        }
    }
}
