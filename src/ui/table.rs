//! A table drawn once at the terminal's width, for a command that prints and
//! exits. Nothing here scrolls or holds state: the terminal already scrolls,
//! which is what keeps `tect graph | less` and `| grep` working.

use ratatui::backend::IntoCrossterm;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Text;
use ratatui::widgets::{Block, Cell, Row, Table, Widget};
use std::fmt::Write as _;

/// One table as ANSI lines: `rows` are cells in `header`'s order, each with
/// whether what the row says is a defect, which is the only thing colour says.
pub fn render(title: &str, header: &[&str], rows: &[(Vec<String>, bool)]) -> String {
    draw(super::width() as u16, title, header, rows)
}

fn draw(width: u16, title: &str, header: &[&str], rows: &[(Vec<String>, bool)]) -> String {
    let widths = columns(header, rows, width.saturating_sub(2));
    // Only as wide as what is in it: a frame stretched to the terminal is the
    // padding this rendering exists to stop.
    let frame = (widths.iter().sum::<u16>() + widths.len() as u16 + 1)
        .max(title.chars().count() as u16 + 2)
        .min(width);

    let head = fold(&widths, header.iter().map(|name| (*name).to_string()));
    let titles = head.iter().map(Vec::len).max().unwrap_or(1) as u16;
    let mut height = 2 + titles;
    let body: Vec<Row> = rows
        .iter()
        .map(|(cells, defect)| {
            let folded = fold(&widths, cells.iter().cloned());
            let lines = folded.iter().map(Vec::len).max().unwrap_or(1) as u16;
            height += lines;
            let row = cells_of(folded).height(lines);
            match defect {
                true => row.red(),
                false => row,
            }
        })
        .collect();

    let area = Rect::new(0, 0, frame, height);
    let mut buffer = Buffer::empty(area);
    Table::new(body, widths.iter().map(|room| Constraint::Length(*room)))
        .header(cells_of(head).height(titles).bold())
        .block(Block::bordered().title(title.bold()))
        .render(area, &mut buffer);
    ansi(&buffer)
}

/// Each cell folded to the column it sits in.
fn fold(widths: &[u16], cells: impl Iterator<Item = String>) -> Vec<Vec<String>> {
    cells
        .zip(widths)
        .map(|(text, room)| wrap(&text, *room as usize))
        .collect()
}

fn cells_of(folded: Vec<Vec<String>>) -> Row<'static> {
    Row::new(
        folded
            .into_iter()
            .map(|lines| Cell::from(Text::from(lines.join("\n")))),
    )
}

/// What each column gets of `room`: its own content where that fits, and
/// whatever the constraint solver leaves it where it does not.
fn columns(header: &[&str], rows: &[(Vec<String>, bool)], room: u16) -> Vec<u16> {
    let natural = header.iter().enumerate().map(|(at, name)| {
        let cells = rows.iter().map(|(cells, _)| cells[at].chars().count());
        Constraint::Max(cells.max().unwrap_or(0).max(name.chars().count()) as u16)
    });
    Layout::horizontal(natural.collect::<Vec<_>>())
        .spacing(1)
        .split(Rect::new(0, 0, room, 1))
        .iter()
        .map(|area| area.width)
        .collect()
}

/// The narrowest terminal that folds no word mid-way: each column as wide as
/// the longest single word in it, plus the gaps between columns and the frame.
/// Below this a table is lossless but unreadable, and the caller falls back.
pub(crate) fn floor(header: &[&str], rows: &[(Vec<String>, bool)]) -> usize {
    let words: usize = header
        .iter()
        .enumerate()
        .map(|(at, name)| {
            rows.iter()
                .map(|(cells, _)| longest_word(&cells[at]))
                .chain(std::iter::once(longest_word(name)))
                .max()
                .unwrap_or(0)
        })
        .sum();
    words + header.len().saturating_sub(1) + 2
}

fn longest_word(text: &str) -> usize {
    text.split_whitespace()
        .map(|word| word.chars().count())
        .max()
        .unwrap_or(0)
}

/// `text` folded to `room` at spaces, and mid-word where one word is wider
/// than the column: a cell clips what it cannot draw, and a clipped name is a
/// wrong answer rather than a short one.
pub(crate) fn wrap(text: &str, room: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in text.split_whitespace() {
        for part in pieces(word, room.max(1)) {
            let line = lines.last_mut().expect("one line to start with");
            match line.is_empty() {
                true => line.push_str(part),
                false if line.chars().count() + 1 + part.chars().count() <= room => {
                    line.push(' ');
                    line.push_str(part);
                }
                false => lines.push(part.to_string()),
            }
        }
    }
    lines
}

/// `word` in pieces no wider than `room`, and whole where it already is.
fn pieces(word: &str, room: usize) -> Vec<&str> {
    let mut cuts: Vec<usize> = word
        .char_indices()
        .step_by(room)
        .map(|(at, _)| at)
        .collect();
    cuts.push(word.len());
    cuts.windows(2).map(|cut| &word[cut[0]..cut[1]]).collect()
}

/// The buffer as lines, with one escape sequence per run of cells sharing a
/// style and none at all around the cells carrying no style.
fn ansi(buffer: &Buffer) -> String {
    let mut out = String::new();
    for y in 0..buffer.area.height {
        let mut run = String::new();
        let mut style = Style::new();
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            let here = style_of(cell);
            if here != style {
                push(&mut out, &run, style);
                run.clear();
                style = here;
            }
            run.push_str(cell.symbol());
        }
        push(&mut out, &run, style);
        out.push('\n');
    }
    out
}

/// A cell's style with what it does not set left unset, so an unstyled run
/// carries no escape at all rather than three resets.
fn style_of(cell: &ratatui::buffer::Cell) -> Style {
    let mut style = Style::new().add_modifier(cell.modifier);
    if cell.fg != Color::Reset {
        style = style.fg(cell.fg);
    }
    if cell.bg != Color::Reset {
        style = style.bg(cell.bg);
    }
    style
}

fn push(out: &mut String, run: &str, style: Style) {
    match style == Style::new() {
        true => out.push_str(run),
        false => {
            let _ = write!(out, "{}", style.into_crossterm().apply(run));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: [&str; 2] = ["Name", "Provided by"];

    fn rows() -> Vec<(Vec<String>, bool)> {
        vec![
            (vec!["shell-config".into(), "core/shell".into()], false),
            (vec!["nothing-provides".into(), String::new()], true),
        ]
    }

    #[test]
    fn every_cell_is_drawn_inside_one_frame_at_the_width_asked_for() {
        let drawn = draw(40, "Capabilities", &HEADER, &rows());
        let lines: Vec<&str> = drawn.lines().collect();
        // Frame, header, and one line per row.
        assert_eq!(lines.len(), 5, "{drawn}");
        // As wide as the two columns and no wider.
        assert_eq!(lines[4].chars().count(), 16 + 11 + 3, "{drawn}");
        assert!(lines[0].starts_with('\u{250c}'), "{drawn}");
        assert!(lines[4].starts_with('\u{2514}'), "{drawn}");
        assert!(drawn.contains("shell-config"), "{drawn}");
        assert!(drawn.contains("core/shell"), "{drawn}");
    }

    #[test]
    fn a_defect_row_is_the_only_coloured_one() {
        let drawn = draw(40, "Capabilities", &HEADER, &rows());
        let coloured: Vec<&str> = drawn
            .lines()
            .filter(|line| line.contains("\u{1b}[38;5;"))
            .collect();
        assert_eq!(coloured.len(), 1, "{drawn}");
        assert!(coloured[0].contains("nothing-provides"), "{drawn}");
    }

    #[test]
    fn a_long_cell_folds_at_a_space_and_the_row_grows_to_hold_it() {
        assert_eq!(wrap("a b c", 3), ["a b", "c"]);
        assert_eq!(wrap("a b c", 80), ["a b c"]);
        assert_eq!(wrap("", 10), [""]);
        assert_eq!(wrap("unbreakable", 4), ["unbr", "eaka", "ble"]);
    }

    #[test]
    fn a_column_is_never_wider_than_the_longest_thing_in_it() {
        let widths = columns(&HEADER, &rows(), 78);
        assert_eq!(widths, ["nothing-provides".len() as u16, 11]);
    }

    #[test]
    fn the_floor_is_each_column_at_its_longest_word_plus_gaps_and_frame() {
        assert_eq!(floor(&HEADER, &rows()), 16 + 10 + 1 + 2);
        assert_eq!(
            floor(&["Hash"], &[(vec!["a b".into(), "cd".into()], false)]),
            4 + 0 + 2
        );
        assert_eq!(floor(&[], &[]), 2);
    }
}
