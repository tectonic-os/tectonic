# KDL

Every file that `tect` reads is written in [KDL](https://kdl.dev) 2. KDL is a document language built from nodes. A node can hold other nodes, so a file reads as an outline of what it declares. KDL needs less punctuation than XML or JSON, it allows comments, and each value has a type. The user reads and edits these files by hand, and `tect check` reports each problem at the line that caused it.

## A node

This `repo.kdl` holds three nodes:

```kdl
schema-version 1
name "Workstation"
workflows at="12:30" {
    build
}
```

A node sits on one line and has these parts:

- The node name comes first, as `schema-version`, `name` and `workflows` do.
- A value after the name is an argument. `name` takes one argument, `"Workstation"`.
- A `key=value` pair is a property. `at="12:30"` is a property of `workflows`.
- A block in braces holds child nodes. `build` is a child of `workflows`.

The schema pages list the nodes that each file accepts, with the arguments, properties and children of each node.

## Values

| Value | Written as |
| --- | --- |
| *string* | `"Workstation"`, in double quotes |
| *number* | `1` |
| *boolean* | `#true` or `#false` |

```kdl
schema-version 1
name "Workstation"
audit {
    enforce #true
}
```

## Comments

- `//` starts a comment that runs to the end of the line.
- `/*` and `*/` hold a comment that can span lines.
- `/-` in front of a node comments out that whole node, with its block. It is the quick way to switch a node off.

```kdl
// The schema release that this file is written against.
schema-version 1
name "Workstation"
/-workflows at="12:30" {
    build
}
```
