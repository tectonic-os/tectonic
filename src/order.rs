//! The order the layers build in, resolved from the graph.

use crate::diag::{Issue, Issues};
use crate::list::{Entry, Image};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

/// The build order, as list indices.
pub fn sort(image: &Image, issues: &mut Issues) -> Vec<usize> {
    let mut offered: BTreeMap<&str, usize> = BTreeMap::new();
    for (index, entry) in image.entries.iter().enumerate() {
        let Some(module) = &entry.module else { continue };
        for decl in module.provides.iter().chain(module.provides_files.iter()) {
            offered.entry(decl.name.as_str()).or_insert(index);
        }
    }

    let n = image.entries.len();
    let mut waits_on: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (index, entry) in image.entries.iter().enumerate() {
        let Some(module) = &entry.module else { continue };
        let hard = module.requires.iter().chain(module.requires_files.iter());
        for decl in hard {
            if let Some(&provider) = offered.get(decl.name.as_str()) {
                if provider != index {
                    waits_on[index].push(provider);
                }
            }
        }
        for decl in &module.after {
            let Some(&provider) = offered.get(decl.name.as_str()) else {
                continue;
            };
            let drags_below_gate =
                entry.flavour.is_none() && image.entries[provider].flavour.is_some();
            if provider != index && !drags_below_gate {
                waits_on[index].push(provider);
            }
        }
        waits_on[index].sort_unstable();
        waits_on[index].dedup();
    }

    let mut blocking: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut remaining: Vec<usize> = vec![0; n];
    for (index, providers) in waits_on.iter().enumerate() {
        remaining[index] = providers.len();
        for &provider in providers {
            blocking[provider].push(index);
        }
    }

    let key = |index: usize| {
        let gated = u8::from(image.entries[index].flavour.is_some());
        Reverse((gated, index))
    };

    let mut ready: BinaryHeap<Reverse<(u8, usize)>> = (0..n)
        .filter(|&index| remaining[index] == 0)
        .map(key)
        .collect();

    let mut order = Vec::with_capacity(n);
    while let Some(Reverse((_, index))) = ready.pop() {
        order.push(index);
        for &waiting in &blocking[index] {
            remaining[waiting] -= 1;
            if remaining[waiting] == 0 {
                ready.push(key(waiting));
            }
        }
    }

    if order.len() < n {
        report_cycle(image, &waits_on, &remaining, issues);
        order.extend((0..n).filter(|index| remaining[*index] > 0));
    }
    order
}

/// Rearranges the list into build order, so everything downstream — the
/// generated Containerfile, the resolved summary, the finalize hook order —
/// sees one order and none of them has to know it was ever different.
pub fn apply(image: &mut Image, order: &[usize]) {
    let mut taken: Vec<Option<Entry>> = image.entries.drain(..).map(Some).collect();
    image.entries = order
        .iter()
        .filter_map(|&index| taken[index].take())
        .collect();
}

/// Everything left when the sort runs out of ready modules is waiting on
/// something else that is also waiting, so the message names the edges rather
/// than just reporting that an order could not be found.
fn report_cycle(image: &Image, waits_on: &[Vec<usize>], remaining: &[usize], issues: &mut Issues) {
    let name = |index: usize| match &image.entries[index].flavour {
        Some(flavour) => format!("{} [{flavour}]", image.entries[index].path),
        None => image.entries[index].path.clone(),
    };

    let mut issue = Issue::new("the module graph has a cycle", &image.src).help(
        "a requirement implies ordering, so a cycle has no build order at all; \
         drop one of the edges, or split the module that closes it",
    );
    for (index, providers) in waits_on.iter().enumerate() {
        if remaining[index] == 0 {
            continue;
        }
        let blocked: Vec<String> = providers
            .iter()
            .filter(|&&provider| remaining[provider] > 0)
            .map(|&provider| format!("`{}`", name(provider)))
            .collect();
        issue = issue.at(
            image.entries[index].span,
            format!("waits on {}", blocked.join(", ")),
        );
    }
    issues.push(issue);
}
