use super::*;
use crate::parse::schema::{check_text, Say};

/// The walker only reads what it is given, so the minimal example and the
/// whole example each have to pass it for every example value to hold. An
/// empty file passes the walker, so an empty example is refused first.
#[test]
fn every_example_on_the_kdl_page_is_a_valid_repo_file() {
    let page = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/kdl.md"),
    )
    .expect("docs/kdl.md exists");
    let blocks: Vec<&str> = page
        .split("```kdl\n")
        .skip(1)
        .map(|rest| rest.split("```").next().unwrap_or_default())
        .collect();
    assert!(!blocks.is_empty(), "docs/kdl.md holds no kdl example");
    for text in blocks {
        let issues = check_text(text, &repo::REPO, false)
            .unwrap_or_else(|err| panic!("docs/kdl.md: {err}\n{text}"));
        assert!(
            issues.is_empty(),
            "docs/kdl.md:\n{}\n{text}",
            issues.plain()
        );
    }
}

#[test]
fn every_example_passes_the_grammar() {
    for s in SECTIONS {
        for show in [Show::Minimal, Show::Scaffold, Show::All] {
            let text = file(s, show);
            if text.trim().is_empty() {
                assert!(
                    show == Show::Minimal
                        && !s.declared
                        && s.node.children.iter().all(|node| !is_minimal(node)),
                    "{} has an empty example despite requiring a node",
                    s.name
                );
            }
            let issues = check_text(&text, s.node, s.declared)
                .unwrap_or_else(|err| panic!("{}: {err}\n{text}", s.name));
            assert!(issues.is_empty(), "{}:\n{}\n{text}", s.name, issues.plain());
        }
    }
}

#[test]
fn every_schema_has_a_page() {
    for s in SECTIONS {
        assert!(AREAS.contains(&s.area), "{} has no page", s.name);
    }
}

#[test]
fn the_page_opens_on_the_file_and_its_scaffold() {
    let out = page(Area::Image);
    assert!(
        out.starts_with("<a id=\"image.kdl\"></a>\n\n# `image.kdl`\n"),
        "{out}"
    );
    let start = out.find("```kdl\n").expect("the page has an example");
    let end = start + 7 + out[start + 7..].find("```").expect("the example closes");
    let top = &out[start..end];
    assert!(top.contains("    layout {\n        bootloader"), "{out}");
    assert!(top.contains("    name \"Workstation\"\n"), "{out}");
    assert!(!top.contains("    id "), "{out}");
    assert!(
        out[end..].contains("| [`image`](#image) (required,&nbsp;repeatable) |"),
        "{out}"
    );
}

/// `pin` sits in a collection, which the scaffold writes, and in a module
/// entry, which it does not. A flag on the shared node would put a pin in
/// every module entry of the image example.
#[test]
fn a_shared_node_is_scaffolded_only_where_its_block_names_it() {
    let image = file(&SECTIONS[1], Show::Scaffold);
    assert!(!image.contains("pin"), "{image}");
    let repo = file(&SECTIONS[0], Show::Scaffold);
    assert!(repo.contains("pin {"), "{repo}");
}

#[test]
fn a_block_scaffolds_only_its_own_children() {
    fn walk(node: &Node) {
        for name in node.scaffolds {
            assert!(
                node.children.iter().any(|child| child.name == *name),
                "`{}` scaffolds `{name}`, which it does not hold",
                node.name
            );
        }
        node.children.iter().for_each(walk);
    }
    SECTIONS.iter().for_each(|s| walk(s.node));
}

#[test]
fn a_table_lists_one_level_and_links_each_row() {
    let out = page(Area::Image);
    let table = out
        .find("| [`layout`](#image-layout) (optional) |")
        .expect("the image table");
    let section = out
        .find("### `layout` (optional)\n")
        .expect("the layout section");
    assert!(table < section, "{out}");
    assert!(!out[table..section].contains("`filesystem`"), "{out}");
    assert!(
        out.contains("| [`filesystem`](#image-layout-filesystem) (optional) |"),
        "{out}"
    );
    assert!(out.contains("#### `filesystem` (optional)"), "{out}");
}

#[test]
fn a_required_node_has_its_own_example() {
    let out = page(Area::Image);
    let at = out
        .find("### `name` (required)\n")
        .expect("the name section");
    assert!(
        out[at..].starts_with("### `name` (required)\n\nos-release `NAME`."),
        "{out}"
    );
    let next = at + out[at..].find("```kdl\n").expect("an example follows");
    assert!(
        out[next..].starts_with("```kdl\nimage {\n    name \"Workstation\"\n}"),
        "{out}"
    );
}

#[test]
fn a_nested_section_shows_the_blocks_that_hold_it() {
    let out = page(Area::Repo);
    assert!(
        out.contains("```kdl\nsecurity-policy {\n    network \"strict\"\n}\n```"),
        "{out}"
    );
}

/// A reader expects a field on the page of the file that holds it, so a
/// block that three files share is documented on each of their pages.
#[test]
fn a_shared_block_is_documented_on_each_page_that_holds_it() {
    assert!(page(Area::Module).contains("<a id=\"asset-pin\"></a>"));
    assert!(page(Area::Repo).contains("<a id=\"sources-name-pin\"></a>"));
    assert!(page(Area::Image).contains("<a id=\"image-modules-module-pin\"></a>"));
}

#[test]
fn a_node_already_documented_links_back() {
    let out = page(Area::Module);
    let gate = out
        .find("## `family` (optional, repeatable)")
        .expect("the family section");
    assert!(
        out[gate..].contains("| [`packages`](#packages) (optional,&nbsp;repeatable) |"),
        "{out}"
    );
    assert_eq!(out.matches("<a id=\"packages\"></a>").count(), 1, "{out}");
}

#[test]
fn notes_follow_their_node_before_the_nodes_inside_it() {
    static NOTED: Node = Node::new("noted", "A block with a note.")
        .children(
            &[Node::new("leaf", "A leaf.").notes(&["The leaf note."])],
            Say::NONE,
        )
        .notes(&["The block note."]);
    let s = Section {
        name: "noted",
        node: &NOTED,
        declared: true,
        area: Area::Repo,
        at: "",
    };
    let mut out = String::new();
    section(&s, 1, &mut out);
    let block = out.find("The block note.\n").expect("the block note");
    let leaf = out
        .find("## `leaf` (optional, repeatable)\n\nA leaf.\n")
        .expect("the leaf section");
    assert!(block < leaf, "{out}");
    assert!(out[leaf..].contains("The leaf note.\n"), "{out}");
}
