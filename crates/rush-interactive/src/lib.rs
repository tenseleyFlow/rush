// Rush interactive - Fish-like interactive features

pub mod completer;
pub mod completion_spec;
pub mod error_hints;
pub mod highlighter;
pub mod hinter;
pub mod prompt;

pub use completer::RushCompleter;
pub use completion_spec::{
    add_completion, remove_completions, with_registry, with_registry_mut,
    CompletionBuilder, CompletionRegistry, CompletionSource, CompletionSpec,
};
pub use error_hints::ErrorHints;
pub use highlighter::RushHighlighter;
pub use hinter::RushHinter;
pub use prompt::RushPrompt;
