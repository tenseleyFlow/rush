use std::collections::{HashMap, HashSet};
use std::env;
use std::rc::Rc;

#[cfg(unix)]
use rush_job::JobList;

/// Callback type for executing command substitution internally
/// Takes the command string and returns the stdout output
pub type CommandExecutor = Rc<dyn Fn(&str, &mut Context) -> Result<String, String>>;

/// Wrapper for CommandExecutor that implements Debug
pub struct CommandExecutorWrapper(pub Option<CommandExecutor>);

impl std::fmt::Debug for CommandExecutorWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(_) => write!(f, "CommandExecutor(Some)"),
            None => write!(f, "CommandExecutor(None)"),
        }
    }
}

/// Shell options that can be set with the 'set' builtin
#[derive(Debug, Clone)]
pub struct ShellOptions {
    /// Exit immediately if a command exits with a non-zero status (set -e)
    pub errexit: bool,
    /// Print commands before executing them (set -x)
    pub xtrace: bool,
    /// Treat unset variables as an error (set -u)
    pub nounset: bool,
    /// Pipeline fails if any command fails (not just the last) (set -o pipefail)
    pub pipefail: bool,
    /// Disable filename expansion (globbing) (set -f)
    pub noglob: bool,
    /// Globs that match nothing expand to nothing (shopt -s nullglob)
    pub nullglob: bool,
    /// Include dotfiles in glob expansion (shopt -s dotglob)
    pub dotglob: bool,
    /// Extended glob patterns (shopt -s extglob)
    pub extglob: bool,
}

impl Default for ShellOptions {
    fn default() -> Self {
        Self {
            errexit: false,
            xtrace: false,
            nounset: false,
            pipefail: false,
            noglob: false,
            nullglob: false,
            dotglob: false,
            extglob: false,
        }
    }
}

/// Execution context holding shell variables and state
#[derive(Debug)]
pub struct Context {
    /// Shell variables (local and environment)
    variables: HashMap<String, String>,
    /// Variables that should be exported to child processes
    exported: HashMap<String, String>,
    /// Local variable scopes (stack of scopes, innermost first)
    /// Each scope contains variables declared with 'local' in that function
    local_scopes: Vec<HashMap<String, String>>,
    /// Exit status of last command
    pub last_exit_status: i32,
    /// Shell functions (name -> body)
    pub functions: HashMap<String, rush_parser::ast::FunctionDef>,
    /// Arrays (name -> values)
    pub arrays: HashMap<String, Vec<String>>,
    /// Command aliases (name -> expansion)
    pub aliases: HashMap<String, String>,
    /// Signal traps (signal name/number -> command)
    pub traps: HashMap<String, String>,
    /// Shell options (set -e, set -x, etc.)
    pub options: ShellOptions,
    /// Read-only variables
    readonly_vars: HashSet<String>,
    /// Positional parameters ($1, $2, $3, ...)
    pub positional_params: Vec<String>,
    /// Command hash table (command name -> full path)
    pub command_hash: HashMap<String, String>,
    /// OPTIND for getopts (current option index)
    pub optind: usize,
    /// Job list for job control (unix only)
    #[cfg(unix)]
    pub job_list: JobList,
    /// Internal command executor (for command substitution)
    /// If set, command substitution will use this instead of sh -c
    pub command_executor: CommandExecutorWrapper,
}

impl Context {
    /// Create a new context with environment variables
    pub fn new() -> Self {
        let mut context = Self {
            variables: HashMap::new(),
            exported: HashMap::new(),
            local_scopes: Vec::new(),
            last_exit_status: 0,
            functions: HashMap::new(),
            arrays: HashMap::new(),
            aliases: HashMap::new(),
            traps: HashMap::new(),
            options: ShellOptions::default(),
            readonly_vars: HashSet::new(),
            positional_params: Vec::new(),
            command_hash: HashMap::new(),
            optind: 1,
            #[cfg(unix)]
            job_list: JobList::new(nix::unistd::getpgrp()),
            command_executor: CommandExecutorWrapper(None),
        };

        // Initialize with environment variables
        for (key, value) in env::vars() {
            context.variables.insert(key.clone(), value.clone());
            context.exported.insert(key, value);
        }

        context
    }

    /// Create a new empty context (for testing)
    pub fn empty() -> Self {
        Self {
            variables: HashMap::new(),
            exported: HashMap::new(),
            local_scopes: Vec::new(),
            last_exit_status: 0,
            functions: HashMap::new(),
            arrays: HashMap::new(),
            aliases: HashMap::new(),
            traps: HashMap::new(),
            options: ShellOptions::default(),
            readonly_vars: HashSet::new(),
            positional_params: Vec::new(),
            command_hash: HashMap::new(),
            optind: 1,
            #[cfg(unix)]
            job_list: JobList::new(nix::unistd::getpgrp()),
            command_executor: CommandExecutorWrapper(None),
        }
    }

    /// Set a variable (local to this shell)
    /// Returns Ok(()) on success, Err with variable name if readonly
    pub fn set_var(&mut self, name: impl Into<String>, value: impl Into<String>) -> Result<(), String> {
        let name = name.into();

        // Check if readonly
        if self.is_readonly(&name) {
            return Err(name);
        }

        let value = value.into();
        self.variables.insert(name, value);
        Ok(())
    }

    /// Get a variable value
    /// Searches local scopes first (innermost to outermost), then global variables
    pub fn get_var(&self, name: &str) -> Option<&str> {
        // Search local scopes from innermost (most recent function) to outermost
        for scope in self.local_scopes.iter().rev() {
            if let Some(value) = scope.get(name) {
                return Some(value.as_str());
            }
        }

        // Not found in local scopes, check global variables
        self.variables.get(name).map(|s| s.as_str())
    }

    /// Export a variable (make it available to child processes)
    pub fn export_var(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name = name.into();
        let value = value.into();
        self.variables.insert(name.clone(), value.clone());
        self.exported.insert(name, value);
    }

    /// Check if a variable is exported
    pub fn is_exported(&self, name: &str) -> bool {
        self.exported.contains_key(name)
    }

    /// Get all exported variables (for passing to child processes)
    pub fn exported_vars(&self) -> &HashMap<String, String> {
        &self.exported
    }

    /// Get all variables (including non-exported)
    pub fn all_vars(&self) -> &HashMap<String, String> {
        &self.variables
    }

    /// Remove a variable
    pub fn unset_var(&mut self, name: &str) {
        self.variables.remove(name);
        self.exported.remove(name);
    }

    /// Unexport a variable (remove from exported but keep in variables)
    pub fn unexport_var(&mut self, name: &str) {
        self.exported.remove(name);
    }

    /// Mark a variable as readonly
    pub fn mark_readonly(&mut self, name: impl Into<String>) {
        self.readonly_vars.insert(name.into());
    }

    /// Check if a variable is readonly
    pub fn is_readonly(&self, name: &str) -> bool {
        self.readonly_vars.contains(name)
    }

    /// Get all readonly variables
    pub fn readonly_vars(&self) -> &HashSet<String> {
        &self.readonly_vars
    }

    /// Push a new local scope (when entering a function)
    pub fn push_scope(&mut self) {
        self.local_scopes.push(HashMap::new());
    }

    /// Pop the current local scope (when exiting a function)
    /// Returns the popped scope
    pub fn pop_scope(&mut self) -> Option<HashMap<String, String>> {
        self.local_scopes.pop()
    }

    /// Set a local variable in the current function scope
    /// If not in a function (no scopes), sets as global variable
    /// Returns Ok(()) on success, Err with variable name if readonly
    pub fn set_local_var(&mut self, name: impl Into<String>, value: impl Into<String>) -> Result<(), String> {
        let name = name.into();

        // Check if readonly
        if self.is_readonly(&name) {
            return Err(name);
        }

        let value = value.into();

        // If we're in a function (have local scopes), set in current scope
        if let Some(current_scope) = self.local_scopes.last_mut() {
            current_scope.insert(name, value);
        } else {
            // Not in a function, set as global
            self.variables.insert(name, value);
        }

        Ok(())
    }

    /// Update the exit status
    pub fn set_exit_status(&mut self, status: i32) {
        self.last_exit_status = status;
    }

    /// Get the exit status
    pub fn exit_status(&self) -> i32 {
        self.last_exit_status
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_get_var() {
        let mut ctx = Context::empty();
        ctx.set_var("FOO", "bar").unwrap();
        assert_eq!(ctx.get_var("FOO"), Some("bar"));
    }

    #[test]
    fn test_export_var() {
        let mut ctx = Context::empty();
        ctx.export_var("PATH", "/usr/bin");
        assert_eq!(ctx.get_var("PATH"), Some("/usr/bin"));
        assert!(ctx.is_exported("PATH"));
        assert_eq!(ctx.exported_vars().get("PATH"), Some(&"/usr/bin".to_string()));
    }

    #[test]
    fn test_unset_var() {
        let mut ctx = Context::empty();
        ctx.set_var("FOO", "bar").unwrap();
        assert_eq!(ctx.get_var("FOO"), Some("bar"));
        ctx.unset_var("FOO");
        assert_eq!(ctx.get_var("FOO"), None);
    }

    #[test]
    fn test_exit_status() {
        let mut ctx = Context::empty();
        assert_eq!(ctx.exit_status(), 0);
        ctx.set_exit_status(42);
        assert_eq!(ctx.exit_status(), 42);
    }
}
