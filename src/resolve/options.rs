//! The module's default, then the selected variant, then the image file.

use crate::diag::{Issue, Issues, Source};
use crate::model::image::{Entry, Image};
use crate::model::options::{check_values, env_name, env_value, Opt, Value, Variant};

/// Single pass, in one order, with no merging: the module's default, then the
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
    let module_path = entry.path.as_str();
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
                                format!("variant `{want}` sets `{name}`, which this module does not declare"),
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
                    Issue::new(format!("`{module_path}` has no variant `{want}`"), list_src).help(
                        if known.is_empty() {
                            "this module declares no variants".to_string()
                        } else {
                            format!("declared variants: {}", known.join(", "))
                        },
                    ),
                );
            }
        }
    }

    let mut seen: Vec<&str> = Vec::new();
    for (name, values, span) in set {
        let Some(opt) = find(name) else {
            let known: Vec<&str> = options.iter().map(|o| o.name.as_str()).collect();
            issues.push(
                Issue::new(format!("`{module_path}` has no option `{name}`"), list_src)
                    .at(*span, "not declared by this module")
                    .help(if known.is_empty() {
                        "this module declares no options".to_string()
                    } else {
                        format!("declared options: {}", known.join(", "))
                    }),
            );
            continue;
        };
        if seen.contains(&name.as_str()) {
            issues.push(
                Issue::new(
                    format!("`{name}` is set twice on `{module_path}`"),
                    list_src,
                )
                .at(*span, "set again here")
                .help("resolution is a single pass, so a second value is an error rather than a merge"),
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
