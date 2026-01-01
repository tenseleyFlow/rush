use reedline::{
    ColumnarMenu, DefaultPrompt, Emacs, FileBackedHistory,
    KeyCode, KeyModifiers, MenuBuilder, Reedline, ReedlineEvent, ReedlineMenu, Signal,
    default_emacs_keybindings,
};
use rush_interactive::{RushCompleter, RushHighlighter, RushHinter};
use std::process::ExitCode;

pub fn run_interactive() -> ExitCode {
    // Set up persistent history
    let history_file = dirs::data_dir()
        .map(|mut path| {
            path.push("rush");
            std::fs::create_dir_all(&path).ok();
            path.push("history.txt");
            path
        })
        .and_then(|path| {
            FileBackedHistory::with_file(1000, path).ok()
        });

    // Create the completion menu with custom marker (instead of default "|")
    let completion_menu = Box::new(
        ColumnarMenu::default()
            .with_name("completion_menu")
            .with_marker("› ")
    );

    // Set up keybindings with Tab completion and arrow key navigation
    let mut keybindings = default_emacs_keybindings();

    // Tab opens menu or cycles through completions
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );

    // Shift+Tab cycles backwards
    keybindings.add_binding(
        KeyModifiers::SHIFT,
        KeyCode::BackTab,
        ReedlineEvent::MenuPrevious,
    );

    // Arrow keys navigate the completion menu (fish-style)
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Down,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::MenuDown,
            ReedlineEvent::Down,
        ]),
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Up,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::MenuUp,
            ReedlineEvent::Up,
        ]),
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Left,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::MenuLeft,
            ReedlineEvent::Left,
        ]),
    );
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Right,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::MenuRight,
            ReedlineEvent::HistoryHintComplete,  // Accept hint if at end of line
            ReedlineEvent::Right,
        ]),
    );

    // Enter executes command
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Enter,
        ReedlineEvent::Submit,
    );

    let mut line_editor = Reedline::create()
        .with_highlighter(Box::new(RushHighlighter::new()))
        .with_hinter(Box::new(RushHinter::new()))
        .with_completer(Box::new(RushCompleter::new()))
        .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
        .with_edit_mode(Box::new(Emacs::new(keybindings)));

    // Add history if we successfully created it
    if let Some(history) = history_file {
        line_editor = line_editor.with_history(Box::new(history));
    }

    let prompt = DefaultPrompt::default();
    let mut context = crate::create_context();

    loop {
        // Check for completed/stopped background jobs before each prompt
        #[cfg(unix)]
        crate::check_background_jobs(&mut context);

        let sig = line_editor.read_line(&prompt);

        match sig {
            Ok(Signal::Success(buffer)) => {
                if buffer.trim().is_empty() {
                    continue;
                }

                if let Err(e) = crate::execute_interactive_line(&buffer, &mut context) {
                    eprintln!("rush: {}", e);
                }
            }
            Ok(Signal::CtrlD) | Ok(Signal::CtrlC) => {
                break;
            }
            Err(e) => {
                eprintln!("rush: error: {}", e);
                return ExitCode::from(1);
            }
        }
    }

    ExitCode::SUCCESS
}
