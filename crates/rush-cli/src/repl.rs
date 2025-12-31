use reedline::{DefaultPrompt, Reedline, Signal};
use rush_expand::Context;
use rush_interactive::RushHighlighter;
use std::process::ExitCode;

pub fn run_interactive() -> ExitCode {
    let mut line_editor = Reedline::create()
        .with_highlighter(Box::new(RushHighlighter::new()));
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
