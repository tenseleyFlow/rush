// Abstract Syntax Tree for Rush shell

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    /// Simple command with optional variable assignments
    Simple(SimpleCommand),
    /// Empty line or comment
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleCommand {
    /// Variable assignments (VAR=value)
    pub assignments: Vec<Assignment>,
    /// Command and arguments (words that may contain expansions)
    pub words: Vec<Word>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    pub name: String,
    pub value: Word,
}

/// A word is composed of one or more parts that will be concatenated
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub parts: Vec<WordPart>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WordPart {
    /// Literal text
    Literal(String),
    /// Variable expansion: $VAR or ${VAR}
    VarExpansion(VarExpansion),
    /// Command substitution: $(cmd)
    CommandSubstitution(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VarExpansion {
    /// Simple: $VAR
    Simple(String),
    /// Braced: ${VAR}
    Braced(String),
    /// With default: ${VAR:-default}
    WithDefault { name: String, default: Box<Word> },
}

impl SimpleCommand {
    pub fn new(assignments: Vec<Assignment>, words: Vec<Word>) -> Self {
        Self { assignments, words }
    }

    pub fn has_command(&self) -> bool {
        !self.words.is_empty()
    }
}

impl Word {
    pub fn new(parts: Vec<WordPart>) -> Self {
        Self { parts }
    }

    pub fn from_literal(s: impl Into<String>) -> Self {
        Self {
            parts: vec![WordPart::Literal(s.into())],
        }
    }

    pub fn is_literal(&self) -> bool {
        self.parts.len() == 1 && matches!(self.parts[0], WordPart::Literal(_))
    }
}

impl Assignment {
    pub fn new(name: String, value: Word) -> Self {
        Self { name, value }
    }
}
