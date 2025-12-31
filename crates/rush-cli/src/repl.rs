use reedline::{DefaultHinter, DefaultPrompt, FileBackedHistory, Reedline, Signal};
use rush_expand::Context;
use rush_interactive::{RushCompleter, RushHighlighter};
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

    let mut line_editor = Reedline::create()
        .with_highlighter(Box::new(RushHighlighter::new()))
        .with_hinter(Box::new(
            DefaultHinter::default()
                .with_style(nu_ansi_term::Style::new().fg(nu_ansi_term::Color::DarkGray))
        ))
        .with_completer(Box::new(RushCompleter::new()));

    // Add history if we successfully created it
    if let Some(history) = history_file {
        line_editor = line_editor.with_history(Box::new(history));
    }

    let prompt = DefaultPrompt::default();
    let mut context = Context::new();

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
