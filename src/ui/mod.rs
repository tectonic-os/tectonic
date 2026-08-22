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
    /// The row this one is nested under, which comes before it. A parent and
    /// any of its children contradict; two children of one parent do not.
    pub parent: Option<usize>,
}

impl Choice {
    pub fn new(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: detail.into(),
            parent: None,
        }
    }

    pub fn under(mut self, parent: usize) -> Self {
        self.parent = Some(parent);
        self
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

const PICK: &str = "up and down to move, enter to choose, esc cancels";
const TOGGLE: &str =
    "up and down to move, enter or space to toggle, enter on done to confirm, esc cancels";
const EITHER: &str = "up and down to move, enter to answer";

/// The row past the last option, and the only way to submit.
const DONE: &str = "done";

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

/// Any of `options`, or none. `on` is what is already true, which a question
/// editing a declaration opens with.
pub fn multi(question: &str, options: &[Choice], on: &[usize]) -> Result<Answer, String> {
    inline(options.len() + 1, |terminal| {
        toggle(terminal, question, options, on)
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
    held: &[usize],
) -> Result<Answer, String> {
    let done = options.len();
    let mut state = ListState::default().with_selected(Some(0));
    let mut on: Vec<usize> = held.to_vec();
    loop {
        terminal
            .draw(|frame| draw(frame, question, options, Some(&on), TOGGLE, &mut state))
            .map_err(|err| err.to_string())?;
        let Some(key) = read()? else { continue };
        match key {
            KeyCode::Enter if state.selected() == Some(done) => return Ok(Answer::Chosen(on)),
            KeyCode::Enter | KeyCode::Char(' ') => match state.selected() {
                Some(at) if at < done => flip(&mut on, at, options),
                _ => {}
            },
            KeyCode::Esc | KeyCode::Char('q') => return Ok(Answer::Cancelled),
            code => move_by(code, &mut state),
        }
    }
}

/// Turning a row on clears the parent it contradicts and every child of it.
fn flip(on: &mut Vec<usize>, at: usize, options: &[Choice]) {
    if let Some(held) = on.iter().position(|held| *held == at) {
        on.remove(held);
        return;
    }
    let parent = options[at].parent;
    on.retain(|held| Some(*held) != parent && options[*held].parent != Some(at));
    on.push(at);
}

/// What draws a child under its parent, and nothing for a row with none.
fn branch(options: &[Choice], at: usize) -> &'static str {
    let Some(parent) = options[at].parent else {
        return "";
    };
    match options[at + 1..]
        .iter()
        .any(|choice| choice.parent == Some(parent))
    {
        true => "\u{251c}\u{2500} ",
        false => "\u{2514}\u{2500} ",
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

    let mut items: Vec<ListItem> = options
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
                Span::raw(branch(options, at)),
                Span::raw(&choice.label),
                Span::raw("  "),
                Span::styled(&choice.detail, Style::new().dim()),
            ]))
        })
        .collect();
    if on.is_some() {
        items.push(ListItem::new(Line::from(DONE.bold())));
    }
    frame.render_stateful_widget(List::new(items).highlight_symbol("> "), body, state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn drawn(options: &[Choice], on: Option<&[usize]>, hint: &str, at: usize) -> String {
        let mut state = ListState::default().with_selected(Some(at));
        let height = options.len() as u16 + 3;
        let mut terminal = Terminal::new(TestBackend::new(60, height)).unwrap();
        terminal
            .draw(|frame| draw(frame, "which module", options, on, hint, &mut state))
            .unwrap();
        terminal.backend().to_string()
    }

    fn tree() -> Vec<Choice> {
        vec![
            Choice::new("linux-desktop", ""),
            Choice::new("dx", "").under(0),
            Choice::new("gaming", "").under(0),
            Choice::new("linux-server", ""),
        ]
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

    #[test]
    fn the_confirm_row_is_the_last_row_of_a_question_taking_several() {
        let options = [Choice::new("gaming", "")];
        let several = drawn(&options, Some(&[]), TOGGLE, 1);
        assert!(several.contains(&format!("> {DONE}")), "{several}");
        let one = drawn(&options, None, PICK, 0);
        assert!(!one.contains(DONE), "{one}");
    }

    #[test]
    fn a_child_is_drawn_under_its_parent_and_the_last_one_closes_the_branch() {
        let drawn = drawn(&tree(), Some(&[]), TOGGLE, 0);
        assert!(drawn.contains("> [ ] linux-desktop"), "{drawn}");
        assert!(drawn.contains("  [ ] \u{251c}\u{2500} dx"), "{drawn}");
        assert!(drawn.contains("  [ ] \u{2514}\u{2500} gaming"), "{drawn}");
        assert!(drawn.contains("  [ ] linux-server"), "{drawn}");
    }

    #[test]
    fn a_parent_and_a_child_cannot_both_be_on() {
        let options = tree();
        let mut on = vec![0];
        flip(&mut on, 1, &options);
        assert_eq!(on, vec![1]);
        flip(&mut on, 0, &options);
        assert_eq!(on, vec![0]);
    }

    #[test]
    fn two_children_of_one_parent_can() {
        let options = tree();
        let mut on = Vec::new();
        flip(&mut on, 1, &options);
        flip(&mut on, 2, &options);
        flip(&mut on, 3, &options);
        assert_eq!(on, vec![1, 2, 3]);
    }

    #[test]
    fn flipping_a_held_row_turns_it_off_and_touches_nothing_else() {
        let options = tree();
        let mut on = vec![1, 3];
        flip(&mut on, 1, &options);
        assert_eq!(on, vec![3]);
    }
}
