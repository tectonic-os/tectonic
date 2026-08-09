//! The only place ratatui appears. Widgets draw into a bounded region of the
//! normal scroll, never an alternate screen, and only where the caller has
//! already found a terminal to draw on.

use ratatui::backend::Backend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};
use ratatui::{Frame, TerminalOptions, Viewport};

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

/// How many options are on screen at once; the rest scroll under them.
const VISIBLE: usize = 8;

const HINT: &str = "up and down to move, enter to choose, esc for none";

/// One of `options`, or none. The region is cleared before this returns, so what
/// the caller prints afterwards lands where the region was.
pub fn select(question: &str, options: &[Choice]) -> Result<Option<usize>, String> {
    let height = (options.len().min(VISIBLE) + 2) as u16;
    let mut terminal = ratatui::try_init_with_options(TerminalOptions {
        viewport: Viewport::Inline(height),
    })
    .map_err(|err| err.to_string())?;

    let chosen = run(&mut terminal, question, options);

    let origin = terminal.get_frame().area().as_position();
    let _ = terminal.clear();
    let _ = terminal.set_cursor_position(origin);
    ratatui::restore();
    chosen
}

fn run<B: Backend>(
    terminal: &mut ratatui::Terminal<B>,
    question: &str,
    options: &[Choice],
) -> Result<Option<usize>, String> {
    let mut state = ListState::default().with_selected(Some(0));
    loop {
        terminal
            .draw(|frame| draw(frame, question, options, &mut state))
            .map_err(|err| err.to_string())?;
        let Event::Key(key) = event::read().map_err(|err| err.to_string())? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => state.select_previous(),
            KeyCode::Down | KeyCode::Char('j') => state.select_next(),
            KeyCode::Home => state.select_first(),
            KeyCode::End => state.select_last(),
            KeyCode::Enter => return Ok(state.selected()),
            KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Err("interrupted".to_string())
            }
            _ => {}
        }
    }
}

fn draw(frame: &mut Frame, question: &str, options: &[Choice], state: &mut ListState) {
    let [head, body, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    frame.render_widget(Line::from(question.bold()), head);
    frame.render_widget(Line::from(HINT.dim()), foot);

    let items: Vec<ListItem> = options
        .iter()
        .map(|choice| {
            ListItem::new(Line::from(vec![
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

    #[test]
    fn every_label_is_drawn_with_its_detail_and_the_selection_marked() {
        let options = [
            Choice::new("gaming", "steam and the rest"),
            Choice::new("starship", "requires shell-config"),
        ];
        let mut state = ListState::default().with_selected(Some(1));
        let mut terminal = Terminal::new(TestBackend::new(60, 4)).unwrap();
        terminal
            .draw(|frame| draw(frame, "which module", &options, &mut state))
            .unwrap();

        let drawn = terminal.backend().to_string();
        assert!(drawn.contains("which module"), "{drawn}");
        assert!(drawn.contains("  gaming  steam and the rest"), "{drawn}");
        assert!(
            drawn.contains("> starship  requires shell-config"),
            "{drawn}"
        );
        assert!(drawn.contains(HINT), "{drawn}");
    }
}
