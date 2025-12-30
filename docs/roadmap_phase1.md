# Phase 1: Minimal Shell (Interactive + Non-Interactive)

**Status**: 🚧 In Progress
**Goal**: Full-featured shell that executes basic commands in both interactive and non-interactive modes

## Tasks

### rush-cli
- [ ] Command-line argument parsing (clap)
  - [ ] Interactive mode (default when terminal)
  - [ ] `-c "command"` flag for command strings
  - [ ] Script file execution
  - [ ] Stdin detection (isatty)
- [ ] Interactive mode with reedline
  - [ ] Basic REPL loop (read → parse → execute → print)
  - [ ] Prompt rendering
  - [ ] Ctrl+C and Ctrl+D handling
- [ ] Non-interactive mode
  - [ ] Read from file
  - [ ] Read from stdin
  - [ ] Execute and exit with proper status code

### rush-parser
- [ ] Create minimal pest grammar (simple commands only)
- [ ] Define AST types: `SimpleCommand { args: Vec<String> }`
- [ ] Implement parser that handles: `ls`, `echo hello`, `cat file.txt`
- [ ] Handle empty lines and comments (#)

### rush-executor
- [ ] Execute external commands via `std::process::Command`
- [ ] PATH resolution
- [ ] Return exit codes
- [ ] Proper stdout/stderr handling
- [ ] Environment variable inheritance

## Validation Criteria

### Interactive Mode
Must be able to run interactively:
- `ls`
- `pwd`
- `echo hello world`
- Exit with Ctrl+D or `exit` command

### Non-Interactive Mode
Must work with:
- `rush -c "ls"`
- `echo "pwd" | rush`
- `rush script.sh` (where script.sh contains commands)
- Proper exit codes (0 on success, non-zero on failure)

## Files to Create

1. `crates/rush-parser/src/grammar.pest` - Pest PEG grammar
2. `crates/rush-parser/src/ast.rs` - AST type definitions
3. `crates/rush-parser/src/parser.rs` - Parser implementation
4. `crates/rush-cli/src/repl.rs` - REPL loop
5. `crates/rush-executor/src/command.rs` - Command execution

## Next Steps

After Phase 1 completion, proceed to Phase 2: Variables & Expansion
