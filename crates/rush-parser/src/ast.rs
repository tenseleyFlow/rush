// Abstract Syntax Tree for Rush shell

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    /// Single complete command (simple, pipeline, control flow, etc.)
    Complete(CompleteCommand),
    /// Multiple complete commands (for scripts with multiple top-level commands)
    Script(Vec<CompleteCommand>),
    /// Empty line or comment
    Empty,
}

/// A complete command with optional background execution flag
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteCommand {
    /// The command to execute
    pub command: CommandType,
    /// Whether to run in background (trailing &)
    pub background: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandType {
    /// Simple command with optional variable assignments
    Simple(SimpleCommand),
    /// Pipeline of commands connected by pipes
    Pipeline(Pipeline),
    /// Commands connected by && or ||
    AndOrList(AndOrList),
    /// If statement
    If(IfStatement),
    /// While loop
    While(WhileStatement),
    /// For loop
    For(ForStatement),
    /// Select loop (menu selection)
    Select(SelectStatement),
    /// Case statement
    Case(CaseStatement),
    /// Function definition
    Function(FunctionDef),
    /// Subshell: (commands)
    Subshell(Subshell),
    /// Extended test: [[ expression ]]
    ExtendedTest(CondExpr),
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
    pub commands: Vec<PipelineElement>,
}

/// An element in a pipeline - can be a simple command, subshell, or extended test
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineElement {
    Simple(SimpleCommand),
    Subshell(Subshell),
    ExtendedTest(CondExpr),
}

/// Conditional expression for [[ ]] extended test
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CondExpr {
    /// Logical OR: expr1 || expr2
    Or(Box<CondExpr>, Box<CondExpr>),
    /// Logical AND: expr1 && expr2
    And(Box<CondExpr>, Box<CondExpr>),
    /// Logical NOT: ! expr
    Not(Box<CondExpr>),
    /// Unary test: -z, -n, -f, -d, etc.
    Unary { op: String, operand: Word },
    /// Binary test: ==, !=, =~, -eq, -lt, etc.
    Binary { left: Word, op: String, right: Word },
    /// Single word (true if non-empty after expansion)
    Word(Word),
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
    /// Here-document: <<DELIMITER or <<-DELIMITER
    Heredoc {
        delimiter: String,
        content: Vec<String>,
        strip_tabs: bool,
        expand: bool,
    },
    /// Here-string: <<<string
    Herestring { content: Word },
    /// Process substitution input: <(command)
    /// Creates a FIFO that reads from the command's stdout
    ProcessSubstInput { command: String },
    /// Process substitution output: >(command)
    /// Creates a FIFO that writes to the command's stdin
    ProcessSubstOutput { command: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndOrList {
    /// First pipeline in the list
    pub first: Pipeline,
    /// Remaining pipelines with their operators
    pub rest: Vec<(AndOrOp, Pipeline)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AndOrOp {
    /// && operator (execute next if previous succeeded)
    And,
    /// || operator (execute next if previous failed)
    Or,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfStatement {
    /// Condition to test
    pub condition: Box<CompleteCommand>,
    /// Commands to execute if condition is true
    pub then_body: Vec<CompleteCommand>,
    /// Elif clauses
    pub elif_clauses: Vec<ElifClause>,
    /// Else body (optional)
    pub else_body: Option<Vec<CompleteCommand>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElifClause {
    /// Condition to test
    pub condition: Box<CompleteCommand>,
    /// Commands to execute if condition is true
    pub then_body: Vec<CompleteCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhileStatement {
    /// Condition to test
    pub condition: Box<CompleteCommand>,
    /// Commands to execute while condition is true
    pub body: Vec<CompleteCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForStatement {
    /// Variable name to iterate over
    pub var_name: String,
    /// Words to iterate through
    pub words: Vec<Word>,
    /// Commands to execute for each word
    pub body: Vec<CompleteCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectStatement {
    /// Variable name to store selected item
    pub var_name: String,
    /// Words to present as menu options
    pub words: Vec<Word>,
    /// Commands to execute after selection
    pub body: Vec<CompleteCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseStatement {
    /// Word to match against patterns
    pub word: Word,
    /// Case clauses with patterns and bodies
    pub clauses: Vec<CaseClause>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseClause {
    /// Patterns to match (connected by |)
    pub patterns: Vec<Word>,
    /// Commands to execute if pattern matches
    pub body: Vec<CompleteCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDef {
    /// Function name
    pub name: String,
    /// Function body (commands to execute)
    pub body: Vec<CompleteCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subshell {
    /// Commands to execute in a subshell
    pub commands: Vec<CompleteCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    pub name: String,
    /// Optional array index for array[index]=value
    pub index: Option<String>,
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
    /// Brace expansion: {a,b,c} or {1..5}
    BraceExpansion(BraceExpansion),
    /// Arithmetic expansion: $((expr))
    ArithmeticExpansion(String),
    /// Array literal: (one two three)
    ArrayLiteral(Vec<Word>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BraceExpansion {
    /// List: {a,b,c}
    List(Vec<String>),
    /// Sequence: {1..10} or {a..z}
    Sequence {
        start: String,
        end: String,
        increment: Option<i32>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VarExpansion {
    /// Simple: $VAR
    Simple(String),
    /// Braced: ${VAR}
    Braced(String),
    /// With default: ${VAR:-default}
    WithDefault { name: String, default: Box<Word> },
    /// Length: ${#VAR}
    Length(String),
    /// Remove shortest prefix: ${VAR#pattern}
    RemoveShortestPrefix { name: String, pattern: String },
    /// Remove longest prefix: ${VAR##pattern}
    RemoveLongestPrefix { name: String, pattern: String },
    /// Remove shortest suffix: ${VAR%pattern}
    RemoveShortestSuffix { name: String, pattern: String },
    /// Remove longest suffix: ${VAR%%pattern}
    RemoveLongestSuffix { name: String, pattern: String },
    /// Replace first: ${VAR/pattern/replacement}
    ReplaceFirst { name: String, pattern: String, replacement: String },
    /// Replace all: ${VAR//pattern/replacement}
    ReplaceAll { name: String, pattern: String, replacement: String },
    /// Substring: ${VAR:offset} or ${VAR:offset:length}
    Substring { name: String, offset: i32, length: Option<usize> },
    /// Uppercase first: ${VAR^}
    UppercaseFirst(String),
    /// Uppercase all: ${VAR^^}
    UppercaseAll(String),
    /// Lowercase first: ${VAR,}
    LowercaseFirst(String),
    /// Lowercase all: ${VAR,,}
    LowercaseAll(String),
    /// Array element: ${arr[index]}
    ArrayElement { name: String, index: String },
    /// Array all elements (separate words): ${arr[@]}
    ArrayAll(String),
    /// Array all elements (single word): ${arr[*]}
    ArrayStar(String),
    /// Array length: ${#arr[@]} or ${#arr[*]}
    ArrayLength(String),
    /// Array indices: ${!arr[@]}
    ArrayIndices(String),
    /// Indirect expansion: ${!var}
    Indirect(String),
    /// Transformation: ${var@Q}, ${var@E}, etc.
    Transform { name: String, op: char },
}

impl Pipeline {
    pub fn new(commands: Vec<PipelineElement>) -> Self {
        Self { commands }
    }

    /// Helper to create a pipeline from simple commands
    pub fn from_simple_commands(commands: Vec<SimpleCommand>) -> Self {
        Self {
            commands: commands.into_iter().map(PipelineElement::Simple).collect(),
        }
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
        Self { name, index: None, value }
    }

    pub fn new_array(name: String, index: String, value: Word) -> Self {
        Self { name, index: Some(index), value }
    }
}

impl AndOrList {
    pub fn new(first: Pipeline, rest: Vec<(AndOrOp, Pipeline)>) -> Self {
        Self { first, rest }
    }
}

impl IfStatement {
    pub fn new(
        condition: Box<CompleteCommand>,
        then_body: Vec<CompleteCommand>,
        elif_clauses: Vec<ElifClause>,
        else_body: Option<Vec<CompleteCommand>>,
    ) -> Self {
        Self {
            condition,
            then_body,
            elif_clauses,
            else_body,
        }
    }
}

impl ElifClause {
    pub fn new(condition: Box<CompleteCommand>, then_body: Vec<CompleteCommand>) -> Self {
        Self {
            condition,
            then_body,
        }
    }
}

impl WhileStatement {
    pub fn new(condition: Box<CompleteCommand>, body: Vec<CompleteCommand>) -> Self {
        Self { condition, body }
    }
}

impl ForStatement {
    pub fn new(var_name: String, words: Vec<Word>, body: Vec<CompleteCommand>) -> Self {
        Self {
            var_name,
            words,
            body,
        }
    }
}

impl CaseStatement {
    pub fn new(word: Word, clauses: Vec<CaseClause>) -> Self {
        Self { word, clauses }
    }
}

impl CaseClause {
    pub fn new(patterns: Vec<Word>, body: Vec<CompleteCommand>) -> Self {
        Self { patterns, body }
    }
}

impl FunctionDef {
    pub fn new(name: String, body: Vec<CompleteCommand>) -> Self {
        Self { name, body }
    }
}

impl Subshell {
    pub fn new(commands: Vec<CompleteCommand>) -> Self {
        Self { commands }
    }
}

impl CompleteCommand {
    pub fn new(command: CommandType, background: bool) -> Self {
        Self { command, background }
    }

    pub fn foreground(command: CommandType) -> Self {
        Self {
            command,
            background: false,
        }
    }
}
