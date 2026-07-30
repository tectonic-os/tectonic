//! The order the layers build in, resolved from the graph.

use crate::diag::{Issue, Issues};
use crate::list::{Entry, List};
use crate::module::Module;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

/// The build order, as list indices.
pub fn sort(list: &List, modules: &[Module], issues: &mut Issues) -> Vec<usize> {
    let by_entry: Vec<Option<&Module>> = list
        .entries
        .iter()
        .map(|e| {
            modules
                .iter()
                .find(|m| m.path == e.path && m.flavour == e.flavour)
        })
        .collect();

    let mut offered: BTreeMap<&str, usize> = BTreeMap::new();
    for (index, module) in by_entry.iter().enumerate() {
        let Some(module) = module else { continue };
        for decl in module.provides.iter().chain(module.provides_files.iter()) {
            offered.entry(decl.name.as_str()).or_insert(index);
        }
    }

    let n = list.entries.len();
    let mut waits_on: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (index, module) in by_entry.iter().enumerate() {
        let Some(module) = module else { continue };
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
                module.flavour.is_none() && by_entry[provider].is_some_and(|p| p.flavour.is_some());
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
        let gated = u8::from(by_entry[index].is_some_and(|m| m.flavour.is_some()));
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
        report_cycle(list, &by_entry, &waits_on, &remaining, issues);
        order.extend((0..n).filter(|index| remaining[*index] > 0));
    }
    order
}

/// Rearranges the list and the loaded manifests into build order, so
/// everything downstream — the generated Containerfile, the resolved
/// summary, the finalize hook order — sees one order and none of them has to
/// know it was ever different.
pub fn apply(list: &mut List, modules: &mut [Module], order: &[usize]) {
    let mut taken: Vec<Option<Entry>> = list.entries.drain(..).map(Some).collect();
    list.entries = order
        .iter()
        .filter_map(|&index| taken[index].take())
        .collect();
    modules.sort_by_key(|m| {
        list.entries
            .iter()
            .position(|e| e.path == m.path && e.flavour == m.flavour)
            .unwrap_or(usize::MAX)
    });
}

/// Everything left when the sort runs out of ready modules is waiting on
/// something else that is also waiting, so the message names the edges rather
/// than just reporting that an order could not be found.
fn report_cycle(
    list: &List,
    by_entry: &[Option<&Module>],
    waits_on: &[Vec<usize>],
    remaining: &[usize],
    issues: &mut Issues,
) {
    let name = |index: usize| match by_entry[index].and_then(|m| m.flavour.as_deref()) {
        Some(flavour) => format!("{} [{flavour}]", list.entries[index].path),
        None => list.entries[index].path.clone(),
    };

    let mut issue = Issue::new("the module graph has a cycle", &list.file, &list.text).help(
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
            list.entries[index].span,
            format!("waits on {}", blocked.join(", ")),
        );
    }
    issues.push(issue);
}
