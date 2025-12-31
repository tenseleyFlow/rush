//! Heredoc content collection
//! 
//! After parsing a command line, if it contains heredoc markers (<<EOF),
//! we need to collect subsequent lines until we find the delimiter.

use rush_parser::ast::{CommandType, CompleteCommand, Redirect, SimpleCommand, Statement};

/// Check if a statement contains any heredoc redirects that need content
pub fn has_heredocs(statement: &Statement) -> bool {
    match statement {
        Statement::Empty => false,
        Statement::Complete(cmd) => command_has_heredocs(cmd),
        Statement::Script(commands) => commands.iter().any(command_has_heredocs),
    }
}

fn command_has_heredocs(cmd: &CompleteCommand) -> bool {
    match &cmd.command {
        CommandType::Simple(simple_cmd) => simple_command_has_heredocs(simple_cmd),
        CommandType::Pipeline(pipeline) => {
            pipeline.commands.iter().any(simple_command_has_heredocs)
        }
        CommandType::AndOrList(list) => {
            simple_command_has_heredocs(&list.first.commands[0])
                || list.rest.iter().any(|(_, p)| {
                    p.commands.iter().any(simple_command_has_heredocs)
                })
        }
        _ => false,
    }
}

fn simple_command_has_heredocs(cmd: &SimpleCommand) -> bool {
    cmd.redirects.iter().any(|r| matches!(r, Redirect::Heredoc { .. }))
}

/// Get list of all heredoc delimiters that need content
pub fn get_heredoc_delimiters(statement: &Statement) -> Vec<String> {
    let mut delimiters = Vec::new();
    match statement {
        Statement::Empty => {}
        Statement::Complete(cmd) => collect_command_delimiters(cmd, &mut delimiters),
        Statement::Script(commands) => {
            for cmd in commands {
                collect_command_delimiters(cmd, &mut delimiters);
            }
        }
    }
    delimiters
}

fn collect_command_delimiters(cmd: &CompleteCommand, delimiters: &mut Vec<String>) {
    match &cmd.command {
        CommandType::Simple(simple_cmd) => {
            collect_simple_delimiters(simple_cmd, delimiters);
        }
        CommandType::Pipeline(pipeline) => {
            for simple_cmd in &pipeline.commands {
                collect_simple_delimiters(simple_cmd, delimiters);
            }
        }
        CommandType::AndOrList(list) => {
            for simple_cmd in &list.first.commands {
                collect_simple_delimiters(simple_cmd, delimiters);
            }
            for (_, pipeline) in &list.rest {
                for simple_cmd in &pipeline.commands {
                    collect_simple_delimiters(simple_cmd, delimiters);
                }
            }
        }
        _ => {}
    }
}

fn collect_simple_delimiters(cmd: &SimpleCommand, delimiters: &mut Vec<String>) {
    for redirect in &cmd.redirects {
        if let Redirect::Heredoc { delimiter, .. } = redirect {
            delimiters.push(delimiter.clone());
        }
    }
}

/// Fill heredoc content into a statement
pub fn fill_heredoc_content(statement: &mut Statement, content_map: &std::collections::HashMap<String, Vec<String>>) {
    match statement {
        Statement::Empty => {}
        Statement::Complete(cmd) => fill_command_content(cmd, content_map),
        Statement::Script(commands) => {
            for cmd in commands {
                fill_command_content(cmd, content_map);
            }
        }
    }
}

fn fill_command_content(cmd: &mut CompleteCommand, content_map: &std::collections::HashMap<String, Vec<String>>) {
    match &mut cmd.command {
        CommandType::Simple(simple_cmd) => {
            fill_simple_content(simple_cmd, content_map);
        }
        CommandType::Pipeline(pipeline) => {
            for simple_cmd in &mut pipeline.commands {
                fill_simple_content(simple_cmd, content_map);
            }
        }
        CommandType::AndOrList(list) => {
            for simple_cmd in &mut list.first.commands {
                fill_simple_content(simple_cmd, content_map);
            }
            for (_, pipeline) in &mut list.rest {
                for simple_cmd in &mut pipeline.commands {
                    fill_simple_content(simple_cmd, content_map);
                }
            }
        }
        _ => {}
    }
}

fn fill_simple_content(cmd: &mut SimpleCommand, content_map: &std::collections::HashMap<String, Vec<String>>) {
    for redirect in &mut cmd.redirects {
        if let Redirect::Heredoc { delimiter, content, .. } = redirect {
            if let Some(lines) = content_map.get(delimiter) {
                *content = lines.clone();
            }
        }
    }
}
