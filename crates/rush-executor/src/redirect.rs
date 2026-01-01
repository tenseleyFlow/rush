use rush_expand::Context;
use rush_parser::ast::{CommandType, Redirect};
use std::fs::{File, OpenOptions};
use std::io;
use std::process::{Command, Stdio};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RedirectError {
    #[error("Failed to open file: {0}")]
    FileOpenError(#[from] io::Error),

    #[error("Invalid file descriptor: {0}")]
    InvalidFileDescriptor(u32),

    #[error("Expansion error: {0}")]
    ExpansionError(String),
}

/// Apply redirections to a Command
///
/// This function processes all redirections and configures the Command's stdio accordingly.
/// Returns optional stdin content for heredocs/herestrings.
pub fn apply_redirects(
    cmd: &mut Command,
    redirects: &[Redirect],
    context: &mut Context,
) -> Result<Option<String>, RedirectError> {
    let mut stdin_content = None;
    for redirect in redirects {
        if let Some(content) = apply_single_redirect(cmd, redirect, context)? {
            stdin_content = Some(content);
        }
    }
    Ok(stdin_content)
}

fn apply_single_redirect(
    cmd: &mut Command,
    redirect: &Redirect,
    context: &mut Context,
) -> Result<Option<String>, RedirectError> {
    match redirect {
        Redirect::Input { file } => {
            // Expand the filename
            let filename = rush_expand::expand_word(file, context)
                .map_err(|e| RedirectError::ExpansionError(e.to_string()))?;

            // Open file for reading
            let file_handle = File::open(&filename)?;
            cmd.stdin(file_handle);
            Ok(None)
        }

        Redirect::Output { fd, file } => {
            // Expand the filename
            let filename = rush_expand::expand_word(file, context)
                .map_err(|e| RedirectError::ExpansionError(e.to_string()))?;

            // Open file for writing (truncate)
            let file_handle = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&filename)?;

            match fd {
                None | Some(1) => {
                    // Redirect stdout
                    cmd.stdout(file_handle);
                }
                Some(2) => {
                    // Redirect stderr
                    cmd.stderr(file_handle);
                }
                Some(fd_num) => {
                    return Err(RedirectError::InvalidFileDescriptor(*fd_num));
                }
            }
            Ok(None)
        }

        Redirect::OutputAppend { fd, file } => {
            // Expand the filename
            let filename = rush_expand::expand_word(file, context)
                .map_err(|e| RedirectError::ExpansionError(e.to_string()))?;

            // Open file for appending
            let file_handle = OpenOptions::new()
                .write(true)
                .create(true)
                .append(true)
                .open(&filename)?;

            match fd {
                None | Some(1) => {
                    // Redirect stdout
                    cmd.stdout(file_handle);
                }
                Some(2) => {
                    // Redirect stderr
                    cmd.stderr(file_handle);
                }
                Some(fd_num) => {
                    return Err(RedirectError::InvalidFileDescriptor(*fd_num));
                }
            }
            Ok(None)
        }

        Redirect::StderrToStdout => {
            // Redirect stderr to stdout
            // Note: This is a simplified version. Proper implementation requires
            // using unsafe dup2 system calls to redirect fd 2 to fd 1
            cmd.stderr(Stdio::inherit());
            Ok(None)
        }

        Redirect::AllOutput { file, append } => {
            // Expand the filename
            let filename = rush_expand::expand_word(file, context)
                .map_err(|e| RedirectError::ExpansionError(e.to_string()))?;

            // Open file
            let file_handle = if *append {
                OpenOptions::new()
                    .write(true)
                    .create(true)
                    .append(true)
                    .open(&filename)?
            } else {
                OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&filename)?
            };

            // Redirect both stdout and stderr
            // Note: This requires duplicating the file handle
            cmd.stdout(file_handle.try_clone()?);
            cmd.stderr(file_handle);
            Ok(None)
        }

        Redirect::Heredoc { delimiter: _, content, strip_tabs, expand } => {
            // Process heredoc content
            let mut lines = content.clone();

            // Strip leading tabs if requested
            if *strip_tabs {
                lines = lines.iter()
                    .map(|line| line.trim_start_matches('\t'))
                    .map(|s| s.to_string())
                    .collect();
            }

            // Expand variables if requested
            if *expand {
                let expanded_lines: Result<Vec<String>, _> = lines.iter()
                    .map(|line| {
                        // Parse the line to detect variables and other expansions
                        // We need to parse it as a simple command and extract the word
                        let fake_cmd = format!("echo {}", line);
                        match rush_parser::parse_line(&fake_cmd) {
                            Ok(rush_parser::Statement::Complete(cmd)) => {
                                match &cmd.command {
                                    CommandType::Simple(simple) => {
                                        // Get the words (skip "echo")
                                        if simple.words.len() > 1 {
                                            rush_expand::expand_words(&simple.words[1..], context)
                                                .map(|words| words.join(" "))
                                                .map_err(|e| RedirectError::ExpansionError(e.to_string()))
                                        } else {
                                            Ok(line.clone())
                                        }
                                    }
                                    _ => Ok(line.clone()),
                                }
                            }
                            _ => Ok(line.clone()),
                        }
                    })
                    .collect();
                lines = expanded_lines?;
            }

            // Join lines and return content
            let input = lines.join("\n");
            cmd.stdin(Stdio::piped());
            Ok(Some(input))
        }

        Redirect::Herestring { content } => {
            // Expand the content word
            let mut input = rush_expand::expand_word(content, context)
                .map_err(|e| RedirectError::ExpansionError(e.to_string()))?;

            // Herestrings add a trailing newline
            input.push('\n');
            cmd.stdin(Stdio::piped());
            Ok(Some(input))
        }
    }
}
