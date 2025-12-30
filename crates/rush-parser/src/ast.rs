// Abstract Syntax Tree for Rush shell

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    /// Simple command with optional variable assignments
    Simple(SimpleCommand),
    /// Pipeline of commands connected by pipes
    Pipeline(Pipeline),
    /// Empty line or comment
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleCommand {
    /// Variable assignments (VAR=value)
    pub assignments: Vec<Assignment>,
    /// Command and arguments (words that may contain expansions)
    pub words: Vec<Word>,
    /// I/O redirections
    pub redirects: Vec<Redirect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pipeline {
    /// Commands connected by pipes
    pub commands: Vec<SimpleCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Redirect {
    /// Input redirection: <file
    Input { file: Word },
    /// Output redirection: >file or N>file
    Output { fd: Option<u32>, file: Word },
    /// Append redirection: >>file or N>>file
    OutputAppend { fd: Option<u32>, file: Word },
    /// Stderr to stdout: 2>&1
    StderrToStdout,
    /// All output: &>file or &>>file
    AllOutput { file: Word, append: bool },
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

impl Pipeline {
    pub fn new(commands: Vec<SimpleCommand>) -> Self {
        Self { commands }
    }

    pub fn is_simple(&self) -> bool {
        self.commands.len() == 1
    }
}

impl SimpleCommand {
    pub fn new(assignments: Vec<Assignment>, words: Vec<Word>) -> Self {
        Self {
            assignments,
            words,
            redirects: Vec::new(),
        }
    }

    pub fn with_redirects(
        assignments: Vec<Assignment>,
        words: Vec<Word>,
        redirects: Vec<Redirect>,
    ) -> Self {
        Self {
            assignments,
            words,
            redirects,
        }
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
