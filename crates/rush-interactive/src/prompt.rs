//! Custom shell prompt with PS1 support
//!
//! Implements bash-compatible prompt escape sequences:
//! - `\u` - Username
//! - `\h` - Hostname (short)
//! - `\H` - Hostname (full)
//! - `\w` - Working directory (~ for home)
//! - `\W` - Basename of working directory
//! - `\$` - `#` if root, `$` otherwise
//! - `\n` - Newline
//! - `\r` - Carriage return
//! - `\t` - Current time 24-hour (HH:MM:SS)
//! - `\T` - Current time 12-hour (HH:MM:SS)
//! - `\@` - Current time 12-hour with AM/PM (HH:MM AM/PM)
//! - `\A` - Current time 24-hour (HH:MM)
//! - `\d` - Date (Day Mon Date)
//! - `\D{format}` - Custom strftime format (e.g., `\D{%Y-%m-%d}`)
//! - `\\` - Literal backslash

use reedline::Prompt;
use std::borrow::Cow;
use std::env;

/// Custom prompt that supports PS1-style escape sequences
pub struct RushPrompt {
    /// Cached username
    username: String,
    /// Cached hostname (short)
    hostname_short: String,
    /// Cached hostname (full)
    hostname_full: String,
    /// Cached home directory
    home_dir: Option<String>,
}

impl RushPrompt {
    pub fn new() -> Self {
        let username = env::var("USER")
            .or_else(|_| env::var("USERNAME"))
            .unwrap_or_else(|_| "user".to_string());

        let hostname_full = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "localhost".to_string());

        let hostname_short = hostname_full
            .split('.')
            .next()
            .unwrap_or("localhost")
            .to_string();

        let home_dir = dirs::home_dir().map(|p| p.to_string_lossy().to_string());

        Self {
            username,
            hostname_short,
            hostname_full,
            home_dir,
        }
    }

    /// Expand PS1-style escape sequences
    pub fn expand_ps1(&self, ps1: &str) -> String {
        let mut result = String::with_capacity(ps1.len() * 2);
        let mut chars = ps1.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\\' {
                if let Some(&next) = chars.peek() {
                    chars.next();
                    match next {
                        'u' => result.push_str(&self.username),
                        'h' => result.push_str(&self.hostname_short),
                        'H' => result.push_str(&self.hostname_full),
                        'w' => result.push_str(&self.get_working_dir(false)),
                        'W' => result.push_str(&self.get_working_dir(true)),
                        '$' => {
                            // # for root, $ otherwise
                            if self.is_root() {
                                result.push('#');
                            } else {
                                result.push('$');
                            }
                        }
                        'n' => result.push('\n'),
                        'r' => result.push('\r'),
                        't' => result.push_str(&self.get_time_24()),      // HH:MM:SS (24-hour)
                        'T' => result.push_str(&self.get_time_12()),      // HH:MM:SS (12-hour)
                        '@' => result.push_str(&self.get_time_12_ampm()), // HH:MM AM/PM
                        'A' => result.push_str(&self.get_time_24_short()), // HH:MM (24-hour)
                        'd' => result.push_str(&self.get_date()),
                        'D' => {
                            // Custom date format: \D{format}
                            if chars.peek() == Some(&'{') {
                                chars.next(); // consume '{'
                                let mut format = String::new();
                                while let Some(&c) = chars.peek() {
                                    chars.next();
                                    if c == '}' {
                                        break;
                                    }
                                    format.push(c);
                                }
                                result.push_str(&self.get_custom_datetime(&format));
                            } else {
                                // No format specified, use default
                                result.push_str(&self.get_date());
                            }
                        }
                        '\\' => result.push('\\'),
                        '[' => {} // Start of non-printing sequence (ignored for now)
                        ']' => {} // End of non-printing sequence (ignored for now)
                        _ => {
                            // Unknown escape, keep as-is
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

    /// Get the current working directory, optionally just the basename
    fn get_working_dir(&self, basename_only: bool) -> String {
        let cwd = env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "?".to_string());

        if basename_only {
            std::path::Path::new(&cwd)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| cwd.clone())
        } else {
            // Replace home directory with ~
            if let Some(home) = &self.home_dir {
                if cwd == *home {
                    "~".to_string()
                } else if cwd.starts_with(home) {
                    format!("~{}", &cwd[home.len()..])
                } else {
                    cwd
                }
            } else {
                cwd
            }
        }
    }

    /// Check if the current user is root
    fn is_root(&self) -> bool {
        #[cfg(unix)]
        {
            nix::unistd::getuid().is_root()
        }
        #[cfg(not(unix))]
        {
            false
        }
    }

    /// Get current time in HH:MM:SS 24-hour format (\t)
    fn get_time_24(&self) -> String {
        chrono::Local::now().format("%H:%M:%S").to_string()
    }

    /// Get current time in HH:MM:SS 12-hour format (\T)
    fn get_time_12(&self) -> String {
        chrono::Local::now().format("%I:%M:%S").to_string()
    }

    /// Get current time in HH:MM AM/PM format (\@)
    fn get_time_12_ampm(&self) -> String {
        chrono::Local::now().format("%I:%M %p").to_string()
    }

    /// Get current time in HH:MM 24-hour format (\A)
    fn get_time_24_short(&self) -> String {
        chrono::Local::now().format("%H:%M").to_string()
    }

    /// Get current date in "Day Mon Date" format (\d)
    fn get_date(&self) -> String {
        chrono::Local::now().format("%a %b %d").to_string()
    }

    /// Get custom datetime format (\D{format})
    fn get_custom_datetime(&self, format: &str) -> String {
        chrono::Local::now().format(format).to_string()
    }
}

impl Default for RushPrompt {
    fn default() -> Self {
        Self::new()
    }
}

impl Prompt for RushPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        // Check for PS1 environment variable
        if let Ok(ps1) = env::var("PS1") {
            Cow::Owned(self.expand_ps1(&ps1))
        } else {
            // Default prompt: path〉 (fish-style)
            Cow::Owned(format!("{}〉", self.get_working_dir(false)))
        }
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        // Check for PS1_RIGHT environment variable (custom extension)
        if let Ok(ps1_right) = env::var("PS1_RIGHT") {
            Cow::Owned(self.expand_ps1(&ps1_right))
        } else {
            // Default right prompt: date and time
            Cow::Owned(chrono::Local::now().format("%m/%d/%Y %I:%M:%S %p").to_string())
        }
    }

    fn render_prompt_indicator(&self, _edit_mode: reedline::PromptEditMode) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        Cow::Borrowed("> ")
    }

    fn render_prompt_history_search_indicator(
        &self,
        _history_search: reedline::PromptHistorySearch,
    ) -> Cow<'_, str> {
        Cow::Borrowed("(search) ")
    }

    fn get_prompt_color(&self) -> reedline::Color {
        reedline::Color::Reset
    }

    fn get_prompt_multiline_color(&self) -> nu_ansi_term::Color {
        nu_ansi_term::Color::LightGray
    }

    fn get_indicator_color(&self) -> reedline::Color {
        reedline::Color::Reset
    }

    fn get_prompt_right_color(&self) -> reedline::Color {
        reedline::Color::Reset
    }
}
