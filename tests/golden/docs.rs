//! The generated documents: the schema pages, the CLI reference and the
//! read-back round trip.

use crate::harness::{crate_dir, normalize_snapshot_body, snapshot_body, walk};

use std::fmt::Write as _;

use std::path::Path;

#[test]
fn schema_doc() {
    use tect::emit::schema_md::{areas, page};
    let pages = areas()
        .into_iter()
        .map(|area| (format!("docs/schema/{}", area.file()), page(area)));
    for (path, rendered) in pages {
        let at = crate_dir().join(&path);
        let doc = std::fs::read_to_string(&at).unwrap_or_else(|err| panic!("{path}: {err}"));
        assert!(doc == rendered, "{path} is stale");
    }

    let mut on_disk: Vec<String> = std::fs::read_dir(crate_dir().join("docs/schema"))
        .expect("docs/schema/ exists")
        .map(|entry| {
            entry
                .expect("docs/schema/ lists")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    on_disk.sort();
    let mut mapped: Vec<String> = areas().iter().map(|area| area.file().to_string()).collect();
    mapped.sort();
    assert_eq!(
        on_disk, mapped,
        "docs/schema/ holds exactly the pages the schema map names"
    );
}

/// A recorded run whose tree shows what one command writes, for the CLI
/// reference. `file` names a file that the run wrote and that the fixture keeps
/// beside the transcript, which the reference also shows.
struct Written {
    words: &'static str,
    flow: &'static str,
    file: Option<&'static str>,
}

const WRITTEN: &[Written] = &[
    Written {
        words: "create repo",
        flow: "flow-create-repo",
        file: None,
    },
    Written {
        words: "create image",
        flow: "flow-create-image",
        file: None,
    },
    Written {
        words: "create flavour",
        flow: "flow-create-flavour",
        file: Some("example.image.kdl"),
    },
    Written {
        words: "create module",
        flow: "flow-create-module",
        file: None,
    },
    Written {
        words: "import module",
        flow: "flow-import-module",
        file: None,
    },
    Written {
        words: "copy module",
        flow: "flow-copy-module",
        file: None,
    },
    Written {
        words: "set workflows",
        flow: "flow-set-workflows",
        file: None,
    },
    Written {
        words: "set conforms",
        flow: "flow-set-conforms",
        file: None,
    },
    Written {
        words: "set claims",
        flow: "flow-set-claims",
        file: None,
    },
];

/// The tree that a recorded run draws after it writes files, then the file
/// that `written` names. The tree starts on a directory line after a blank
/// line and runs to the next blank line.
fn written(written: &Written) -> String {
    let transcript = snapshot_body(written.flow, "transcript.txt");
    let mut tree = String::new();
    let mut previous = "";
    for line in transcript.lines() {
        let opens = previous.is_empty() && line.ends_with('/') && !line.starts_with(' ');
        if tree.is_empty() && !opens {
            previous = line;
            continue;
        }
        if line.is_empty() || line.starts_with("====") {
            break;
        }
        tree.push_str(line);
        tree.push('\n');
    }
    assert!(
        !tree.is_empty(),
        "{} draws no tree of what it wrote",
        written.flow
    );
    let mut out = format!("**What it writes:**\n\n```text\n{tree}```\n\n");
    if let Some(file) = written.file {
        let text = snapshot_body(written.flow, file);
        let _ = write!(out, "`{file}` after it runs:\n\n```kdl\n{text}\n```\n\n");
    }
    out
}

/// A flag or argument line of `clap-markdown`, as a table row. The table
/// replaces the dash that `clap-markdown` writes between a name and its help.
fn row(line: &str) -> Option<String> {
    let entry = line.strip_prefix("* ")?;
    let (name, help) = entry.split_once(" \u{2014} ").unwrap_or((entry, ""));
    Some(format!("| {name} | {help} |"))
}

/// One command's section of the reference, in the order the reader needs it:
/// what it does and the questions it asks, its usage and flags, its schema,
/// what it writes, then its notes.
fn reference_section(section: &str, command: &clap::Command, words: &str) -> String {
    let notes = command
        .get_after_long_help()
        .map(|help| help.to_string())
        .unwrap_or_default();
    let mut body = String::new();
    let mut listing = false;
    for line in section.lines() {
        if line == "###### **Subcommands:**" {
            listing = true;
            continue;
        }
        if listing {
            if line.is_empty() || line.starts_with("* `") {
                continue;
            }
            listing = false;
            // A label right after a list would join the last list item.
            body.push('\n');
        }
        match line {
            // A sixth-level heading puts every label into the page contents.
            "###### **Arguments:**" => body.push_str("| Argument | Description |\n| --- | --- |"),
            "###### **Options:**" => body.push_str("| Option | Description |\n| --- | --- |"),
            // The table header stands where the blank line under the label was.
            "" if body.ends_with("| --- | --- |\n") => continue,
            _ => body.push_str(&row(line).unwrap_or_else(|| line.to_string())),
        }
        body.push('\n');
    }
    let mut out = match notes.is_empty() {
        true => body,
        false => body.replacen(&format!("{notes}\n\n"), "", 1),
    };
    let trimmed = out.trim_end().len();
    out.truncate(trimmed);
    out.push_str("\n\n");
    if let Some((_, link)) = tect::command::schema_links()
        .into_iter()
        .find(|(path, _)| path == words)
    {
        let _ = writeln!(out, "**Schema:** {link}\n");
    }
    if let Some(found) = WRITTEN.iter().find(|found| found.words == words) {
        out.push_str(&written(found));
    }
    let trimmed = out.trim_end().len();
    out.truncate(trimmed);
    if !notes.is_empty() {
        let _ = write!(out, "\n\n{notes}");
    }
    out.push_str("\n\n");
    out
}

/// The reference in docs/cli.md, rendered from the clap tree the parser
/// reads, so the reference and the parser cannot disagree.
#[test]
fn commands_doc() {
    use clap::CommandFactory;
    let path = crate_dir().join("docs/cli.md");
    let options = clap_markdown::MarkdownOptions::new()
        .show_table_of_contents(false)
        .show_footer(false);
    let tree = tect::command::Cli::command();
    let detail = clap_markdown::help_markdown_command_custom(&tree, &options);
    // The overview owns the title, so the reference starts at its first command.
    let first = detail
        .find("\n## `")
        .expect("the reference holds a command");
    let mut reference = String::new();
    for section in detail[first + 1..].split("\n## `") {
        let section = section.strip_prefix("## `").unwrap_or(section);
        let heading = section.split('`').next().unwrap_or_default();
        let words = heading
            .strip_prefix("tect")
            .unwrap_or_default()
            .trim_start();
        let command = words
            .split_whitespace()
            .try_fold(&tree, |command, word| command.find_subcommand(word))
            .expect("each reference heading names a command");
        // The overview tables list every command, so a group with no help of
        // its own repeats them alone.
        if command.get_subcommands().next().is_some() && command.get_long_about().is_none() {
            continue;
        }
        reference.push_str(&reference_section(
            &format!("## `{section}"),
            command,
            words,
        ));
    }
    let rendered = format!("{}{}\n", tect::command::overview(), reference.trim_end());
    let doc = std::fs::read_to_string(&path).expect("docs/cli.md exists");
    assert!(doc == rendered, "docs/cli.md is stale");
}

/// Every document the tool writes has to read back as what was written. The
/// corpus is the oracle, so the whole of it is the round trip.
#[test]
fn every_written_document_reads_back() {
    let dir = crate_dir().join("tests/snapshots");
    let mut read = 0;
    for path in walk(&dir) {
        let file = path
            .file_stem()
            .map(Path::new)
            .and_then(Path::extension)
            .and_then(|extension| extension.to_str());
        if file != Some("json") {
            continue;
        }
        let text = insta::Snapshot::from_file(&path)
            .unwrap_or_else(|err| panic!("{}: {err}", path.display()))
            .as_text()
            .expect("the golden is a text snapshot")
            .to_string();
        // A command with nothing to say writes nothing, which is not a document.
        if text.trim().is_empty() {
            continue;
        }
        let parsed = common::json::Json::parse(&text)
            .unwrap_or_else(|err| panic!("{}: {err}", path.display()));
        let rendered = normalize_snapshot_body(&parsed.render());
        assert!(
            rendered == text,
            "{} did not read back as what was written",
            path.display()
        );
        read += 1;
    }
    assert!(read >= 20, "only {read} documents were read");
}
