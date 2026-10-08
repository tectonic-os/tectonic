use common::ui::testing::{Screen, Snapshot};
use std::path::Path;

pub(crate) const WIDTH: u16 = 80;
pub(crate) const HEIGHT: u16 = 32;

pub(crate) fn screen() -> Screen {
    Screen::inline(WIDTH, HEIGHT)
}

pub(crate) fn assert_screen(name: &str, snapshot: Snapshot) -> String {
    let rendered = snapshot.to_string();
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots"));
    settings.set_prepend_module_to_snapshot(false);
    settings.set_omit_expression(true);
    settings.bind(|| insta::assert_snapshot!(format!("tect-screen__{name}"), &rendered));
    rendered
}

pub(crate) fn assert_style_at(rendered: &str, x: u16, y: u16, style: &str, what: &str) {
    let coordinate = format!("x: {x}, y: {y},");
    let line = rendered
        .lines()
        .find(|line| line.contains(&coordinate))
        .unwrap_or_else(|| panic!("{what} has no recorded style at {x},{y}"));
    assert!(line.contains(style), "{what} has the wrong style: {line}");
}
