//! One shipped workflow body with the repository's declaration applied.

/// The schedule substituted, and a guarded region kept only where its fact
/// holds. No marker survives into what is written.
pub fn render(body: &str, schedule: Option<&str>, facts: &[&str]) -> String {
    let mut out = String::new();
    let mut keep = true;
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(fact) = trimmed.strip_prefix("# tect:only ") {
            keep = facts.contains(&fact);
            continue;
        }
        if trimmed == "# tect:end" {
            keep = true;
            continue;
        }
        if !keep {
            continue;
        }
        match line.trim_end().strip_suffix("# tect:schedule") {
            Some(cron) => out.push_str(&scheduled(cron.trim_end(), schedule)),
            None => out.push_str(line),
        }
        out.push('\n');
    }
    out
}

/// The quoted value on the cron line replaced, leaving the line as shipped
/// where nothing declared one.
fn scheduled(line: &str, schedule: Option<&str>) -> String {
    let Some(cron) = schedule else {
        return line.to_string();
    };
    let Some(open) = line.find('\'') else {
        return line.to_string();
    };
    let Some(close) = line[open + 1..].find('\'').map(|at| open + 1 + at) else {
        return line.to_string();
    };
    let mut out = line.to_string();
    out.replace_range(open + 1..close, cron);
    out
}

#[cfg(test)]
mod tests {
    use super::render;

    const BODY: &str = include_str!("../../assets/.github/workflows/build.yml");

    #[test]
    fn the_kernel_input_is_there_only_for_a_repository_that_has_one() {
        let with = render(BODY, None, &["kernel"]);
        assert!(with.contains("inputs:"), "{with}");
        assert!(with.contains("    - cron: '45 9 1 * *'"), "{with}");

        let without = render(BODY, None, &["no-kernel"]);
        assert!(!without.contains("inputs:"), "{without}");
        assert!(!without.contains("- cron: '45 9 1 * *'"), "{without}");
        assert!(without.contains("  workflow_dispatch:\n"), "{without}");
        assert!(without.contains("  push:\n"), "{without}");
        // Every step reading it runs under `set -u`, so it is declared either way.
        assert!(without.contains("\n  KERNEL: ''\n"), "{without}");
        assert!(!without.contains("github.event.inputs.kernel"), "{without}");
    }

    #[test]
    fn the_declared_schedule_replaces_the_shipped_one() {
        let out = render(BODY, Some("0 5 * * 1"), &["no-kernel"]);
        assert!(out.contains("    - cron: '0 5 * * 1'\n"), "{out}");
        assert!(!out.contains("30 12 * * *"), "{out}");
    }

    /// Every shipped body, not just the one with regions in it: a marker left
    /// behind anywhere is a comment a repository commits and nobody wrote.
    #[test]
    fn no_marker_reaches_a_generated_file() {
        for shipped in crate::resolve::workflow::SHIPPED {
            for facts in [&["kernel"][..], &["no-kernel"][..]] {
                for schedule in [None, Some("0 5 * * 1")] {
                    let out = render(shipped.body, schedule, facts);
                    assert!(!out.contains("tect:"), "{}\n{out}", shipped.stem);
                    if schedule.is_some() && shipped.at.is_some() {
                        assert!(
                            out.contains("    - cron: '0 5 * * 1'\n"),
                            "{}\n{out}",
                            shipped.stem
                        );
                    }
                }
            }
        }
    }
}
