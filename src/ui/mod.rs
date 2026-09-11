//! The only place ratatui appears. Widgets draw into a bounded region of the
//! normal scroll, never an alternate screen, and only where the caller has
//! already found a terminal to draw on.

pub mod table;
pub mod tree;

use crate::copy::{EITHER, LINE_KEYS, NEST, PICK, TOGGLE};

use ratatui::backend::Backend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::crossterm::terminal;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Padding, Paragraph};
use ratatui::{DefaultTerminal, Frame, TerminalOptions, Viewport};
use std::io::IsTerminal;
use std::sync::OnceLock;

/// What a terminal that will not say how wide it is is taken to be.
const NARROWEST: usize = 80;

/// Whether anything drawn is being watched, which is what decides both colour
/// and whether a read-out is a table or the markdown a file would hold.
pub(crate) fn colour() -> bool {
    std::io::stdout().is_terminal()
}

/// Asked of the terminal only where the output is one, so a redirected run and
/// a piped one draw the same thing whatever is behind them. `COLUMNS` names a
/// width the terminal will not say, and a width of nothing is the narrowest.
pub(crate) fn width() -> usize {
    if !colour() {
        return NARROWEST;
    }
    std::env::var("COLUMNS")
        .ok()
        .and_then(|cols| parse_width(&cols).map(usize::from))
        .or_else(|| {
            terminal::size()
                .map(|(cols, _)| usize::from(cols))
                .ok()
                .filter(|cols| *cols > 0)
        })
        .unwrap_or(NARROWEST)
}

fn parse_width(width: &str) -> Option<u16> {
    width.parse().ok().filter(|width| *width > 0)
}

/// Whether the terminal is wide enough that no table folds a word mid-way,
/// which is the point at which a read-out falls back to its markdown.
pub(crate) fn fits(parts: &[crate::emit::Part]) -> bool {
    let room = width();
    parts.iter().all(|part| match part {
        crate::emit::Part::Table(table) => room >= table::floor(table.header, &table.rows),
        _ => true,
    })
}

pub fn parts(parts: &[crate::emit::Part]) -> String {
    parts
        .iter()
        .map(|part| match part {
            crate::emit::Part::Heading(text) => {
                ratatui::crossterm::style::Stylize::bold(text.as_str()).to_string()
            }
            crate::emit::Part::Text(text) => table::wrap(text, width()).join("\n"),
            crate::emit::Part::Table(table) => {
                table::render(&table.title, table.header, &table.rows)
                    .trim_end()
                    .to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
        + "\n"
}

/// One option, and what a person needs to see to pick between it and the rest.
pub struct Choice {
    pub label: String,
    pub detail: String,
    /// The row this one is nested under, which comes before it. A parent and
    /// any of its children contradict; two children of one parent do not.
    pub parent: Option<usize>,
    /// The dotted group this one sits inside. A group *contains* its rows; a
    /// `parent` contradicts them. A question taking several answers draws a
    /// group as a collapsed tree. Nothing reads it and `parent` both.
    pub group: String,
    /// Whether it can be picked at all. One that cannot is shown anyway and
    /// refuses the key that would pick it: what it needs is the reason it is
    /// worth showing.
    pub available: bool,
    /// Whether it is drawn dim, which is what reads as absent from the answer.
    /// `unavailable` sets it, a refused option being exactly that. `content`
    /// leaves it clear.
    pub dim: bool,
    /// Whether the label is drawn a character at a time, each character tinted
    /// by what it is. `tinted` is the drawing and the reason for it.
    pub tint: bool,
}

impl Choice {
    pub fn new(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: detail.into(),
            parent: None,
            group: String::new(),
            available: true,
            dim: false,
            tint: false,
        }
    }

    /// Shown, and not pickable. The detail beside it says why.
    pub fn unavailable(mut self) -> Self {
        self.available = false;
        self.dim = true;
        self
    }

    /// On the screen to be read: not pickable, and at full contrast. Dim here
    /// would read as absent from the answer, which a recovery key never is.
    pub fn content(mut self) -> Self {
        self.available = false;
        self
    }

    /// Drawn a character at a time, each character tinted by what it is. For a
    /// recovery key: sixty-four characters of hex run together, held in no
    /// file, copied off this screen by eye.
    pub fn tinted(mut self) -> Self {
        self.tint = true;
        self
    }

    pub fn under(mut self, parent: usize) -> Self {
        self.parent = Some(parent);
        self
    }

    /// Options sharing a group must be given together, or the group is drawn
    /// once for each run of them.
    pub fn within(mut self, group: impl Into<String>) -> Self {
        self.group = group.into();
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

/// One of `options`, or none.
pub fn select(question: &str, options: &[Choice]) -> Result<Option<usize>, String> {
    select_current(question, options, 0)
}

/// The same, editing an existing answer: it opens on `at`, so a question asked
/// again is not navigated again.
pub fn select_current(
    question: &str,
    options: &[Choice],
    at: usize,
) -> Result<Option<usize>, String> {
    inline(height(options.len()), |terminal| {
        pick(terminal, question, options, PICK, at)
    })
}

/// Which of `yes` and `no`, drawn as the two answers they are.
pub fn confirm(question: &str, yes: &str, no: &str) -> Result<bool, String> {
    confirm_current(question, yes, no, true)
}

/// A confirmation editing an existing answer opens on that answer.
pub fn confirm_current(question: &str, yes: &str, no: &str, current: bool) -> Result<bool, String> {
    let options = [Choice::new(yes, ""), Choice::new(no, "")];
    let chosen = inline(height(options.len()), |terminal| {
        pick(terminal, question, &options, EITHER, usize::from(!current))
    })?;
    Ok(chosen == Some(0))
}

/// Any of `options`, or none. `on` is what is already true, which a question
/// editing a declaration opens with. Options carrying a `group` are drawn as a
/// collapsed tree with a filter.
pub fn multi(question: &str, options: &[Choice], on: &[usize]) -> Result<Answer, String> {
    if options.iter().any(|choice| !choice.group.is_empty()) {
        let rows = nodes(options).len();
        return inline(height(rows) + 2, |terminal| {
            nest(terminal, question, options, on)
        });
    }
    inline(height(options.len()), |terminal| {
        toggle(terminal, question, options, on)
    })
}

/// One row of a form, and what a person does to it. Every field is answered on
/// the screen it is read from: nothing here opens a screen of its own.
pub enum Field {
    /// Typed in place.
    Text { label: String, value: String },
    /// Typed in place and drawn as its length.
    Secret { label: String, value: String },
    /// Opened in place: the options appear under the row until one is taken.
    Pick {
        label: String,
        options: Vec<Choice>,
        at: Option<usize>,
    },
    /// Shown and not answerable. It is on the form because a person about to
    /// erase a disk should see what is going onto it.
    Fixed { label: String, value: String },
}

impl Field {
    pub fn text(label: &str, value: &str) -> Self {
        Self::Text {
            label: label.to_string(),
            value: value.to_string(),
        }
    }

    pub fn secret(label: &str, value: &str) -> Self {
        Self::Secret {
            label: label.to_string(),
            value: value.to_string(),
        }
    }

    pub fn pick(label: &str, options: Vec<Choice>, at: Option<usize>) -> Self {
        Self::Pick {
            label: label.to_string(),
            options,
            at,
        }
    }

    pub fn fixed(label: &str, value: &str) -> Self {
        Self::Fixed {
            label: label.to_string(),
            value: value.to_string(),
        }
    }

    /// Whether a person can answer it, which is what the cursor skips over
    /// when a field is answered and the next one opens.
    fn answerable(&self) -> bool {
        !matches!(self, Self::Fixed { .. })
    }

    fn label(&self) -> &str {
        match self {
            Self::Text { label, .. }
            | Self::Secret { label, .. }
            | Self::Pick { label, .. }
            | Self::Fixed { label, .. } => label,
        }
    }

    /// The answer, as the caller reads it back.
    pub fn value(&self) -> String {
        match self {
            Self::Text { value, .. } | Self::Secret { value, .. } | Self::Fixed { value, .. } => {
                value.clone()
            }
            Self::Pick { options, at, .. } => at
                .and_then(|at| options.get(at))
                .map(|choice| choice.label.clone())
                .unwrap_or_default(),
        }
    }

    /// What the row reads as. A field nobody has answered says so, because an
    /// empty column on a form does not say whether there is a value in it.
    fn shown(&self) -> String {
        match self {
            Self::Secret { value, .. } if !value.is_empty() => "*".repeat(value.chars().count()),
            _ => match self.value() {
                answer if answer.is_empty() => crate::copy::NOT_SET.to_string(),
                answer => answer,
            },
        }
    }

    /// What is drawn while it is being typed into, where that differs from
    /// what is drawn the rest of the time.
    fn typing(&self) -> String {
        match self {
            Self::Secret { value, .. } => "*".repeat(value.chars().count()),
            _ => self.value(),
        }
    }

    fn push(&mut self, letter: char) {
        match self {
            Self::Text { value, .. } | Self::Secret { value, .. } => value.push(letter),
            Self::Pick { .. } | Self::Fixed { .. } => {}
        }
    }

    fn pop(&mut self) {
        match self {
            Self::Text { value, .. } | Self::Secret { value, .. } => {
                value.pop();
            }
            Self::Pick { .. } | Self::Fixed { .. } => {}
        }
    }
}

/// What the form came back with. A leave key is the caller's to interpret: on
/// an installer it is a question, and elsewhere it is an exit.
pub enum Filled {
    /// One of the actions, by index. The fields hold the answers.
    Took(usize),
    Left,
}

/// Which of the three things a key means, which is the whole of the form's
/// state beside the cursor.
enum Mode {
    Rows,
    Typing,
    Open(usize),
}

/// A screen holding every question at once. `actions` are the buttons under the
/// rows, and `blocked` answers why the first of them cannot be taken — drawn
/// dim and refusing the key, with the reason under it.
///
/// `blocked` is asked on every draw. It judges the fields being edited on this
/// screen, so a reason computed before the loop leaves the action dim however
/// complete the answers become.
pub fn form(
    fields: &mut [Field],
    actions: &[&str],
    blocked: impl Fn(&[Field]) -> Option<String>,
    shown: impl Fn(&[Field]) -> Vec<usize>,
    keys: &str,
) -> Result<Filled, String> {
    let rows = fields.len() + 3;
    inline(rows as u16, |terminal| {
        let mut cursor = 0usize;
        let mut button = 0usize;
        let mut mode = Mode::Rows;
        loop {
            // Which rows there are is asked on every draw, for the same reason
            // `blocked` is: a field can decide whether another one is a
            // question at all, and the answer changes on this screen.
            let visible = shown(fields);
            cursor = cursor.min(visible.len());
            let at_row = visible.get(cursor).copied();
            let blocked = blocked(fields);
            let open = match mode {
                Mode::Open(at) => Some(at),
                _ => None,
            };
            let typing = matches!(mode, Mode::Typing);
            let lines = laid_out(
                fields,
                &visible,
                cursor,
                button,
                open,
                typing,
                actions,
                blocked.as_deref(),
            );
            render(terminal, lines.len() as u16 + 1, keys, |frame, area| {
                sheet_of(frame, area, &lines, keys)
            })?;
            let Some(key) = read()? else { continue };
            let Some(row) = at_row else {
                // The actions row, which is the one row that is not a field.
                match key {
                    KeyCode::Esc | KeyCode::Char('q') => return Ok(Filled::Left),
                    KeyCode::Up | KeyCode::Char('k') => cursor = cursor.saturating_sub(1),
                    KeyCode::Left => button = button.saturating_sub(1),
                    KeyCode::Right => button = (button + 1).min(actions.len() - 1),
                    // The first action is the one `blocked` speaks for. The
                    // rest are always there, because a screen you cannot leave
                    // is worse than one you cannot finish.
                    KeyCode::Enter if button > 0 || blocked.is_none() => {
                        return Ok(Filled::Took(button))
                    }
                    _ => {}
                }
                continue;
            };
            match &mut mode {
                Mode::Typing => match key {
                    // Answering a field moves on to the next and opens it, so a
                    // form is filled top to bottom. Esc is not an answer and
                    // stays where it is.
                    KeyCode::Enter => mode = onward(fields, &visible, &mut cursor),
                    KeyCode::Esc => mode = Mode::Rows,
                    // Up and down move between fields while one is being typed
                    // into: every field opens for typing as the cursor reaches
                    // it, so without this the arrow keys stop working the moment
                    // the form starts being filled in.
                    KeyCode::Up => mode = backward(fields, &visible, &mut cursor),
                    KeyCode::Down => mode = onward(fields, &visible, &mut cursor),
                    KeyCode::Backspace => fields[row].pop(),
                    KeyCode::Char(letter) => fields[row].push(letter),
                    _ => {}
                },
                Mode::Open(at) => {
                    let Field::Pick { options, .. } = &fields[row] else {
                        mode = Mode::Rows;
                        continue;
                    };
                    match key {
                        KeyCode::Enter => {
                            let taken = *at;
                            if available(options, taken) {
                                if let Field::Pick { at: held, .. } = &mut fields[row] {
                                    *held = Some(taken);
                                }
                                mode = onward(fields, &shown(fields), &mut cursor);
                            }
                        }
                        KeyCode::Esc | KeyCode::Char('q') => mode = Mode::Rows,
                        KeyCode::Up | KeyCode::Char('k') => *at = at.saturating_sub(1),
                        KeyCode::Down | KeyCode::Char('j') => {
                            *at = (*at + 1).min(options.len().saturating_sub(1))
                        }
                        _ => {}
                    }
                }
                Mode::Rows => match key {
                    KeyCode::Esc | KeyCode::Char('q') => return Ok(Filled::Left),
                    KeyCode::Up | KeyCode::Char('k') => cursor = cursor.saturating_sub(1),
                    KeyCode::Down | KeyCode::Char('j') => cursor = (cursor + 1).min(visible.len()),
                    KeyCode::Enter => mode = opened(&fields[row]),
                    _ => {}
                },
            }
        }
    })
}

/// The row after this one, opened. A form is a list of questions and answering
/// one asks the next; the actions row is where that stops, since taking an
/// action is a decision and not an answer.
fn onward(fields: &[Field], visible: &[usize], cursor: &mut usize) -> Mode {
    loop {
        *cursor = (*cursor + 1).min(visible.len());
        match visible.get(*cursor) {
            // A row nobody can answer is not a stop on the way down.
            Some(row) if !fields[*row].answerable() && *cursor < visible.len() => continue,
            Some(row) => return opened(&fields[*row]),
            None => return Mode::Rows,
        }
    }
}

/// The row before this one, opened. The pair to `onward`, for the arrow that
/// goes the other way.
fn backward(fields: &[Field], visible: &[usize], cursor: &mut usize) -> Mode {
    loop {
        let above = cursor.saturating_sub(1);
        let stuck = above == *cursor;
        *cursor = above;
        match visible.get(*cursor) {
            Some(row) if !fields[*row].answerable() && !stuck => continue,
            Some(row) => return opened(&fields[*row]),
            None => return Mode::Rows,
        }
    }
}

/// What editing a field means, which is the only thing that differs between
/// one that is typed and one that is chosen.
fn opened(field: &Field) -> Mode {
    match field {
        Field::Pick { at, .. } => Mode::Open(at.unwrap_or(0)),
        Field::Fixed { .. } => Mode::Rows,
        _ => Mode::Typing,
    }
}

/// The rows as they are drawn: every field, the options under whichever one is
/// open, a blank, the actions, and what stops the first of them.
///
/// `open` is the option the cursor is on inside `fields[cursor]`, which is the
/// only field that can be open: a form shows one list at a time.
#[allow(clippy::too_many_arguments)]
fn laid_out(
    fields: &[Field],
    visible: &[usize],
    cursor: usize,
    button: usize,
    open: Option<usize>,
    typing: bool,
    actions: &[&str],
    blocked: Option<&str>,
) -> Vec<Line<'static>> {
    let width = visible
        .iter()
        .map(|row| fields[*row].label().chars().count())
        .max()
        .unwrap_or(0);
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (at, row) in visible.iter().enumerate() {
        let field = &fields[*row];
        let here = at == cursor;
        let unanswered = field.value().is_empty();
        // The block is the caret: this draws no cursor of its own, and a field
        // being typed into has to look different from one that is not.
        let value = match here && typing {
            true => format!("{}\u{2588}", field.typing()),
            false => field.shown(),
        };
        // The row the cursor is on carries the accent, label and all. A marker
        // alone is what a list uses; a form is read down its labels.
        let label = match (here, field.answerable()) {
            (true, _) => Style::new().fg(HIGHLIGHT).bold(),
            // A row nobody answers is dim, so the form says which of its rows
            // are questions without being told.
            (false, false) => Style::new().dim(),
            (false, true) => Style::new(),
        };
        lines.push(Line::from(vec![
            Span::styled(
                match here {
                    true => "> ",
                    false => "  ",
                },
                label,
            ),
            Span::styled(format!("{:<width$}  ", field.label()), label),
            match unanswered && !typing {
                true => Span::styled(value, Style::new().dim()),
                false => Span::raw(value),
            },
        ]));
        let (Some(open), Field::Pick { options, .. }) = (open.filter(|_| here), field) else {
            continue;
        };
        for (n, choice) in options.iter().enumerate() {
            let on_it = n == open;
            let row = match (choice.available, on_it) {
                (false, _) => Style::new().dim(),
                (true, true) => Style::new().fg(HIGHLIGHT).bold(),
                (true, false) => Style::new(),
            };
            lines.push(Line::from(vec![
                Span::styled(
                    match on_it {
                        true => "    > ",
                        false => "      ",
                    },
                    row,
                ),
                Span::styled(choice.label.clone(), row),
                Span::raw("  "),
                Span::styled(choice.detail.clone(), Style::new().dim()),
            ]));
        }
    }

    lines.push(Line::default());
    let on_actions = cursor == visible.len();
    let mut buttons: Vec<Span<'static>> = vec![Span::raw("  ")];
    for (n, action) in actions.iter().enumerate() {
        // Only the first action is ever blocked. The rest are the way off this
        // screen, and a screen nobody can leave is worse than one nobody can
        // finish.
        let style = match (n == 0 && blocked.is_some(), on_actions && n == button) {
            (true, true) => Style::new().dim().reversed(),
            (true, false) => Style::new().dim(),
            (false, true) => Style::new().bold().reversed(),
            (false, false) => Style::new(),
        };
        buttons.push(Span::styled(format!("  {action}  "), style));
        buttons.push(Span::raw(" "));
    }
    lines.push(Line::from(buttons));
    if let Some(why) = blocked {
        lines.push(Line::from(Span::styled(
            format!("  {why}"),
            Style::new().dim(),
        )));
    }
    lines
}

/// The rows, and the keys under them. No question head: a form's rows say what
/// they are, and the container's title says what is being filled in.
fn sheet_of(frame: &mut Frame, area: Rect, lines: &[Line<'static>], keys: &str) {
    // The row is only reserved where this widget is the one drawing the hint.
    // Inside a box the box's bottom edge has it, and a blank row under the
    // actions is a row of nothing.
    let keys = hint_row(keys);
    let [body, foot] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(u16::from(!keys.is_empty())),
    ])
    .areas(area);
    frame.render_widget(Paragraph::new(lines.to_vec()), body);
    frame.render_widget(Line::from(keys.dim()), foot);
}

/// The answers a command has collected, one row per piece of configuration,
/// with `action` under them. `Some(rows.len())` is `action`, anything smaller
/// is the row to ask again, and `None` is a cancel.
///
/// It sizes to its rows, which the command's questions bound, so the action
/// cannot scroll off. `blocked` is why the action cannot be taken yet, which
/// draws it dim and unpickable with the reason beside it; `None` is a screen
/// whose answers are complete. `at` is the row it opens on.
pub fn review(
    question: &str,
    rows: &[(String, String)],
    action: &str,
    keys: &str,
    blocked: Option<&str>,
    at: usize,
) -> Result<Option<usize>, String> {
    let options = sheet(rows, action, blocked);
    let chosen = inline((options.len() + 2) as u16, |terminal| {
        pick(
            terminal,
            question,
            &options,
            keys,
            at.min(options.len() - 1),
        )
    })?;
    // The spacer is never landed on, so anything past the last row is the action.
    Ok(chosen.map(|at| at.min(rows.len())))
}

/// A question over a read-only summary of what answering it would do: the rows
/// are shown and cannot be landed on, and the two answers sit under them.
/// `true` is `yes`; esc is `no`.
pub fn confirm_over(
    question: &str,
    rows: &[(String, String)],
    yes: &str,
    no: &str,
) -> Result<bool, String> {
    let options = summary_sheet(rows, yes, no);
    let at = options.len() - 2;
    let chosen = inline((options.len() + 2) as u16, |terminal| {
        pick(terminal, question, &options, EITHER, at)
    })?;
    Ok(chosen == Some(at))
}

/// One thing to do, over the same read-only rows `confirm_over` draws. The way
/// out is esc and `keys` says so. A widget that sets `CHROME` opens full screen
/// and paints over anything already printed under it.
pub fn offer_over(
    question: &str,
    rows: Vec<Choice>,
    action: &str,
    keys: &str,
) -> Result<bool, String> {
    let mut options = rows;
    options.push(Choice::new("", ""));
    options.push(Choice::new(action, ""));
    let at = options.len() - 1;
    let chosen = inline((options.len() + 2) as u16, |terminal| {
        pick(terminal, question, &options, keys, at)
    })?;
    Ok(chosen == Some(at))
}

/// The summary and the answers under it. Every row of the summary is
/// unavailable, so the cursor passes over what it is being asked about and
/// lands only on an answer.
fn summary_sheet(rows: &[(String, String)], yes: &str, no: &str) -> Vec<Choice> {
    let width = rows
        .iter()
        .map(|(label, _)| label.chars().count())
        .max()
        .unwrap_or(0);
    let mut options: Vec<Choice> = rows
        .iter()
        .map(|(label, value)| Choice::new(format!("{label:<width$}"), value).unavailable())
        .collect();
    options.push(Choice::new("", ""));
    options.push(Choice::new(yes, ""));
    options.push(Choice::new(no, ""));
    options
}

/// The rows as they are drawn: labels padded to one column, a spacer, and the
/// action last, which is dim and unpickable for as long as anything it needs
/// is missing.
fn sheet(rows: &[(String, String)], action: &str, blocked: Option<&str>) -> Vec<Choice> {
    let width = rows
        .iter()
        .map(|(label, _)| label.chars().count())
        .max()
        .unwrap_or(0);
    let mut options: Vec<Choice> = rows
        .iter()
        .map(|(label, value)| Choice::new(format!("{label:<width$}"), value))
        .collect();
    options.push(Choice::new("", ""));
    options.push(match blocked {
        None => Choice::new(action, ""),
        Some(why) => Choice::new(action, why).unavailable(),
    });
    options
}

/// A line typed and shown, with a default standing in until one is typed.
/// Enter answers with what is there and esc with nothing, which the caller
/// turns into its default or the refusal naming its flag. `prefix` stands
/// before the answer and is not part of it.
pub fn line(question: &str, prefix: &str, default: Option<&str>) -> Result<String, String> {
    inline(3, |terminal| {
        let mut typed = String::new();
        loop {
            render(terminal, 3, LINE_KEYS, |frame, area| {
                written(frame, area, question, prefix, &typed, default)
            })?;
            let Some(key) = read()? else { continue };
            match key {
                KeyCode::Enter => return Ok(typed),
                KeyCode::Esc => return Ok(String::new()),
                KeyCode::Backspace => {
                    typed.pop();
                }
                KeyCode::Char(letter) => typed.push(letter),
                _ => {}
            }
        }
    })
}

/// The default is drawn dim where the answer will be, because it is what enter
/// takes.
fn written(
    frame: &mut Frame,
    area: Rect,
    question: &str,
    prefix: &str,
    typed: &str,
    default: Option<&str>,
) {
    let [head, body, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);
    frame.render_widget(Line::from(question.bold().cyan()), head);
    let answer = match (typed.is_empty(), default) {
        (true, Some(default)) => Span::styled(default, Style::new().dim()),
        _ => Span::raw(typed),
    };
    frame.render_widget(Line::from(vec![Span::raw(prefix), answer]), body);
    frame.render_widget(Line::from(hint_row(LINE_KEYS).dim()), foot);
}

/// How many of the messages under the gauge are kept. They are what a step is
/// doing. The record of it is the log file, so a few is the whole point.
const NOTED: usize = 5;

/// The bar, the step, the messages under their rule, and the line that does
/// not move.
const ROWS: u16 = NOTED as u16 + 4;

/// A bounded region over something that takes a while, held open across an
/// event stream, which is why this is a handle. Every other widget in this
/// file is a closure.
pub struct Progress {
    terminal: DefaultTerminal,
    /// What was finished before the step now running.
    pct: u16,
    /// The share of the whole that step carries.
    flight: u16,
    /// How many messages have arrived during it, which is the only signal
    /// there is for how far into it the machine has got.
    within: u32,
    /// Which frame of the spinner is drawn beside the step.
    turn: usize,
    step: String,
    notes: Vec<String>,
    foot: String,
}

impl Progress {
    /// `foot` is the one line that stays put under the messages: where the
    /// transcript is being written, and what cancelling would now mean.
    ///
    /// Raw mode is turned back off: nothing is read from the keyboard while
    /// this is open, and leaving it on would make Ctrl+C a key nobody reads.
    /// A region held over an hour of work that can hang must stay interruptible.
    pub fn open(foot: &str) -> Result<Self, String> {
        let terminal = open(ROWS)?;
        let _ = terminal::disable_raw_mode();
        Ok(Self {
            terminal,
            pct: 0,
            flight: 0,
            within: 0,
            turn: 0,
            step: String::new(),
            notes: Vec::new(),
            foot: foot.to_string(),
        })
    }

    /// A step: the gauge moves, and the messages start again because they
    /// belong to the step they were written under.
    pub fn step(&mut self, pct: u16, flight: u16, name: &str) -> Result<(), String> {
        self.pct = pct.min(100);
        self.flight = flight.min(100);
        self.within = 0;
        self.step = name.to_string();
        self.notes.clear();
        self.show()
    }

    /// How far along the whole install the bar is drawn.
    ///
    /// fisherman says what a step weighs, never how far into it the machine
    /// has got, and one step carries most of the weight. Each message during a
    /// step takes a fixed share of what is left of that step, so the number
    /// climbs and never reaches where the next step begins. Counted, since
    /// fisherman emits no sub-step progress.
    fn at(&self) -> u16 {
        crept(self.pct, self.flight, self.within)
    }

    /// Time passing, and nothing else. A step can hold the machine for minutes
    /// between two messages, and a screen that has not changed in that long
    /// cannot be told from one that has stopped. Only the spinner moves.
    pub fn tick(&mut self) -> Result<(), String> {
        self.turn = self.turn.wrapping_add(1);
        self.show()
    }

    /// A line under the gauge, oldest dropped.
    pub fn note(&mut self, text: &str) -> Result<(), String> {
        self.within += 1;
        if self.notes.len() == NOTED {
            self.notes.remove(0);
        }
        self.notes.push(text.to_string());
        self.show()
    }

    fn show(&mut self) -> Result<(), String> {
        let (pct, turn, step, notes, foot) = (
            self.at(),
            self.turn,
            self.step.as_str(),
            self.notes.as_slice(),
            self.foot.as_str(),
        );
        render(&mut self.terminal, ROWS, foot, |frame, area| {
            working(frame, area, pct, turn, step, notes, foot)
        })
    }

    pub fn close(self) {
        close(self.terminal);
    }
}

/// The bar, the step it is on, the messages in a pane of their own, and the
/// line that does not move.
fn working(
    frame: &mut Frame,
    area: Rect,
    pct: u16,
    turn: usize,
    step: &str,
    notes: &[String],
    foot: &str,
) {
    let [meter, name, body, tail] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);
    frame.render_widget(bar(pct, meter.width), meter);
    frame.render_widget(
        Line::from(vec![
            Span::styled(TURNING[turn % TURNING.len()], Style::new().fg(HIGHLIGHT)),
            Span::raw(" "),
            Span::styled(step.to_string(), Style::new().fg(HIGHLIGHT).bold()),
        ]),
        name,
    );
    // The pane the output is bounded by, which is the whole reason a log had
    // to go somewhere: what scrolls out of it is gone from the screen.
    let pane = Block::new()
        .borders(Borders::TOP)
        .border_style(Style::new().dim())
        .title(Line::from(" output ".dim()));
    let inside = pane.inner(body);
    frame.render_widget(pane, body);
    frame.render_widget(Paragraph::new(notes.join("\n")).dim(), inside);
    frame.render_widget(Line::from(hint_row(foot).dim()), tail);
}

/// The two ends of the gradient, and of the palette: `ACCENT` and `HIGHLIGHT`
/// are these, so nothing on the screen can drift away from the bar.
const COLD: (u8, u8, u8) = (0x5a, 0x56, 0xe0);
const HOT: (u8, u8, u8) = (0xee, 0x6f, 0xf8);

/// Where the bar stands: what finished before this step, plus a share of what
/// this step weighs for each message that has arrived during it. Held out of
/// `Progress` so it can be read without a terminal.
fn crept(pct: u16, flight: u16, within: u32) -> u16 {
    // `install OS` copies a blob per layer and there are dozens, so the share
    // has to be small enough that a hundred of them do not run out of bar.
    let left = HELD.powi(within.min(400) as i32);
    pct + (f64::from(flight) * (1.0 - left)) as u16
}

/// What is left of a step after one more message during it.
const HELD: f64 = 0.97;

/// One fill that only ever grows, with the percentage after it; `Progress::at`
/// advances it. Solid the whole way across with the track in grey — no partial
/// blocks, since a serial console has none, and the kernel VT the media falls
/// back to has grey as its own colour 8.
fn bar<'a>(pct: u16, width: u16) -> Line<'a> {
    let label = format!(" {pct:>3}%");
    let room = usize::from(width).saturating_sub(label.chars().count());
    let done = room * usize::from(pct.min(100)) / 100;
    let mut spans: Vec<Span> = (0..room)
        .map(|at| match at < done {
            true => Span::styled("\u{2588}", Style::new().fg(blend(at, room))),
            false => Span::styled("\u{2588}", Style::new().fg(TRACK)),
        })
        .collect();
    spans.push(Span::styled(label, Style::new().bold()));
    Line::from(spans)
}

/// The bar's unfilled length, and the legend on the box's edge.
const TRACK: Color = Color::DarkGray;

/// The spinner beside the running step. Braille, which the media's console
/// draws and a kernel VT has no glyphs for.
const TURNING: [&str; 10] = [
    "\u{280b}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283c}", "\u{2834}", "\u{2826}", "\u{2827}",
    "\u{2807}", "\u{280f}",
];

/// How far along the gradient cell `at` of `room` is.
fn blend(at: usize, room: usize) -> Color {
    let step = |cold: u8, hot: u8| {
        let (cold, hot) = (i32::from(cold), i32::from(hot));
        (cold + (hot - cold) * at as i32 / room.max(1) as i32) as u8
    };
    Color::Rgb(
        step(COLD.0, HOT.0),
        step(COLD.1, HOT.1),
        step(COLD.2, HOT.2),
    )
}

/// The palette's own colour lifted towards white. A tinted row is read one
/// character at a time, so it wants hue and full contrast.
fn lifted(base: (u8, u8, u8)) -> Color {
    let lift = |channel: u8| (u16::from(channel) + (0xff - u16::from(channel)) * 2 / 3) as u8;
    Color::Rgb(lift(base.0), lift(base.1), lift(base.2))
}

/// A label drawn one character at a time: digits off the gradient's hot end,
/// letters off its cold one, everything else white. Three classes, because the
/// pairs a person confuses — `0` and `O`, `1` and `l`, `5` and `S` — are one
/// from each, so colour separates what shape does not.
fn tinted(label: &str) -> Vec<Span<'static>> {
    label
        .chars()
        .map(|letter| {
            let colour = match letter {
                _ if letter.is_ascii_digit() => lifted(HOT),
                _ if letter.is_alphabetic() => lifted(COLD),
                _ => Color::White,
            };
            Span::styled(letter.to_string(), Style::new().fg(colour))
        })
        .collect()
}

/// The lines a question takes: its rows, whatever they are, under the question
/// and over the hint. Unused where a command has taken the screen, since the
/// viewport is then the terminal and nothing sizes to its content.
fn height(rows: usize) -> u16 {
    (rows.min(VISIBLE) + 2) as u16
}

/// A serial console comes up 0x0 and nothing on it ever sends `SIGWINCH`, so a
/// viewport laid out for the size it is told draws nothing at all. Setting the
/// size here covers every way the tool is started.
fn give_size() {
    if !unsized_tty(terminal::size().ok()) {
        return;
    }
    let size = libc::winsize {
        ws_row: 24,
        ws_col: NARROWEST as u16,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCSWINSZ, &size) };
}

fn unsized_tty(size: Option<(u16, u16)>) -> bool {
    !matches!(size, Some((cols, rows)) if cols > 0 && rows > 0)
}

/// The title bar a command that owns the console draws over every widget,
/// which is also what puts them all in a full-screen viewport. Unset is the
/// normal case: a bounded region of the scroll, with no chrome.
///
/// Global, so one process expresses one mode.
static CHROME: OnceLock<String> = OnceLock::new();

/// Called once, at the entry of a command that owns the screen until it is
/// done. Nothing reads it back and nothing can unset it.
pub fn own_screen(title: impl Into<String>) {
    let _ = CHROME.set(title.into());
}

/// The viewport, raw mode and the size a serial console will not say. Paired
/// with `close`, which every path out of a widget goes through.
fn open(height: u16) -> Result<DefaultTerminal, String> {
    give_size();
    let viewport = match CHROME.get() {
        Some(_) => Viewport::Fullscreen,
        None => Viewport::Inline(height),
    };
    ratatui::try_init_with_options(TerminalOptions { viewport }).map_err(|err| err.to_string())
}

/// Cleared, and the cursor put back where the region started, so what the
/// caller prints afterwards lands where the region was.
fn close(mut terminal: DefaultTerminal) {
    let origin = terminal.get_frame().area().as_position();
    let _ = terminal.clear();
    let _ = terminal.set_cursor_position(origin);
    let _ = terminal.show_cursor();
    // Not `ratatui::restore`, which also leaves an alternate screen never entered.
    let _ = terminal::disable_raw_mode();
}

/// A bounded region of the normal scroll, cleared again before this returns, so
/// what the caller prints afterwards lands where the region was.
fn inline<T>(
    height: u16,
    body: impl FnOnce(&mut DefaultTerminal) -> Result<T, String>,
) -> Result<T, String> {
    let mut terminal = open(height)?;
    let out = body(&mut terminal);
    close(terminal);
    out
}

/// What the container leaves a widget to draw in: the whole frame where there
/// is no chrome, and the inside of the box where there is. The one caller of
/// `terminal.draw`.
///
/// `rows` is what the widget wants. Inline it is the viewport and this ignores
/// it; in a box it is what the box is sized and centred on.
fn render<B: Backend>(
    terminal: &mut ratatui::Terminal<B>,
    rows: u16,
    keys: &str,
    body: impl FnOnce(&mut Frame, Rect),
) -> Result<(), String> {
    terminal
        .draw(|frame| {
            let area = chrome(frame, CHROME.get().map(String::as_str), rows, keys);
            body(frame, area);
        })
        .map(|_| ())
        .map_err(|err| err.to_string())
}

/// Past this the box stops growing and centres instead: a question read across
/// two hundred columns is a question read twice.
const WIDEST: u16 = 76;

/// Everything coloured on this screen comes off the bar's gradient: the box is
/// its cold end and the row the cursor is on is its hot one. One palette, so
/// the screen reads as one thing.
const ACCENT: Color = Color::Rgb(COLD.0, COLD.1, COLD.2);
const HIGHLIGHT: Color = Color::Rgb(HOT.0, HOT.1, HOT.2);

/// The room between the border and what it holds. A box drawn tight around its
/// content reads as a table.
const PAD_X: u16 = 3;
const PAD_Y: u16 = 1;

/// Paints the box, if there is one, and answers with the room left inside it.
/// Takes the title as a parameter: the mode is a `OnceLock` and `cargo test` is
/// one process, so a test that set it would put a title bar on every other test
/// drawing in parallel.
fn chrome(frame: &mut Frame, title: Option<&str>, rows: u16, keys: &str) -> Rect {
    let Some(title) = title else {
        return frame.area();
    };
    let full = frame.area();
    let box_area = centred(
        full,
        WIDEST.min(full.width),
        rows.saturating_add(2 + 2 * PAD_Y),
    );
    let block = Block::new()
        .borders(Borders::ALL)
        // The installer media draws through kmscon. Where it cannot start,
        // systemd hands tty1 back to the kernel VT, whose bitmap font has no
        // arc glyphs and draws these corners as `+`.
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(ACCENT))
        .padding(Padding::symmetric(PAD_X, PAD_Y))
        // The title starts one cell in, so the corner reads as a corner with a
        // line coming off it. Flush against the words it reads as a bracket.
        .title(Line::from(vec![
            Span::styled("\u{2500} ", Style::new().fg(ACCENT)),
            Span::styled(title.trim().to_string(), Style::new().bold()),
            Span::raw(" "),
        ]))
        // The keys sit on the bottom edge, where the title sits on the top.
        // Inside the box they cost a row and read as content; on the border
        // they are what they are, which is a legend.
        .title_bottom(
            Line::from(vec![
                Span::styled("\u{2500} ", Style::new().fg(ACCENT)),
                Span::styled(keys.to_string(), Style::new().fg(TRACK)),
                Span::raw(" "),
            ])
            .left_aligned(),
        );
    let inside = block.inner(box_area);
    frame.render_widget(block, box_area);
    inside
}

/// The hint a widget draws under itself, which is nothing where a box is
/// already drawing it on its bottom edge.
fn hint_row(keys: &str) -> &str {
    match CHROME.get() {
        Some(_) => "",
        None => keys,
    }
}

/// `area`, no larger than `width` by `height`, in the middle of it.
fn centred(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

fn pick<B: Backend>(
    terminal: &mut ratatui::Terminal<B>,
    question: &str,
    options: &[Choice],
    hint: &str,
    selected: usize,
) -> Result<Option<usize>, String> {
    let mut state = ListState::default().with_selected(Some(selected));
    loop {
        render(terminal, height(options.len()), hint, |frame, area| {
            draw(frame, area, question, options, None, hint, &mut state)
        })?;
        let Some(key) = read()? else { continue };
        match key {
            KeyCode::Enter => match state.selected() {
                Some(at) if !available(options, at) => {}
                chosen => return Ok(chosen),
            },
            KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
            code => {
                move_by(code, &mut state);
                skip_spacers(code, options, &mut state);
            }
        }
    }
}

/// A row with no label is a spacer, which the cursor passes over in whichever
/// direction it was sent. Only the review screen has one, and it is what puts a
/// blank line above `Create`.
fn skip_spacers(code: KeyCode, options: &[Choice], state: &mut ListState) {
    for _ in 0..options.len() {
        match state.selected() {
            Some(at) if options.get(at).is_some_and(|row| row.label.is_empty()) => {
                move_by(code, state)
            }
            _ => return,
        }
    }
}

fn toggle<B: Backend>(
    terminal: &mut ratatui::Terminal<B>,
    question: &str,
    options: &[Choice],
    held: &[usize],
) -> Result<Answer, String> {
    let mut state = ListState::default().with_selected(Some(0));
    let mut on: Vec<usize> = held.to_vec();
    loop {
        render(terminal, height(options.len()), TOGGLE, |frame, area| {
            draw(
                frame,
                area,
                question,
                options,
                Some(&on),
                TOGGLE,
                &mut state,
            )
        })?;
        let Some(key) = read()? else { continue };
        match key {
            KeyCode::Enter => return Ok(Answer::Chosen(on)),
            KeyCode::Char(' ') => match state.selected() {
                Some(at) if available(options, at) => flip(&mut on, at, options),
                _ => {}
            },
            KeyCode::Esc | KeyCode::Char('q') => return Ok(Answer::Cancelled),
            code => move_by(code, &mut state),
        }
    }
}

/// Whether the row at `at` is one there is, and one that can be picked.
fn available(options: &[Choice], at: usize) -> bool {
    options.get(at).is_some_and(|choice| choice.available)
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

/// What the interrupt is answered with, and the one error a caller is meant to
/// read. Ctrl+C in raw mode arrives as a key, so this is the whole of how a
/// widget says a person wants out.
pub const INTERRUPTED: &str = "interrupted";

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
            Err(INTERRUPTED.to_string())
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
    area: Rect,
    question: &str,
    options: &[Choice],
    on: Option<&[usize]>,
    hint_text: &str,
    state: &mut ListState,
) {
    let [head, body, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);

    frame.render_widget(Line::from(question.bold().cyan()), head);

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(at, choice)| {
            let mark = match on {
                None => "",
                Some(on) if on.contains(&at) => "[x] ",
                Some(_) => "[ ] ",
            };
            // A refused option is dim whole, so it reads as absent from the
            // answer while still saying what it needs.
            let row = match choice.dim {
                false => Style::new(),
                true => Style::new().dim(),
            };
            let mut spans = vec![
                Span::styled(mark, row),
                Span::styled(branch(options, at), row),
            ];
            match choice.tint {
                true => spans.extend(tinted(&choice.label)),
                false => spans.push(Span::styled(choice.label.clone(), row)),
            }
            spans.push(Span::raw("  "));
            spans.push(Span::styled(choice.detail.clone(), Style::new().dim()));
            ListItem::new(Line::from(spans))
        })
        .collect();
    let rows = items.len();
    frame.render_stateful_widget(List::new(items).highlight_symbol("> "), body, state);
    frame.render_widget(
        Line::from(hint(hint_row(hint_text), state, body.height, rows).dim()),
        foot,
    );
}

/// The hint, and how far down a list longer than the window this is. Nothing
/// else says there are rows under the last one on screen.
fn hint(hint: &str, state: &ListState, height: u16, rows: usize) -> String {
    let seen = (state.offset() + usize::from(height)).min(rows);
    match seen < rows || state.offset() > 0 {
        true => format!("{hint}  {seen} of {rows}"),
        false => hint.to_string(),
    }
}

/// One row of the tree: an option, or a branch holding the rows after it that
/// are deeper than it.
struct Node {
    label: String,
    depth: usize,
    at: Option<usize>,
}

/// The tree the groups describe, in the order the options were given: a branch
/// for each dotted part not already open, then the option under it.
fn nodes(options: &[Choice]) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut open: Vec<&str> = Vec::new();
    for (at, choice) in options.iter().enumerate() {
        let path: Vec<&str> = match choice.group.is_empty() {
            true => Vec::new(),
            false => choice.group.split('.').collect(),
        };
        let same = open
            .iter()
            .zip(&path)
            .take_while(|(held, part)| held == part)
            .count();
        open.truncate(same);
        for part in &path[same..] {
            open.push(part);
            nodes.push(Node {
                label: open.join("."),
                depth: open.len() - 1,
                at: None,
            });
        }
        nodes.push(Node {
            label: choice.label.clone(),
            depth: open.len(),
            at: Some(at),
        });
    }
    nodes
}

/// The options a row stands for: itself, or everything a branch contains.
fn leaves(nodes: &[Node], at: usize) -> Vec<usize> {
    if let Some(option) = nodes[at].at {
        return vec![option];
    }
    let depth = nodes[at].depth;
    nodes[at + 1..]
        .iter()
        .take_while(|node| node.depth > depth)
        .filter_map(|node| node.at)
        .collect()
}

/// Containment, which is what `Choice::parent` contradicts: a branch
/// holds what is under it, and turns all of it on until all of it is.
fn check(on: &mut Vec<usize>, nodes: &[Node], at: usize) {
    let leaves = leaves(nodes, at);
    if leaves.iter().all(|leaf| on.contains(leaf)) {
        on.retain(|held| !leaves.contains(held));
        return;
    }
    for leaf in leaves {
        if !on.contains(&leaf) {
            on.push(leaf);
        }
    }
}

/// A branch some of whose options are on is neither, which is what makes a
/// closed branch worth reading.
fn checkbox(on: &[usize], nodes: &[Node], at: usize) -> &'static str {
    let leaves = leaves(nodes, at);
    match leaves.iter().filter(|leaf| on.contains(leaf)).count() {
        0 => "[ ] ",
        held if held == leaves.len() => "[x] ",
        _ => "[-] ",
    }
}

/// What is on screen, as indices into `nodes`: every row no closed branch
/// hides, or every option the filter matches, which flattens the tree for as
/// long as one is typed.
fn shown(nodes: &[Node], options: &[Choice], open: &[bool], filter: &str) -> Vec<usize> {
    if !filter.is_empty() {
        let filter = filter.to_lowercase();
        let matches = |choice: &Choice| {
            choice.label.to_lowercase().contains(&filter)
                || choice.detail.to_lowercase().contains(&filter)
        };
        return (0..nodes.len())
            .filter(|at| {
                nodes[*at]
                    .at
                    .is_some_and(|option| matches(&options[option]))
            })
            .collect();
    }
    let mut rows = Vec::new();
    let mut hidden: Option<usize> = None;
    for (at, node) in nodes.iter().enumerate() {
        match hidden {
            Some(depth) if node.depth > depth => continue,
            _ => hidden = None,
        }
        rows.push(at);
        if node.at.is_none() && !open[at] {
            hidden = Some(node.depth);
        }
    }
    rows
}

/// The same answer `toggle` gives — indices into `options`, never into what the
/// filter left on screen.
fn nest<B: Backend>(
    terminal: &mut ratatui::Terminal<B>,
    question: &str,
    options: &[Choice],
    held: &[usize],
) -> Result<Answer, String> {
    let tree = nodes(options);
    let mut open = vec![false; tree.len()];
    let mut on = held.to_vec();
    let mut filter = String::new();
    let mut state = ListState::default().with_selected(Some(0));
    loop {
        let rows = shown(&tree, options, &open, &filter);
        render(terminal, height(tree.len()) + 2, NEST, |frame, area| {
            nested(
                frame, area, question, options, &tree, &rows, &open, &on, &filter, &mut state,
            )
        })?;
        let Some(key) = read()? else { continue };
        let row = state.selected().and_then(|at| rows.get(at)).copied();
        match key {
            KeyCode::Enter => {
                on.sort_unstable();
                return Ok(Answer::Chosen(on));
            }
            KeyCode::Esc if !filter.is_empty() => filter.clear(),
            KeyCode::Esc => return Ok(Answer::Cancelled),
            KeyCode::Backspace => {
                filter.pop();
            }
            KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right => {
                if let Some(at) = row {
                    match (tree[at].at.is_some(), key) {
                        (true, KeyCode::Left | KeyCode::Right) => {}
                        (true, _) => check(&mut on, &tree, at),
                        (false, KeyCode::Char(' ')) => check(&mut on, &tree, at),
                        (false, KeyCode::Left) => open[at] = false,
                        (false, _) => open[at] = true,
                    }
                }
            }
            KeyCode::Char(letter) => filter.push(letter),
            code => move_by(code, &mut state),
        }
    }
}

/// The rows, the detail of whatever is highlighted, and the filter as it is
/// typed. A rule's description is prose and belongs under the list, which has
/// the room a label has not.
#[allow(clippy::too_many_arguments)]
fn nested(
    frame: &mut Frame,
    area: Rect,
    question: &str,
    options: &[Choice],
    nodes: &[Node],
    rows: &[usize],
    open: &[bool],
    on: &[usize],
    filter: &str,
    state: &mut ListState,
) {
    let [head, body, detail, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(2),
        Constraint::Length(1),
    ])
    .areas(area);

    frame.render_widget(Line::from(question.bold().cyan()), head);
    let under = state
        .selected()
        .and_then(|at| rows.get(at))
        .and_then(|at| nodes[*at].at)
        .map_or("", |at| options[at].detail.as_str());
    frame.render_widget(
        Paragraph::new(table::wrap(under, usize::from(detail.width)).join("\n")).dim(),
        detail,
    );

    let items: Vec<ListItem> = rows
        .iter()
        .map(|at| {
            let node = &nodes[*at];
            let sign = match (node.at.is_some(), open[*at]) {
                (true, _) => "",
                (false, true) => "\u{25be} ",
                (false, false) => "\u{25b8} ",
            };
            ListItem::new(Line::from(vec![
                Span::raw(checkbox(on, nodes, *at)),
                Span::raw("  ".repeat(node.depth)),
                Span::raw(sign),
                Span::raw(&node.label),
            ]))
        })
        .collect();
    let shown = items.len();
    frame.render_stateful_widget(List::new(items).highlight_symbol("> "), body, state);
    // No `filter:` label: the hint says what typing does, and the ten columns
    // it cost were the tail of the hint at the narrowest terminal.
    frame.render_widget(
        Line::from(vec![
            Span::raw(match filter.is_empty() {
                true => String::new(),
                false => format!("{filter}  "),
            }),
            Span::styled(hint(NEST, state, body.height, shown), Style::new().dim()),
        ]),
        foot,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::style::Modifier;
    use ratatui::Terminal;

    /// The serial console case: it comes up 0x0 and stays there, and a
    /// viewport laid out for that draws nothing at all.
    #[test]
    fn a_terminal_that_reports_no_size_is_given_one() {
        assert!(unsized_tty(None));
        assert!(unsized_tty(Some((0, 0))));
        assert!(unsized_tty(Some((80, 0))));
        assert!(!unsized_tty(Some((80, 24))));
    }

    /// The completion screen draws the recovery key. It is generated at install
    /// time, kept out of the log on purpose and written to no file, so this
    /// screen is the only copy there is.
    #[test]
    fn the_completion_screen_draws_the_recovery_key() {
        // 32 random bytes as hex, which is what fisherman's RandomPassphrase
        // hands back for a `tpm2-luks` install.
        const KEY: &str = "6f1b4c0d2a9e83f57b6c1d40e2578a93bb0e4f21c7d68a5039e1b74c2f8d605a";
        let options: Vec<Choice> = vec![
            Choice::new(crate::copy::WRITE_DOWN, "").content(),
            Choice::new(KEY, "").content().tinted(),
            Choice::new(crate::copy::KEY_NOT_LOGGED, "").content(),
            Choice::new(
                crate::copy::logging(Some(std::path::Path::new("/var/log/tect-install.log"))),
                "",
            )
            .content(),
            Choice::new("", ""),
            Choice::new(crate::copy::RESTART, ""),
        ];
        let rows = 4;
        // The rows are read and not answered, and the action is the one thing
        // the cursor can land on.
        for at in 0..rows {
            assert!(!available(&options, at), "row {at} is landable");
            assert!(!options[at].dim, "row {at} is dim");
        }
        assert!(available(&options, options.len() - 1));

        let at = options.len() - 1;
        let mut state = ListState::default().with_selected(Some(at));
        let mut terminal = Terminal::new(TestBackend::new(72, options.len() as u16 + 2)).unwrap();
        terminal
            .draw(|frame| {
                draw(
                    frame,
                    frame.area(),
                    crate::copy::INSTALL_DONE,
                    &options,
                    None,
                    crate::copy::DONE_KEYS,
                    &mut state,
                )
            })
            .unwrap();
        let drawn = terminal.backend().to_string();
        assert!(drawn.contains(KEY), "the key is not on the screen: {drawn}");
        assert!(drawn.contains(crate::copy::KEY_NOT_LOGGED), "{drawn}");
        assert!(drawn.contains("/var/log/tect-install.log"), "{drawn}");
        assert!(drawn.contains(crate::copy::RESTART), "{drawn}");

        // And drawn at full contrast, a character at a time. `unavailable`
        // dims a refused option. The key is the one string on this screen that
        // leaves with the person.
        let buffer = terminal.backend().buffer();
        let row = KEY_ROW;
        let lit = (0..buffer.area.width)
            .map(|column| &buffer[(column, row)])
            .filter(|cell| !cell.symbol().trim().is_empty())
            .collect::<Vec<_>>();
        assert_eq!(
            lit.len(),
            KEY.len(),
            "the key's row is not the key: {drawn}"
        );
        for cell in &lit {
            assert!(
                !cell.modifier.contains(Modifier::DIM),
                "the recovery key is drawn dim: {drawn}"
            );
        }

        // Letters and digits take a tint each, so `0` and `O` are told apart
        // by something on a screen where nothing else can check them.
        let tint = |class: fn(&char) -> bool| {
            KEY.chars()
                .zip(&lit)
                .filter(|(letter, _)| class(letter))
                .map(|(_, cell)| cell.fg)
                .collect::<std::collections::HashSet<_>>()
        };
        let digits = tint(|letter| letter.is_ascii_digit());
        let letters = tint(|letter| letter.is_alphabetic());
        assert_eq!(digits.len(), 1, "digits take one tint: {digits:?}");
        assert_eq!(letters.len(), 1, "letters take one tint: {letters:?}");
        assert!(
            digits.is_disjoint(&letters),
            "letters and digits share a tint: {digits:?} {letters:?}"
        );
    }

    /// The question takes the first row and the key is the second summary row
    /// under it, `WRITE_DOWN` being the first.
    const KEY_ROW: u16 = 2;

    /// The last question before a disk is wiped. It carries what installing
    /// costs — the disk, what happens to it, and every answer about to be acted
    /// on — all shown, none landable, with the cursor opening on the way back.
    #[test]
    fn the_confirmation_shows_the_answers_and_lands_only_on_a_choice() {
        let rows = [
            ("disk".to_string(), "/dev/vda".to_string()),
            ("computer name".to_string(), "deb2".to_string()),
            (
                "password".to_string(),
                crate::copy::PASSWORD_SET.to_string(),
            ),
        ];
        let options = summary_sheet(&rows, crate::copy::CONTINUE, crate::copy::GO_BACK);
        // Every summary row refuses the key that would pick it.
        for at in 0..rows.len() {
            assert!(!available(&options, at), "row {at} is landable");
        }
        assert!(available(&options, options.len() - 2));
        assert!(available(&options, options.len() - 1));

        let mut state = ListState::default().with_selected(Some(options.len() - 2));
        let mut terminal = Terminal::new(TestBackend::new(60, options.len() as u16 + 2)).unwrap();
        let question = crate::copy::erasing("/dev/vda");
        terminal
            .draw(|frame| {
                draw(
                    frame,
                    frame.area(),
                    &question,
                    &options,
                    None,
                    EITHER,
                    &mut state,
                )
            })
            .unwrap();
        let drawn = terminal.backend().to_string();
        assert!(drawn.contains("will be erased. Are you sure?"), "{drawn}");
        assert!(
            drawn.contains("/dev/vda") && drawn.contains("deb2"),
            "{drawn}"
        );
        assert!(drawn.contains(crate::copy::CONTINUE), "{drawn}");
        assert!(drawn.contains(crate::copy::GO_BACK), "{drawn}");
    }

    fn typed_line(prefix: &str, typed: &str, default: Option<&str>) -> String {
        let mut terminal = Terminal::new(TestBackend::new(60, 3)).unwrap();
        terminal
            .draw(|frame| written(frame, frame.area(), "who owns it", prefix, typed, default))
            .unwrap();
        terminal.backend().to_string()
    }

    /// The default stands where the answer will be until one is typed, and the
    /// prefix stands before both.
    #[test]
    fn a_default_is_shown_until_something_is_typed_over_it() {
        let empty = typed_line("github.com/", "", Some("someone"));
        assert!(empty.contains("github.com/someone"), "{empty}");
        assert!(empty.contains(LINE_KEYS), "{empty}");

        let over = typed_line("github.com/", "else", Some("someone"));
        assert!(over.contains("github.com/else"), "{over}");
        assert!(!over.contains("someone"), "{over}");

        // A question with no default leaves the answer's line empty.
        let bare = typed_line("", "", None);
        assert!(bare.contains("who owns it"), "{bare}");
        // The backend quotes each row, so the quotes come off before reading it.
        let answer = bare.lines().nth(1).unwrap().replace('"', "");
        assert_eq!(answer.trim(), "", "{bare}");
    }

    /// The installer's screen as it is drawn: one box, centred, with the form
    /// inside it: two fields answered, two not, and an action under them.
    #[test]
    fn a_command_that_owns_the_screen_draws_one_box_around_the_form() {
        let fields = [
            Field::pick(
                "disk",
                vec![Choice::new("/dev/vda", "64G  QEMU HARDDISK")],
                None,
            ),
            Field::text("computer name", "deb2"),
            Field::secret("password", ""),
        ];
        let visible: Vec<usize> = (0..fields.len()).collect();
        let shown = laid_out(
            &fields,
            &visible,
            0,
            0,
            None,
            false,
            &[crate::copy::INSTALL, crate::copy::SHUT_DOWN],
            Some("still needs a disk, a password"),
        );
        let mut terminal = Terminal::new(TestBackend::new(64, 16)).unwrap();
        terminal
            .draw(|frame| {
                let area = chrome(
                    frame,
                    Some("Tectonic installer"),
                    shown.len() as u16,
                    crate::copy::INSTALL_KEYS,
                );
                // What the widget is given inside a box: the box has the
                // legend, so passing the keys here would draw a second one.
                sheet_of(frame, area, &shown, "")
            })
            .unwrap();
        let drawn = terminal.backend().to_string();
        let rows: Vec<&str> = drawn.lines().collect();
        // The backend quotes each row, so the quotes come off before it is read.
        assert!(rows[0].replace('"', "").trim().is_empty(), "{drawn}");
        let top = rows
            .iter()
            .position(|row| row.contains("Tectonic installer"))
            .unwrap_or_else(|| panic!("{drawn}"));
        // Rounded, which the media's console draws.
        assert!(rows[top].contains('\u{256d}'), "{drawn}");
        // A padding row stands between the border and the first field.
        assert!(rows[top + 2].contains("disk"), "{drawn}");
        assert!(drawn.contains(crate::copy::INSTALL), "{drawn}");
        assert!(drawn.contains(crate::copy::SHUT_DOWN), "{drawn}");
        assert!(drawn.contains("still needs a disk, a password"), "{drawn}");
        // Exactly one legend, on the bottom edge. Two is what a widget drawing
        // its own foot inside a box that already has one looks like.
        assert_eq!(
            drawn.matches(crate::copy::INSTALL_KEYS).count(),
            1,
            "{drawn}"
        );
        let last = rows
            .iter()
            .rposition(|row| row.contains('\u{2570}'))
            .unwrap();
        assert!(rows[last].contains(crate::copy::INSTALL_KEYS), "{drawn}");
    }

    /// The number has to move while a step is running, because one step is most
    /// of an install: `install OS` weighs 87 of 100 and everything before it
    /// comes to 2. Each counted message takes a share of what is left of the
    /// step, and the bar never reaches where the next step begins.
    #[test]
    fn the_bar_climbs_through_a_step_and_stops_short_of_the_next() {
        let seen: Vec<u16> = (0..400).map(|within| crept(2, 87, within)).collect();
        assert_eq!(seen[0], 2);
        // It only ever grows.
        assert!(
            seen.windows(2).all(|pair| pair[1] >= pair[0]),
            "{:?}",
            &seen[..40]
        );
        // It is off the mark within a handful of messages, and past halfway by
        // the time a layered image has copied its layers.
        assert!(seen[10] > 10, "{}", seen[10]);
        assert!(seen[50] > 45, "{}", seen[50]);
        // And it never reaches where the next step begins.
        assert!(seen.iter().all(|at| *at < 89), "{}", seen[399]);
    }

    /// The region the install log stopped being the only copy of: a gauge on
    /// the percentage, the step under it, the last few messages under that,
    /// and the one line that does not move.
    #[test]
    fn the_progress_region_draws_the_step_the_messages_and_the_line_beneath() {
        let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
        let notes = ["first".to_string(), "second".to_string()];
        terminal
            .draw(|frame| {
                working(
                    frame,
                    frame.area(),
                    50,
                    0,
                    "7/12 install OS",
                    &notes,
                    "log: /run/tect-install.log",
                )
            })
            .unwrap();
        let drawn = terminal.backend().to_string();
        assert!(drawn.contains("50%"), "{drawn}");
        assert!(drawn.contains("7/12 install OS"), "{drawn}");
        // The spinner turns beside the step, which is the only thing on this
        // screen that moves when fisherman has gone quiet.
        assert!(drawn.contains(TURNING[0]), "{drawn}");
        assert_eq!(
            TURNING
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            10
        );
        assert!(
            drawn.contains("first") && drawn.contains("second"),
            "{drawn}"
        );
        assert!(drawn.contains("log: /run/tect-install.log"), "{drawn}");
    }

    /// The installer's screen as a form: every question on it at once, the
    /// disk list opened in place under its own row, and the two actions with
    /// what stops the first of them written under it.
    #[test]
    fn a_form_shows_every_field_and_opens_a_list_where_it_stands() {
        let disks = vec![
            Choice::new("/dev/sda", "512G  Samsung SSD"),
            Choice::new("/dev/sdb", "1T  WD  removable"),
        ];
        let fields = [
            Field::pick("disk", disks, None),
            Field::text("computer name", "debian-bootc"),
            Field::text("username", ""),
            Field::secret("password", "hunter2"),
            Field::secret("confirm", "hunter2"),
            Field::pick(
                "encryption",
                vec![Choice::new("none", "not encrypted")],
                Some(0),
            ),
        ];
        let visible: Vec<usize> = (0..fields.len()).collect();
        let shown = laid_out(
            &fields,
            &visible,
            0,
            0,
            Some(1),
            false,
            &["Install", "Shut down"],
            Some("still needs a disk, a username"),
        );
        let mut terminal = Terminal::new(TestBackend::new(64, shown.len() as u16 + 1)).unwrap();
        terminal
            .draw(|frame| sheet_of(frame, frame.area(), &shown, crate::copy::INSTALL_KEYS))
            .unwrap();
        let drawn = terminal.backend().to_string();
        eprintln!("SHOT\n{drawn}SHOT");
        // The list is open under its own row, not on a screen of its own.
        let rows: Vec<&str> = drawn.lines().collect();
        assert!(rows[0].contains("> disk"), "{drawn}");
        assert!(rows[1].contains("/dev/sda"), "{drawn}");
        assert!(rows[2].contains("> /dev/sdb"), "{drawn}");
        assert!(rows[3].contains("computer name"), "{drawn}");
        // A secret reads as its length and never as itself.
        assert!(
            drawn.contains("*******") && !drawn.contains("hunter2"),
            "{drawn}"
        );
        // An unanswered field says so, and the blocked action says why.
        assert!(drawn.contains("not set"), "{drawn}");
        assert!(
            drawn.contains("Install") && drawn.contains("Shut down"),
            "{drawn}"
        );
        assert!(drawn.contains("still needs a disk, a username"), "{drawn}");
    }

    #[test]
    fn columns_width_must_be_nonzero_and_fit_the_renderer() {
        assert_eq!(parse_width("0"), None);
        assert_eq!(parse_width("65535"), Some(u16::MAX));
        assert_eq!(parse_width("65536"), None);
    }

    fn drawn(options: &[Choice], on: Option<&[usize]>, hint: &str, at: usize) -> String {
        let mut state = ListState::default().with_selected(Some(at));
        let height = options.len() as u16 + 3;
        let mut terminal = Terminal::new(TestBackend::new(60, height)).unwrap();
        terminal
            .draw(|frame| {
                draw(
                    frame,
                    frame.area(),
                    "which module",
                    options,
                    on,
                    hint,
                    &mut state,
                )
            })
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

    /// The narrowest terminal the tool draws for. A hint cut in half is the
    /// same defect the questions had.
    #[test]
    fn every_hint_fits_the_narrowest_terminal() {
        let mut state = ListState::default().with_selected(Some(0));
        for hint in [PICK, TOGGLE, EITHER] {
            let mut terminal = Terminal::new(TestBackend::new(60, 4)).unwrap();
            terminal
                .draw(|frame| {
                    draw(
                        frame,
                        frame.area(),
                        "which module",
                        &[Choice::new("gaming", "")],
                        None,
                        hint,
                        &mut state,
                    )
                })
                .unwrap();
            let drawn = terminal.backend().to_string();
            assert!(drawn.contains(hint), "{drawn}");
        }
        let drawn = nest_drawn_at(60, &rules(), &[true; 7], &[], "", 0);
        assert!(drawn.contains(NEST), "{drawn}");
    }

    #[test]
    fn a_list_longer_than_the_window_says_how_far_down_it_is() {
        let short = ListState::default();
        assert_eq!(hint(PICK, &short, 8, 3), PICK);
        assert_eq!(hint(PICK, &short, 8, 21), format!("{PICK}  8 of 21"));
        let scrolled = ListState::default().with_offset(13);
        assert_eq!(hint(PICK, &scrolled, 8, 21), format!("{PICK}  21 of 21"));
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

    fn rules() -> Vec<Choice> {
        vec![
            Choice::new("1.1.1 tmp", "a separate partition").within("1.1"),
            Choice::new("1.1.2 nodev", "no device files there").within("1.1"),
            Choice::new("1.2.1 gpgcheck", "signatures are checked").within("1.2"),
            Choice::new("RHEL-09-232010", "numbered by nothing"),
        ]
    }

    fn shape(nodes: &[Node]) -> Vec<(&str, usize, Option<usize>)> {
        nodes
            .iter()
            .map(|node| (node.label.as_str(), node.depth, node.at))
            .collect()
    }

    fn nest_drawn(
        options: &[Choice],
        open: &[bool],
        on: &[usize],
        filter: &str,
        at: usize,
    ) -> String {
        nest_drawn_at(60, options, open, on, filter, at)
    }

    fn nest_drawn_at(
        width: u16,
        options: &[Choice],
        open: &[bool],
        on: &[usize],
        filter: &str,
        at: usize,
    ) -> String {
        let nodes = nodes(options);
        let rows = shown(&nodes, options, open, filter);
        let mut state = ListState::default().with_selected(Some(at));
        let height = rows.len() as u16 + 5;
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                nested(
                    frame,
                    frame.area(),
                    "which rules",
                    options,
                    &nodes,
                    &rows,
                    open,
                    on,
                    filter,
                    &mut state,
                )
            })
            .unwrap();
        terminal.backend().to_string()
    }

    #[test]
    fn a_branch_is_drawn_for_every_dotted_part_and_a_row_with_no_group_stays_at_the_top() {
        assert_eq!(
            shape(&nodes(&rules())),
            vec![
                ("1", 0, None),
                ("1.1", 1, None),
                ("1.1.1 tmp", 2, Some(0)),
                ("1.1.2 nodev", 2, Some(1)),
                ("1.2", 1, None),
                ("1.2.1 gpgcheck", 2, Some(2)),
                ("RHEL-09-232010", 0, Some(3)),
            ]
        );
    }

    #[test]
    fn a_branch_turns_on_everything_under_it_and_off_again() {
        let nodes = nodes(&rules());
        let mut on = Vec::new();
        check(&mut on, &nodes, 0);
        assert_eq!(on, vec![0, 1, 2]);
        check(&mut on, &nodes, 0);
        assert!(on.is_empty());
    }

    #[test]
    fn a_branch_holding_some_of_what_is_on_is_neither_on_nor_off() {
        let nodes = nodes(&rules());
        let mut on = Vec::new();
        check(&mut on, &nodes, 2);
        assert_eq!(checkbox(&on, &nodes, 1), "[-] ");
        check(&mut on, &nodes, 3);
        assert_eq!(checkbox(&on, &nodes, 1), "[x] ");
        assert_eq!(checkbox(&on, &nodes, 4), "[ ] ");
    }

    #[test]
    fn a_closed_branch_hides_what_it_contains_and_an_open_one_shows_it() {
        let options = rules();
        let nodes = nodes(&options);
        let mut open = vec![false; nodes.len()];
        assert_eq!(shown(&nodes, &options, &open, ""), vec![0, 6]);
        open[0] = true;
        assert_eq!(shown(&nodes, &options, &open, ""), vec![0, 1, 4, 6]);
        open[1] = true;
        assert_eq!(shown(&nodes, &options, &open, ""), vec![0, 1, 2, 3, 4, 6]);
    }

    #[test]
    fn a_filtered_answer_names_the_option_chosen_and_not_the_row_it_was_on() {
        let options = rules();
        let nodes = nodes(&options);
        let open = vec![false; nodes.len()];
        let rows = shown(&nodes, &options, &open, "signatures");
        assert_eq!(rows, vec![5]);
        let mut on = Vec::new();
        check(&mut on, &nodes, rows[0]);
        assert_eq!(on, vec![2]);
        assert_eq!(options[on[0]].label, "1.2.1 gpgcheck");
    }

    #[test]
    fn the_detail_of_the_highlighted_row_is_drawn_under_the_tree() {
        let options = rules();
        let open = vec![true; nodes(&options).len()];
        let drawn = nest_drawn(&options, &open, &[1], "", 3);
        assert!(drawn.contains("> [x]     1.1.2 nodev"), "{drawn}");
        assert!(drawn.contains("[-]   \u{25be} 1.1"), "{drawn}");
        assert!(drawn.contains("no device files there"), "{drawn}");
        assert!(drawn.contains(NEST), "{drawn}");
    }
}
