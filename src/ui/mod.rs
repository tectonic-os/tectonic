//! The only place ratatui appears. Widgets draw into a bounded region of the
//! normal scroll, never an alternate screen, and only where the caller has
//! already found a terminal to draw on.

pub mod table;
pub mod tree;

use ratatui::backend::Backend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::crossterm::terminal;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};
use ratatui::{DefaultTerminal, Frame, TerminalOptions, Viewport};
use std::io::IsTerminal;

/// What a terminal that will not say how wide it is is taken to be.
const NARROWEST: usize = 80;

/// Whether anything drawn is being watched, which is what decides both colour
/// and whether a read-out is a table or the markdown a file would hold.
pub fn colour() -> bool {
    std::io::stdout().is_terminal()
}

/// Asked of the terminal only where the output is one, so a redirected run and
/// a piped one draw the same thing whatever is behind them.
pub fn width() -> usize {
    match colour() {
        true => terminal::size().map_or(NARROWEST, |(cols, _)| cols as usize),
        false => NARROWEST,
    }
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
    /// The dotted group this one sits inside, which *contains* it rather than
    /// contradicting it, and which a question taking several answers draws as
    /// a collapsed tree. Nothing reads it and `parent` both.
    pub group: String,
}

impl Choice {
    pub fn new(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: detail.into(),
            parent: None,
            group: String::new(),
        }
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

const PICK: &str = "up and down to move, enter to choose, esc cancels";
const TOGGLE: &str =
    "up and down to move, enter or space to toggle, enter on done to confirm, esc cancels";
const EITHER: &str = "up and down to move, enter to answer";
/// No `j` and `k` here: every printable key is the filter being typed.
const NEST: &str = "type to filter, space toggles, enter opens, esc backs out";

/// The row past the last option, and the only way to submit.
const DONE: &str = "done";

/// One of `options`, or none.
pub fn select(question: &str, options: &[Choice]) -> Result<Option<usize>, String> {
    inline(height(options.len()), |terminal| {
        pick(terminal, question, options, PICK, 0)
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
/// collapsed tree with a filter, since a few hundred rows are neither readable
/// nor reachable as a flat list.
pub fn multi(question: &str, options: &[Choice], on: &[usize]) -> Result<Answer, String> {
    if options.iter().any(|choice| !choice.group.is_empty()) {
        let rows = nodes(options).len() + 1;
        return inline(height(rows) + 1, |terminal| {
            nest(terminal, question, options, on)
        });
    }
    inline(height(options.len() + 1), |terminal| {
        toggle(terminal, question, options, on)
    })
}

/// The lines a question takes: its rows, whatever they are, under the question
/// and over the hint.
fn height(rows: usize) -> u16 {
    (rows.min(VISIBLE) + 2) as u16
}

/// A bounded region of the normal scroll, cleared again before this returns, so
/// what the caller prints afterwards lands where the region was.
fn inline<T>(
    height: u16,
    body: impl FnOnce(&mut DefaultTerminal) -> Result<T, String>,
) -> Result<T, String> {
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
    selected: usize,
) -> Result<Option<usize>, String> {
    let mut state = ListState::default().with_selected(Some(selected));
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

/// Containment, not the contradiction `Choice::parent` describes: a branch
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
        let done = rows.len();
        if state.selected().unwrap_or(0) > done {
            state.select(Some(done));
        }
        terminal
            .draw(|frame| {
                nested(
                    frame, question, options, &tree, &rows, &open, &on, &filter, &mut state,
                )
            })
            .map_err(|err| err.to_string())?;
        let Some(key) = read()? else { continue };
        let row = state.selected().filter(|at| *at < done).map(|at| rows[at]);
        match key {
            KeyCode::Enter if state.selected() == Some(done) => {
                on.sort_unstable();
                return Ok(Answer::Chosen(on));
            }
            KeyCode::Esc if !filter.is_empty() => filter.clear(),
            KeyCode::Esc => return Ok(Answer::Cancelled),
            KeyCode::Backspace => {
                filter.pop();
            }
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right => {
                if let Some(at) = row {
                    match (tree[at].at.is_some(), key) {
                        (true, KeyCode::Left | KeyCode::Right) => {}
                        (true, _) => check(&mut on, &tree, at),
                        (false, KeyCode::Char(' ')) => check(&mut on, &tree, at),
                        (false, KeyCode::Left) => open[at] = false,
                        (false, KeyCode::Right) => open[at] = true,
                        (false, _) => open[at] = !open[at],
                    }
                }
            }
            KeyCode::Char(letter) => filter.push(letter),
            code => move_by(code, &mut state),
        }
    }
}

/// The rows, the detail of whatever is highlighted, and the filter as it is
/// typed. A rule's description is prose and belongs under the list, not beside
/// a label.
#[allow(clippy::too_many_arguments)]
fn nested(
    frame: &mut Frame,
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
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    frame.render_widget(Line::from(question.bold().cyan()), head);
    let under = state
        .selected()
        .and_then(|at| rows.get(at))
        .and_then(|at| nodes[*at].at)
        .map_or("", |at| options[at].detail.as_str());
    frame.render_widget(Line::from(under.dim()), detail);
    frame.render_widget(
        Line::from(vec![
            Span::raw(format!("filter: {filter}  ")),
            Span::styled(NEST, Style::new().dim()),
        ]),
        foot,
    );

    let mut items: Vec<ListItem> = rows
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
    items.push(ListItem::new(Line::from(DONE.bold())));
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
        let nodes = nodes(options);
        let rows = shown(&nodes, options, open, filter);
        let mut state = ListState::default().with_selected(Some(at));
        let height = rows.len() as u16 + 4;
        let mut terminal = Terminal::new(TestBackend::new(60, height)).unwrap();
        terminal
            .draw(|frame| {
                nested(
                    frame,
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
        assert!(drawn.contains("filter:   type to filter"), "{drawn}");
    }
}
