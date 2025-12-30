use reedline::{DefaultPrompt, Reedline, Signal};
use rush_expand::Context;
use std::process::ExitCode;

pub fn run_interactive() -> ExitCode {
    let mut line_editor = Reedline::create();
    let prompt = DefaultPrompt::default();
    let mut context = Context::new();

    loop {
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
