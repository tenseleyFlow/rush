//! Custom hinter that combines history and completion suggestions
//!
//! Unlike DefaultHinter which only uses history, this hinter also provides
//! ghost text suggestions from completions when there's a single match.

use crate::completer::RushCompleter;
use nu_ansi_term::{Color, Style};
use reedline::{Completer, Hinter, History};

/// A hinter that provides suggestions from both history and completions
pub struct RushHinter {
    style: Style,
    current_hint: String,
    min_chars: usize,
}

impl RushHinter {
    pub fn new() -> Self {
        Self {
            style: Style::new().fg(Color::DarkGray),
            current_hint: String::new(),
            min_chars: 1,
        }
    }

    /// Set the style for the hint text
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set minimum characters before showing hints
    pub fn with_min_chars(mut self, min_chars: usize) -> Self {
        self.min_chars = min_chars;
        self
    }

    /// Get hint from history (matching prefix)
    fn get_history_hint(&self, line: &str, history: &dyn History) -> Option<String> {
        if line.is_empty() {
            return None;
        }

        // Search history for commands starting with current line
        let search = history
            .search(reedline::SearchQuery::last_with_prefix(
                line.to_string(),
                None, // No session filter
            ))
            .ok()?;

        // Return the first match's suffix (part after current input)
        search.first().and_then(|entry| {
            let cmd = &entry.command_line;
            if cmd.starts_with(line) && cmd.len() > line.len() {
                Some(cmd[line.len()..].to_string())
            } else {
                None
            }
        })
    }

    /// Get hint from completions
    /// Shows hint for single match, or common prefix for multiple matches
    fn get_completion_hint(&self, line: &str, pos: usize) -> Option<String> {
        let mut completer = RushCompleter::new();
        let suggestions = completer.complete(line, pos);

        if suggestions.is_empty() {
            return None;
        }

        // Get the current word being completed
        let current_word_start = suggestions[0].span.start;
        let current_word = &line[current_word_start..pos];

        if suggestions.len() == 1 {
            // Single match - show full completion as hint
            let suggestion = &suggestions[0];
            if suggestion.value.starts_with(current_word) {
                let hint = &suggestion.value[current_word.len()..];
                if !hint.is_empty() {
                    return Some(hint.to_string());
                }
            }
        } else {
            // Multiple matches - find common prefix beyond current input
            let first = &suggestions[0].value;
            let common_prefix = suggestions.iter().skip(1).fold(first.clone(), |acc, s| {
                acc.chars()
                    .zip(s.value.chars())
                    .take_while(|(a, b)| a == b)
                    .map(|(a, _)| a)
                    .collect()
            });

            // Only show hint if common prefix extends beyond what user typed
            if common_prefix.len() > current_word.len() && common_prefix.starts_with(current_word) {
                let hint = &common_prefix[current_word.len()..];
                if !hint.is_empty() {
                    return Some(hint.to_string());
                }
            }
        }

        None
    }
}

impl Default for RushHinter {
    fn default() -> Self {
        Self::new()
    }
}

impl Hinter for RushHinter {
    fn handle(
        &mut self,
        line: &str,
        pos: usize,
        history: &dyn History,
        use_ansi_coloring: bool,
        _cwd: &str,
    ) -> String {
        self.current_hint.clear();

        // Don't show hints for very short input
        if line.len() < self.min_chars {
            return String::new();
        }

        // Only provide hints when cursor is at end of line
        if pos != line.len() {
            return String::new();
        }

        // First try history hints (higher priority)
        if let Some(hint) = self.get_history_hint(line, history) {
            self.current_hint = hint.clone();
            return if use_ansi_coloring {
                self.style.paint(&hint).to_string()
            } else {
                hint
            };
        }

        // Fall back to completion hints
        if let Some(hint) = self.get_completion_hint(line, pos) {
            self.current_hint = hint.clone();
            return if use_ansi_coloring {
                self.style.paint(&hint).to_string()
            } else {
                hint
            };
        }

        String::new()
    }

    fn complete_hint(&self) -> String {
        self.current_hint.clone()
    }

    fn next_hint_token(&self) -> String {
        // Return the first word/token of the hint for incremental completion
        let hint = &self.current_hint;

        // Find first whitespace or end of string
        if let Some(space_pos) = hint.find(char::is_whitespace) {
            hint[..space_pos].to_string()
        } else {
            hint.clone()
        }
    }
}
