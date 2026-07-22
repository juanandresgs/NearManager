use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crossterm::{
    cursor::MoveTo,
    execute,
    terminal::{Clear, ClearType},
};
use near_terminal::{
    Key, KeyKind, TerminalDiagnostics, TerminalEvent, TerminalEventReactor, TerminalRuntimeEvent,
    TerminalSession,
};
use serde::Serialize;

#[derive(Serialize)]
struct RecordedEvent {
    elapsed_millis: u128,
    event: TerminalEvent,
}

#[derive(Serialize)]
struct ProbeReport<'a> {
    schema_version: u32,
    complete: bool,
    termination_signal: Option<i32>,
    label: &'a str,
    term: Option<String>,
    term_program: Option<String>,
    tmux: bool,
    diagnostics: &'a TerminalDiagnostics,
    events: &'a [RecordedEvent],
}

struct ReportContext<'a> {
    output: &'a Path,
    label: &'a str,
    diagnostics: &'a TerminalDiagnostics,
    events: &'a [RecordedEvent],
}

fn arguments() -> Result<(PathBuf, String), String> {
    let mut arguments = env::args_os().skip(1);
    let output = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "usage: near-input-probe <output.json> [terminal-label]".to_owned())?;
    let label = arguments.next().map_or_else(
        || "unspecified-terminal".to_owned(),
        |value| value.to_string_lossy().into_owned(),
    );
    if arguments.next().is_some() {
        return Err("usage: near-input-probe <output.json> [terminal-label]".to_owned());
    }
    Ok((output, label))
}

fn render(event_count: usize, event: Option<&TerminalEvent>) -> io::Result<()> {
    let mut output = io::stdout();
    execute!(output, MoveTo(0, 0), Clear(ClearType::All))?;
    writeln!(output, "Near terminal input probe")?;
    writeln!(output)?;
    writeln!(
        output,
        "Exercise keypad, modified arrows, modifier holds/releases, focus, paste, and resize."
    )?;
    writeln!(
        output,
        "Press Ctrl+Q to save and exit. Events captured: {event_count}"
    )?;
    if let Some(event) = event {
        writeln!(output)?;
        writeln!(output, "Last normalized event: {event:?}")?;
    }
    output.flush()
}

fn should_exit(event: &TerminalEvent) -> bool {
    matches!(
        event,
        TerminalEvent::Key(stroke)
            if stroke.kind == KeyKind::Press
                && stroke.modifiers.control
                && stroke.key == Key::Character('q')
    )
}

fn write_report(
    context: &ReportContext<'_>,
    complete: bool,
    termination_signal: Option<i32>,
) -> Result<(), Box<dyn std::error::Error>> {
    let report = ProbeReport {
        schema_version: 1,
        complete,
        termination_signal,
        label: context.label,
        term: env::var("TERM").ok(),
        term_program: env::var("TERM_PROGRAM").ok(),
        tmux: env::var_os("TMUX").is_some(),
        diagnostics: context.diagnostics,
        events: context.events,
    };
    if let Some(parent) = context.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(context.output, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}

fn run(output: &Path, label: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut session = TerminalSession::enter()?;
    let mut reactor = TerminalEventReactor::new()?;
    let diagnostics = session.diagnostics().clone();
    let started = Instant::now();
    let mut events = Vec::new();
    write_report(
        &ReportContext {
            output,
            label,
            diagnostics: &diagnostics,
            events: &events,
        },
        false,
        None,
    )?;
    render(0, None)?;
    let (complete, termination_signal) = loop {
        match reactor.wait(Some(Duration::from_millis(50)))? {
            TerminalRuntimeEvent::Terminal(event) => {
                let exit = should_exit(&event);
                events.push(RecordedEvent {
                    elapsed_millis: started.elapsed().as_millis(),
                    event,
                });
                write_report(
                    &ReportContext {
                        output,
                        label,
                        diagnostics: &diagnostics,
                        events: &events,
                    },
                    false,
                    None,
                )?;
                render(events.len(), events.last().map(|record| &record.event))?;
                if exit {
                    break (true, None);
                }
            }
            TerminalRuntimeEvent::Terminate(signal) => break (false, Some(signal)),
            TerminalRuntimeEvent::Wake | TerminalRuntimeEvent::Timeout => {}
        }
    };
    let restore = session.restore();
    write_report(
        &ReportContext {
            output,
            label,
            diagnostics: &diagnostics,
            events: &events,
        },
        complete,
        termination_signal,
    )?;
    restore.map_err(Into::into)
}

fn main() {
    let result = match arguments() {
        Ok((output, label)) => run(&output, &label).map_err(|error| error.to_string()),
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        eprintln!("near-input-probe: {error}");
        std::process::exit(2);
    }
}
