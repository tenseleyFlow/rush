// Rush expand - Variable and glob expansion engine

pub mod arithmetic;
pub mod brace;
pub mod brace_parse;
pub mod command_subst;
pub mod context;
pub mod expand;
pub mod glob;

pub use arithmetic::{evaluate_arithmetic, ArithmeticError};
pub use brace::expand_brace;
pub use brace_parse::detect_brace_patterns;
pub use command_subst::{execute_command_substitution, CommandSubstError};
pub use context::Context;
pub use expand::{expand_word, expand_word_with_braces, expand_words, ExpansionError};
pub use glob::{expand_glob, GlobOptions};
