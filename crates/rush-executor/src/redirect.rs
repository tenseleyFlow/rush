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

        Redirect::ProcessSubstInput { command } => {
            // Process substitution: <(command)
            // Creates a FIFO that provides the command's stdout
            #[cfg(unix)]
            {
                let fifo_path = crate::process_subst::execute_process_subst_input(command, context)
                    .map_err(|e| RedirectError::FileOpenError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Process substitution failed: {}", e),
                    )))?;

                // Open the FIFO for reading (this will block until the writer opens it)
                let file_handle = File::open(&fifo_path)?;
                cmd.stdin(file_handle);
                Ok(None)
            }
            #[cfg(not(unix))]
            {
                Err(RedirectError::FileOpenError(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "Process substitution not supported on this platform".to_string(),
                )))
            }
        }

        Redirect::ProcessSubstOutput { command } => {
            // Process substitution: >(command)
            // Creates a FIFO that feeds into the command's stdin
            #[cfg(unix)]
            {
                let fifo_path = crate::process_subst::execute_process_subst_output(command, context)
                    .map_err(|e| RedirectError::FileOpenError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Process substitution failed: {}", e),
                    )))?;

                // Open the FIFO for writing (this will block until the reader opens it)
                let file_handle = OpenOptions::new()
                    .write(true)
                    .open(&fifo_path)?;
                cmd.stdout(file_handle);
                Ok(None)
            }
            #[cfg(not(unix))]
            {
                Err(RedirectError::FileOpenError(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "Process substitution not supported on this platform".to_string(),
                )))
            }
        }
    }
}

/// Saved file descriptors for restoring after builtin execution
#[cfg(unix)]
pub struct SavedFds {
    saved_stdout: Option<i32>,
    saved_stderr: Option<i32>,
}

#[cfg(unix)]
impl SavedFds {
    fn new() -> Self {
        Self {
            saved_stdout: None,
            saved_stderr: None,
        }
    }
}

#[cfg(unix)]
impl Drop for SavedFds {
    fn drop(&mut self) {
        use nix::libc;
        // Restore stdout if saved
        if let Some(saved) = self.saved_stdout {
            unsafe {
                libc::dup2(saved, libc::STDOUT_FILENO);
                libc::close(saved);
            }
        }
        // Restore stderr if saved
        if let Some(saved) = self.saved_stderr {
            unsafe {
                libc::dup2(saved, libc::STDERR_FILENO);
                libc::close(saved);
            }
        }
    }
}

/// Apply redirections to the current process (for builtins)
/// Returns a guard that restores the original file descriptors when dropped
#[cfg(unix)]
pub fn apply_redirects_to_process(
    redirects: &[Redirect],
    context: &mut Context,
) -> Result<SavedFds, RedirectError> {
    use nix::libc;
    use std::os::unix::io::IntoRawFd;

    let mut saved = SavedFds::new();

    for redirect in redirects {
        match redirect {
            Redirect::Output { fd, file } => {
                let filename = rush_expand::expand_word(file, context)
                    .map_err(|e| RedirectError::ExpansionError(e.to_string()))?;

                let file_handle = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&filename)?;

                let file_fd = file_handle.into_raw_fd();

                match fd {
                    None | Some(1) => {
                        // Save stdout if not already saved
                        if saved.saved_stdout.is_none() {
                            saved.saved_stdout = Some(unsafe { libc::dup(libc::STDOUT_FILENO) });
                        }
                        unsafe { libc::dup2(file_fd, libc::STDOUT_FILENO); }
                        unsafe { libc::close(file_fd); }
                    }
                    Some(2) => {
                        // Save stderr if not already saved
                        if saved.saved_stderr.is_none() {
                            saved.saved_stderr = Some(unsafe { libc::dup(libc::STDERR_FILENO) });
                        }
                        unsafe { libc::dup2(file_fd, libc::STDERR_FILENO); }
                        unsafe { libc::close(file_fd); }
                    }
                    Some(fd_num) => {
                        unsafe { libc::close(file_fd); }
                        return Err(RedirectError::InvalidFileDescriptor(*fd_num));
                    }
                }
            }
            Redirect::OutputAppend { fd, file } => {
                let filename = rush_expand::expand_word(file, context)
                    .map_err(|e| RedirectError::ExpansionError(e.to_string()))?;

                let file_handle = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .append(true)
                    .open(&filename)?;

                let file_fd = file_handle.into_raw_fd();

                match fd {
                    None | Some(1) => {
                        if saved.saved_stdout.is_none() {
                            saved.saved_stdout = Some(unsafe { libc::dup(libc::STDOUT_FILENO) });
                        }
                        unsafe { libc::dup2(file_fd, libc::STDOUT_FILENO); }
                        unsafe { libc::close(file_fd); }
                    }
                    Some(2) => {
                        if saved.saved_stderr.is_none() {
                            saved.saved_stderr = Some(unsafe { libc::dup(libc::STDERR_FILENO) });
                        }
                        unsafe { libc::dup2(file_fd, libc::STDERR_FILENO); }
                        unsafe { libc::close(file_fd); }
                    }
                    Some(fd_num) => {
                        unsafe { libc::close(file_fd); }
                        return Err(RedirectError::InvalidFileDescriptor(*fd_num));
                    }
                }
            }
            Redirect::StderrToStdout => {
                // 2>&1 - redirect stderr to stdout
                if saved.saved_stderr.is_none() {
                    saved.saved_stderr = Some(unsafe { libc::dup(libc::STDERR_FILENO) });
                }
                unsafe { libc::dup2(libc::STDOUT_FILENO, libc::STDERR_FILENO); }
            }
            Redirect::AllOutput { file, append } => {
                let filename = rush_expand::expand_word(file, context)
                    .map_err(|e| RedirectError::ExpansionError(e.to_string()))?;

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

                let file_fd = file_handle.into_raw_fd();

                if saved.saved_stdout.is_none() {
                    saved.saved_stdout = Some(unsafe { libc::dup(libc::STDOUT_FILENO) });
                }
                if saved.saved_stderr.is_none() {
                    saved.saved_stderr = Some(unsafe { libc::dup(libc::STDERR_FILENO) });
                }
                unsafe {
                    libc::dup2(file_fd, libc::STDOUT_FILENO);
                    libc::dup2(file_fd, libc::STDERR_FILENO);
                    libc::close(file_fd);
                }
            }
            // Input redirects, heredocs, etc. are not commonly used with builtins
            // They're handled specially or not applicable
            _ => {}
        }
    }

    Ok(saved)
}

/// Stub for non-Unix platforms
#[cfg(not(unix))]
pub struct SavedFds;

#[cfg(not(unix))]
pub fn apply_redirects_to_process(
    _redirects: &[Redirect],
    _context: &mut Context,
) -> Result<SavedFds, RedirectError> {
    Ok(SavedFds)
}
