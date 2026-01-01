//! Command-specific completion specifications
//!
//! This module provides a fish-style completion system where users can define
//! custom completions for specific commands using the `complete` builtin.
//!
//! Example usage:
//! ```bash
//! # Add subcommand completions for git
//! complete -c git -a "add commit push pull fetch checkout branch"
//!
//! # Add dynamic branch completions for git checkout
//! complete -c git -n "__rush_seen_subcommand checkout" -a "(git branch --format='%(refname:short)')"
//!
//! # Add option completions
//! complete -c ls -s l -d "Long format"
//! complete -c ls -s a -l all -d "Show hidden files"
//!
//! # Disable file completion for a command
//! complete -c git -f
//! ```

use std::collections::HashMap;
use std::sync::RwLock;

/// Global registry of completion specifications
static COMPLETION_REGISTRY: RwLock<Option<CompletionRegistry>> = RwLock::new(None);

/// A single completion specification
#[derive(Debug, Clone)]
pub struct CompletionSpec {
    /// The command this completion applies to
    pub command: String,

    /// Condition that must be true for this completion to apply
    /// This is a shell expression that will be evaluated
    pub condition: Option<String>,

    /// The completions to provide
    pub source: CompletionSource,

    /// Description for these completions
    pub description: Option<String>,

    /// If true, don't also complete files
    pub no_files: bool,
}

/// Source of completion values
#[derive(Debug, Clone)]
pub enum CompletionSource {
    /// Static list of completion strings
    Static(Vec<String>),

    /// Dynamic command to run (output lines become completions)
    Dynamic(String),

    /// Short option (e.g., -v)
    ShortOption(char),

    /// Long option (e.g., --verbose)
    LongOption(String),

    /// Short and long option together (e.g., -v/--verbose)
    Option {
        short: Option<char>,
        long: Option<String>,
    },
}

/// Registry holding all completion specifications
#[derive(Debug, Default)]
pub struct CompletionRegistry {
    /// Completions indexed by command name
    specs: HashMap<String, Vec<CompletionSpec>>,

    /// Commands that should not have file completion
    no_file_commands: std::collections::HashSet<String>,
}

impl CompletionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a completion specification
    pub fn add(&mut self, spec: CompletionSpec) {
        if spec.no_files {
            self.no_file_commands.insert(spec.command.clone());
        }
        self.specs
            .entry(spec.command.clone())
            .or_default()
            .push(spec);
    }

    /// Remove all completions for a command
    pub fn remove(&mut self, command: &str) {
        self.specs.remove(command);
        self.no_file_commands.remove(command);
    }

    /// Get completions for a command
    pub fn get(&self, command: &str) -> Option<&Vec<CompletionSpec>> {
        self.specs.get(command)
    }

    /// Check if a command has file completion disabled
    pub fn has_no_files(&self, command: &str) -> bool {
        self.no_file_commands.contains(command)
    }

    /// Get all registered commands
    pub fn commands(&self) -> Vec<&String> {
        self.specs.keys().collect()
    }

    /// Get all specs (for listing)
    pub fn all_specs(&self) -> &HashMap<String, Vec<CompletionSpec>> {
        &self.specs
    }
}

/// Initialize the global completion registry
pub fn init_registry() {
    let mut registry = COMPLETION_REGISTRY.write().unwrap();
    if registry.is_none() {
        let mut reg = CompletionRegistry::new();
        // Add default completions
        add_default_completions(&mut reg);
        *registry = Some(reg);
    }
}

/// Get a reference to the global registry for reading
pub fn with_registry<F, R>(f: F) -> R
where
    F: FnOnce(&CompletionRegistry) -> R,
{
    init_registry();
    let registry = COMPLETION_REGISTRY.read().unwrap();
    f(registry.as_ref().unwrap())
}

/// Get a mutable reference to the global registry
pub fn with_registry_mut<F, R>(f: F) -> R
where
    F: FnOnce(&mut CompletionRegistry) -> R,
{
    init_registry();
    let mut registry = COMPLETION_REGISTRY.write().unwrap();
    f(registry.as_mut().unwrap())
}

/// Add a completion spec to the global registry
pub fn add_completion(spec: CompletionSpec) {
    with_registry_mut(|reg| reg.add(spec));
}

/// Remove completions for a command from the global registry
pub fn remove_completions(command: &str) {
    with_registry_mut(|reg| reg.remove(command));
}

/// Add default completions for common commands
fn add_default_completions(registry: &mut CompletionRegistry) {
    // Git completions
    add_git_completions(registry);

    // Cargo completions
    add_cargo_completions(registry);
}

fn add_git_completions(registry: &mut CompletionRegistry) {
    // Git subcommands
    let git_subcommands = vec![
        "add", "bisect", "branch", "checkout", "clone", "commit", "diff",
        "fetch", "grep", "init", "log", "merge", "mv", "pull", "push",
        "rebase", "reset", "restore", "rm", "show", "stash", "status",
        "switch", "tag", "worktree",
    ];

    registry.add(CompletionSpec {
        command: "git".to_string(),
        condition: None,
        source: CompletionSource::Static(git_subcommands.into_iter().map(String::from).collect()),
        description: Some("Git subcommand".to_string()),
        no_files: false,
    });

    // Git branch completion for checkout/switch
    registry.add(CompletionSpec {
        command: "git".to_string(),
        condition: Some("__rush_git_needs_branch".to_string()),
        source: CompletionSource::Dynamic("git branch --format='%(refname:short)' 2>/dev/null".to_string()),
        description: Some("Branch name".to_string()),
        no_files: true,
    });

    // Common git options
    registry.add(CompletionSpec {
        command: "git".to_string(),
        condition: None,
        source: CompletionSource::Option {
            short: Some('v'),
            long: Some("verbose".to_string()),
        },
        description: Some("Be verbose".to_string()),
        no_files: false,
    });

    registry.add(CompletionSpec {
        command: "git".to_string(),
        condition: None,
        source: CompletionSource::Option {
            short: Some('h'),
            long: Some("help".to_string()),
        },
        description: Some("Show help".to_string()),
        no_files: false,
    });
}

fn add_cargo_completions(registry: &mut CompletionRegistry) {
    // Cargo subcommands
    let cargo_subcommands = vec![
        "build", "check", "clean", "doc", "new", "init", "add", "remove",
        "run", "test", "bench", "update", "search", "publish", "install",
        "uninstall", "fmt", "clippy", "fix", "tree", "vendor",
    ];

    registry.add(CompletionSpec {
        command: "cargo".to_string(),
        condition: None,
        source: CompletionSource::Static(cargo_subcommands.into_iter().map(String::from).collect()),
        description: Some("Cargo subcommand".to_string()),
        no_files: false,
    });

    // Common cargo options
    registry.add(CompletionSpec {
        command: "cargo".to_string(),
        condition: None,
        source: CompletionSource::Option {
            short: None,
            long: Some("release".to_string()),
        },
        description: Some("Build in release mode".to_string()),
        no_files: false,
    });

    registry.add(CompletionSpec {
        command: "cargo".to_string(),
        condition: None,
        source: CompletionSource::Option {
            short: None,
            long: Some("all-features".to_string()),
        },
        description: Some("Enable all features".to_string()),
        no_files: false,
    });

    registry.add(CompletionSpec {
        command: "cargo".to_string(),
        condition: None,
        source: CompletionSource::Option {
            short: Some('p'),
            long: Some("package".to_string()),
        },
        description: Some("Package to build".to_string()),
        no_files: false,
    });

    registry.add(CompletionSpec {
        command: "cargo".to_string(),
        condition: None,
        source: CompletionSource::Option {
            short: None,
            long: Some("bin".to_string()),
        },
        description: Some("Binary to run".to_string()),
        no_files: false,
    });
}

/// Helper struct for building completion specs fluently
pub struct CompletionBuilder {
    spec: CompletionSpec,
}

impl CompletionBuilder {
    pub fn new(command: &str) -> Self {
        Self {
            spec: CompletionSpec {
                command: command.to_string(),
                condition: None,
                source: CompletionSource::Static(vec![]),
                description: None,
                no_files: false,
            },
        }
    }

    pub fn condition(mut self, cond: &str) -> Self {
        self.spec.condition = Some(cond.to_string());
        self
    }

    pub fn completions(mut self, completions: Vec<String>) -> Self {
        self.spec.source = CompletionSource::Static(completions);
        self
    }

    pub fn dynamic(mut self, command: &str) -> Self {
        self.spec.source = CompletionSource::Dynamic(command.to_string());
        self
    }

    pub fn short_option(mut self, opt: char) -> Self {
        self.spec.source = CompletionSource::ShortOption(opt);
        self
    }

    pub fn long_option(mut self, opt: &str) -> Self {
        self.spec.source = CompletionSource::LongOption(opt.to_string());
        self
    }

    pub fn option(mut self, short: Option<char>, long: Option<&str>) -> Self {
        self.spec.source = CompletionSource::Option {
            short,
            long: long.map(String::from),
        };
        self
    }

    pub fn description(mut self, desc: &str) -> Self {
        self.spec.description = Some(desc.to_string());
        self
    }

    pub fn no_files(mut self) -> Self {
        self.spec.no_files = true;
        self
    }

    pub fn build(self) -> CompletionSpec {
        self.spec
    }

    pub fn register(self) {
        add_completion(self.build());
    }
}
