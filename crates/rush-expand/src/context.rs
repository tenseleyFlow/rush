use std::collections::HashMap;
use std::env;

#[cfg(unix)]
use rush_job::JobList;

/// Execution context holding shell variables and state
#[derive(Debug)]
pub struct Context {
    /// Shell variables (local and environment)
    variables: HashMap<String, String>,
    /// Variables that should be exported to child processes
    exported: HashMap<String, String>,
    /// Exit status of last command
    pub last_exit_status: i32,
    /// Job list for job control (unix only)
    #[cfg(unix)]
    pub job_list: JobList,
}

impl Context {
    /// Create a new context with environment variables
    pub fn new() -> Self {
        let mut context = Self {
            variables: HashMap::new(),
            exported: HashMap::new(),
            last_exit_status: 0,
            #[cfg(unix)]
            job_list: JobList::new(nix::unistd::getpgrp()),
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
            last_exit_status: 0,
            #[cfg(unix)]
            job_list: JobList::new(nix::unistd::getpgrp()),
        }
    }

    /// Set a variable (local to this shell)
    pub fn set_var(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name = name.into();
        let value = value.into();
        self.variables.insert(name, value);
    }

    /// Get a variable value
    pub fn get_var(&self, name: &str) -> Option<&str> {
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
        ctx.set_var("FOO", "bar");
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
        ctx.set_var("FOO", "bar");
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
