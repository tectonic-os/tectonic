//! The component's default, then the selected variant, then the image file.

use crate::diag::{Issue, Issues, Source};
use crate::model::component::Component;
use crate::model::image::{Entry, Image};
use crate::model::options::{check_values, env_name, env_value, Opt, Value, Variant};

/// Single pass, in one order, with no merging: the component's default, then the
/// selected variant, then the value in the image file.
pub fn resolve(
    options: &[Opt],
    variants: &[Variant],
    src: &Source,
    entry: &Entry,
    image: &Image,
    issues: &mut Issues,
) -> Vec<(String, String)> {
    let selected = entry.variant.as_ref();
    let set = &entry.options;
    let component_path = entry.path.as_str();
    let list_src = &image.src;

    let mut resolved: Vec<(String, Vec<Value>)> = options
        .iter()
        .map(|o| (o.name.clone(), o.default.clone()))
        .collect();

    let find = |name: &str| options.iter().find(|o| o.name == name);

    if let Some(want) = selected {
        match variants.iter().find(|v| &v.name == want) {
            Some(variant) => {
                for (name, values, span) in &variant.sets {
                    let Some(opt) = find(name) else {
                        issues.push(
                            Issue::new(
                                format!("variant `{want}` sets `{name}`, which this component does not declare"),
                                src,
                            )
                            .at(*span, "no such option")
                            .help("a variant may only set options declared in the same manifest"),
                        );
                        continue;
                    };
                    if check_values(name, opt.ty, values, src, *span, issues) {
                        if let Some(slot) = resolved.iter_mut().find(|(n, _)| n == name) {
                            slot.1 = values.clone();
                        }
                    }
                }
            }
            None => {
                let known: Vec<&str> = variants.iter().map(|v| v.name.as_str()).collect();
                issues.push(
                    Issue::new(
                        format!("`{component_path}` has no variant `{want}`"),
                        list_src,
                    )
                    .help(if known.is_empty() {
                        "this component declares no variants".to_string()
                    } else {
                        format!("declared variants: {}", known.join(", "))
                    }),
                );
            }
        }
    }

    let mut seen: Vec<&str> = Vec::new();
    for (name, values, span) in set {
        let Some(opt) = find(name) else {
            let known: Vec<&str> = options.iter().map(|o| o.name.as_str()).collect();
            issues.push(
                Issue::new(
                    format!("`{component_path}` has no option `{name}`"),
                    list_src,
                )
                .at(*span, "not declared by this component")
                .help(if known.is_empty() {
                    "this component declares no options".to_string()
                } else {
                    format!("declared options: {}", known.join(", "))
                }),
            );
            continue;
        };
        if seen.contains(&name.as_str()) {
            issues.push(
                Issue::new(
                    format!("`{name}` is set twice on `{component_path}`"),
                    list_src,
                )
                .at(*span, "set again here")
                .help("resolution is a single pass, so a second value is an error"),
            );
            continue;
        }
        seen.push(name.as_str());

        if check_values(name, opt.ty, values, list_src, *span, issues) {
            if let Some(slot) = resolved.iter_mut().find(|(n, _)| n == name) {
                slot.1 = values.clone();
            }
        }
    }

    options
        .iter()
        .map(|opt| {
            let values = resolved
                .iter()
                .find(|(n, _)| n == &opt.name)
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            (env_name(&opt.name), env_value(opt.ty, &values))
        })
        .collect()
}

/// Replaces each `$<name>` reference in a declaration's string with what
/// `value` gives for the option it names. A name runs over lower-case
/// letters, digits and dashes, which is the option name rule. A `$` that
/// starts no name stays as written.
pub fn interpolate(text: &str, mut value: impl FnMut(&str) -> String) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find('$') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let len = match after.starts_with(|c: char| c.is_ascii_lowercase()) {
            true => after
                .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
                .unwrap_or(after.len()),
            false => 0,
        };
        match len {
            0 => out.push('$'),
            _ => out.push_str(&value(&after[..len])),
        }
        rest = &after[len..];
    }
    out.push_str(rest);
    out
}

/// Writes the entry's option values into the component's package names and
/// provider choices. The manifest parse already held each reference to a
/// declared string option. The value comes from the image file, so a package
/// name it spoils is reported against the entry.
pub fn apply(component: &mut Component, entry: &Entry, image: &Image, issues: &mut Issues) {
    let resolved = &component.resolved;
    let value = |name: &str| {
        let env = env_name(name);
        resolved
            .iter()
            .find(|(have, _)| *have == env)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    for batch in component
        .packages
        .iter_mut()
        .chain(component.groups.iter_mut())
    {
        let mut kept = Vec::new();
        for name in &batch.packages {
            let written = interpolate(name, value);
            if written == *name {
                kept.push(written);
                continue;
            }
            match crate::parse::component::bad_package(&written) {
                Some(problem) => issues.push(
                    Issue::new(
                        format!("`{}` installs `{name}` as `{written}`, which {problem}", entry.path),
                        &image.src,
                    )
                    .at(entry.span, "an option value spoils the package name")
                    .help("the option set on this entry, or its default, fills the package name; it holds letters, digits and . _ + : -"),
                ),
                None => kept.push(written),
            }
        }
        batch.packages = kept;
    }
    for import in &mut component.imports {
        import.path = interpolate(&import.path, value);
        for (_, values) in &mut import.options {
            for held in values {
                if let Value::String(text) = held {
                    *text = interpolate(text, value);
                }
            }
        }
    }
    if let Some(base) = &mut component.base {
        base.image = interpolate(&base.image, value);
    }
}

#[cfg(test)]
mod tests {
    use super::interpolate;

    #[test]
    fn a_reference_reads_the_option_it_names() {
        let written = interpolate("firefox=$version", |name| format!("<{name}>"));
        assert_eq!(written, "firefox=<version>");
    }

    #[test]
    fn a_reference_name_ends_at_the_first_character_outside_the_name_rule() {
        let written = interpolate("$a-1.$b_c", |name| format!("<{name}>"));
        assert_eq!(written, "<a-1>.<b>_c");
    }

    #[test]
    fn a_dollar_that_starts_no_name_stays() {
        assert_eq!(interpolate("a$1$", |_| "x".to_string()), "a$1$");
    }
}
