# Rush Shell - Development Roadmap

## Vision

A modern, friendly, and fast shell with 100% bash compatibility and fish-like features.

## Development Approach

**Incremental**: Start with minimal working REPL, add features iteratively. Each phase produces a working shell with progressively more capabilities.

## Phases

### Phase 0: Project Setup ✅
**Status**: Completed
**Goal**: Establish workspace structure
- 7-crate workspace architecture
- All dependencies configured
- Directory structure established

### Phase 1: Minimal REPL 🔜
**Status**: Next
**Goal**: Interactive shell that executes basic commands
- Simple command parsing
- External command execution
- Basic REPL with reedline
- **Validation**: Can run `ls`, `pwd`, `echo hello world`

### Phase 2: Variables & Expansion
**Goal**: Shell variables and expansions
- Variable assignments and references
- Parameter expansion (`${VAR}`, `${VAR:-default}`)
- Command substitution `$(cmd)`
- Word splitting

### Phase 3: Pipelines & Redirection
**Goal**: Pipelines and I/O redirection
- Pipeline syntax: `cmd1 | cmd2 | cmd3`
- Redirections: `<`, `>`, `>>`, `2>`, `2>&1`, `&>`
- Pipe creation and management

### Phase 4: Control Flow
**Goal**: if/while/for/case statements
- Conditional evaluation (if/then/else/fi)
- Loop constructs (while/for)
- Case statement matching
- Built-in test command

### Phase 5: Job Control
**Goal**: Background jobs, fg/bg, signals
- Job tracking and process groups
- Terminal control
- Signal handling (SIGCHLD, SIGTSTP, SIGINT)
- Built-ins: `jobs`, `fg`, `bg`

### Phase 6: Fish Features
**Goal**: Syntax highlighting, suggestions, completions
- Real-time syntax highlighting
- History-based auto-suggestions
- Smart tab completion
- Better error messages

### Phase 7: Bash Compatibility Hardening
**Goal**: Close compatibility gaps
- Extended glob patterns
- Advanced parameter expansion
- Brace expansion, arithmetic expansion
- Here-documents, functions, arrays, subshells

## Success Criteria

- **Phase 1-3**: Daily command-line use (files, git, basic scripts)
- **Phase 4-5**: Run moderately complex bash scripts unmodified
- **Phase 6**: Better UX than bash for interactive use
- **Phase 7**: 90%+ compatibility with common bash patterns

## Key Features

### Bash Compatibility
- Command execution and pipelines
- Variables and expansions
- Control flow (if/while/for/case)
- Job control
- Full POSIX shell compliance

### Fish-Inspired Features
- Syntax highlighting
- Autosuggestions from history
- Better error messages
- Improved tab completion

## Architecture

7-crate modular design:
- **rush-cli**: Main binary and REPL orchestration
- **rush-parser**: Pest PEG parser for shell syntax
- **rush-expand**: Variable/glob expansion engine
- **rush-executor**: Command execution and pipelines
- **rush-job**: Job control and process management
- **rush-eval**: Control flow evaluation engine
- **rush-interactive**: Fish-like interactive features

## Technology Stack

- **Parser**: Pest (PEG)
- **REPL**: reedline
- **Terminal**: crossterm
- **Unix APIs**: nix
- **Glob**: globset
- **Error handling**: thiserror

## Current Status

**Phase 0**: ✅ Complete - Workspace established and compiling
**Phase 1**: 🔜 Ready to begin - Minimal REPL implementation next
