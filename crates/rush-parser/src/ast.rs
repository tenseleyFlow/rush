// Abstract Syntax Tree for Rush shell

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    /// Simple command: command name and arguments
    Simple(SimpleCommand),
    /// Empty line or comment
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleCommand {
    /// Command and arguments (first element is the command)
    pub args: Vec<String>,
}

impl SimpleCommand {
    pub fn new(args: Vec<String>) -> Self {
        Self { args }
    }

    pub fn command(&self) -> Option<&str> {
        self.args.first().map(|s| s.as_str())
    }

    pub fn arguments(&self) -> &[String] {
        if self.args.is_empty() {
            &[]
        } else {
            &self.args[1..]
        }
    }
}
