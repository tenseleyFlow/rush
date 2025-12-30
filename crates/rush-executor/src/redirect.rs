use rush_expand::Context;
use rush_parser::ast::Redirect;
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
pub fn apply_redirects(
    cmd: &mut Command,
    redirects: &[Redirect],
    context: &Context,
) -> Result<(), RedirectError> {
    for redirect in redirects {
        apply_single_redirect(cmd, redirect, context)?;
    }
    Ok(())
}

fn apply_single_redirect(
    cmd: &mut Command,
    redirect: &Redirect,
    context: &Context,
) -> Result<(), RedirectError> {
    match redirect {
        Redirect::Input { file } => {
            // Expand the filename
            let filename = rush_expand::expand_word(file, context)
                .map_err(|e| RedirectError::ExpansionError(e.to_string()))?;

            // Open file for reading
            let file_handle = File::open(&filename)?;
            cmd.stdin(file_handle);
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
        }

        Redirect::StderrToStdout => {
            // Redirect stderr to stdout
            // Note: This is a simplified version. Proper implementation requires
            // using unsafe dup2 system calls to redirect fd 2 to fd 1
            cmd.stderr(Stdio::inherit());
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
        }
    }

    Ok(())
}
