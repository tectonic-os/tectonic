//! The only place ratatui appears. Widgets draw into a bounded region of the
//! normal scroll, never an alternate screen, and only where the caller has
//! already found a terminal to draw on.

pub mod tree;

use ratatui::backend::Backend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::crossterm::terminal;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};
use ratatui::{DefaultTerminal, Frame, TerminalOptions, Viewport};

/// One option, and what a person needs to see to pick between it and the rest.
pub struct Choice {
    pub label: String,
    pub detail: String,
}

impl Choice {
    pub fn new(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: detail.into(),
        }
    }
}

/// What a question taking several answers came back with. Leaving without
/// answering and answering with nothing are different, and only the asker
/// knows whether they mean the same thing.
pub enum Answer {
    Cancelled,
    Chosen(Vec<usize>),
}

/// How many options are on screen at once; the rest scroll under them.
const VISIBLE: usize = 8;

const PICK: &str = "up and down to move, enter to choose, esc for none";
const TOGGLE: &str = "up and down to move, space to toggle, enter to confirm, esc for none";
const EITHER: &str = "up and down to move, enter to answer";

/// One of `options`, or none.
pub fn select(question: &str, options: &[Choice]) -> Result<Option<usize>, String> {
    inline(options.len(), |terminal| {
        pick(terminal, question, options, PICK)
    })
}

/// Which of `yes` and `no`, drawn as the two answers they are.
pub fn confirm(question: &str, yes: &str, no: &str) -> Result<bool, String> {
    let options = [Choice::new(yes, ""), Choice::new(no, "")];
    let chosen = inline(options.len(), |terminal| {
        pick(terminal, question, &options, EITHER)
    })?;
    Ok(chosen == Some(0))
}

/// Any of `options`, in the order they were toggled on, or none.
pub fn multi(question: &str, options: &[Choice]) -> Result<Answer, String> {
    inline(options.len(), |terminal| {
        toggle(terminal, question, options)
    })
}

/// A bounded region of the normal scroll, cleared again before this returns, so
/// what the caller prints afterwards lands where the region was.
fn inline<T>(
    options: usize,
    body: impl FnOnce(&mut DefaultTerminal) -> Result<T, String>,
) -> Result<T, String> {
    let height = (options.min(VISIBLE) + 2) as u16;
    let mut terminal = ratatui::try_init_with_options(TerminalOptions {
        viewport: Viewport::Inline(height),
    })
    .map_err(|err| err.to_string())?;

    let out = body(&mut terminal);

    let origin = terminal.get_frame().area().as_position();
    let _ = terminal.clear();
    let _ = terminal.set_cursor_position(origin);
    let _ = terminal.show_cursor();
    // Not `ratatui::restore`, which also leaves an alternate screen never entered.
    let _ = terminal::disable_raw_mode();
    out
}

fn pick<B: Backend>(
    terminal: &mut ratatui::Terminal<B>,
    question: &str,
    options: &[Choice],
    hint: &str,
) -> Result<Option<usize>, String> {
    let mut state = ListState::default().with_selected(Some(0));
    loop {
        terminal
            .draw(|frame| draw(frame, question, options, None, hint, &mut state))
            .map_err(|err| err.to_string())?;
        let Some(key) = read()? else { continue };
        match key {
            KeyCode::Enter => return Ok(state.selected()),
            KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
            code => move_by(code, &mut state),
        }
    }
}

fn toggle<B: Backend>(
    terminal: &mut ratatui::Terminal<B>,
    question: &str,
    options: &[Choice],
) -> Result<Answer, String> {
    let mut state = ListState::default().with_selected(Some(0));
    let mut on: Vec<usize> = Vec::new();
    loop {
        terminal
            .draw(|frame| draw(frame, question, options, Some(&on), TOGGLE, &mut state))
            .map_err(|err| err.to_string())?;
        let Some(key) = read()? else { continue };
        match key {
            KeyCode::Char(' ') => {
                if let Some(at) = state.selected() {
                    match on.iter().position(|held| *held == at) {
                        Some(held) => {
                            on.remove(held);
                        }
                        None => on.push(at),
                    }
                }
            }
            KeyCode::Enter => return Ok(Answer::Chosen(on)),
            KeyCode::Esc | KeyCode::Char('q') => return Ok(Answer::Cancelled),
            code => move_by(code, &mut state),
        }
    }
}

/// The next key pressed, None for an event that is not one, and an error only
/// for the interrupt.
fn read() -> Result<Option<KeyCode>, String> {
    let Event::Key(key) = event::read().map_err(|err| err.to_string())? else {
        return Ok(None);
    };
    if key.kind != KeyEventKind::Press {
        return Ok(None);
    }
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Err("interrupted".to_string())
        }
        code => Ok(Some(code)),
    }
}

fn move_by(code: KeyCode, state: &mut ListState) {
    match code {
        KeyCode::Up | KeyCode::Char('k') => state.select_previous(),
        KeyCode::Down | KeyCode::Char('j') => state.select_next(),
        KeyCode::Home => state.select_first(),
        KeyCode::End => state.select_last(),
        _ => {}
    }
}

/// `on` is None for a question taking one answer, which marks nothing.
fn draw(
    frame: &mut Frame,
    question: &str,
    options: &[Choice],
    on: Option<&[usize]>,
    hint: &str,
    state: &mut ListState,
) {
    let [head, body, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    frame.render_widget(Line::from(question.bold().cyan()), head);
    frame.render_widget(Line::from(hint.dim()), foot);

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(at, choice)| {
            let mark = match on {
                None => "",
                Some(on) if on.contains(&at) => "[x] ",
                Some(_) => "[ ] ",
            };
            ListItem::new(Line::from(vec![
                Span::raw(mark),
                Span::raw(&choice.label),
                Span::raw("  "),
                Span::styled(&choice.detail, Style::new().dim()),
            ]))
        })
        .collect();
    frame.render_stateful_widget(List::new(items).highlight_symbol("> "), body, state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn drawn(options: &[Choice], on: Option<&[usize]>, hint: &str, at: usize) -> String {
        let mut state = ListState::default().with_selected(Some(at));
        let mut terminal = Terminal::new(TestBackend::new(60, 4)).unwrap();
        terminal
            .draw(|frame| draw(frame, "which module", options, on, hint, &mut state))
            .unwrap();
        terminal.backend().to_string()
    }

    #[test]
    fn every_label_is_drawn_with_its_detail_and_the_selection_marked() {
        let options = [
            Choice::new("gaming", "steam and the rest"),
            Choice::new("starship", "requires shell-config"),
        ];
        let drawn = drawn(&options, None, PICK, 1);
        assert!(drawn.contains("which module"), "{drawn}");
        assert!(drawn.contains("  gaming  steam and the rest"), "{drawn}");
        assert!(
            drawn.contains("> starship  requires shell-config"),
            "{drawn}"
        );
        assert!(drawn.contains(PICK), "{drawn}");
    }

    #[test]
    fn a_toggled_option_is_drawn_held_and_the_rest_are_not() {
        let options = [Choice::new("gaming", ""), Choice::new("starship", "")];
        let drawn = drawn(&options, Some(&[1]), TOGGLE, 0);
        assert!(drawn.contains("> [ ] gaming"), "{drawn}");
        assert!(drawn.contains("  [x] starship"), "{drawn}");
    }

    #[test]
    fn a_question_taking_one_answer_marks_nothing() {
        let options = [Choice::new("Yes", ""), Choice::new("No", "")];
        let drawn = drawn(&options, None, EITHER, 0);
        assert!(drawn.contains("> Yes"), "{drawn}");
        assert!(!drawn.contains('['), "{drawn}");
    }
}
