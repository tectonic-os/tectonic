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

    #[test]
    fn a_checksum_fixup_says_which_commit_the_later_jobs_build() {
        assert!(BODY.contains(
            "echo \"Checksum fixup commit $(git rev-parse --short HEAD); later jobs build the amended tree.\" >> \"$GITHUB_STEP_SUMMARY\""
        ));
    }

    #[test]
    fn a_scheduled_scan_is_absent_from_pushes() {
        let out = render(BODY, None, &["no-kernel", "scheduled-scan"]);
        assert!(
            out.contains(
                "    if: needs.build_push.outputs.publish == 'true' && (github.event_name == 'schedule' || github.event_name == 'workflow_dispatch')\n"
            ),
            "{out}"
        );
        assert!(
            !out.contains(
                "    if: needs.build_push.outputs.publish == 'true' && github.event_name != 'pull_request'\n"
            ),
            "{out}"
        );
    }

    #[test]
    fn scheduled_publishing_is_absent_from_pushes_but_not_dispatches() {
        let gate = r#"          if [ "${{ github.event_name }}" != "schedule" ] \
             && [ "${{ github.event_name }}" != "workflow_dispatch" ]; then
            publish=false
          fi
"#;
        let scheduled = render(BODY, None, &["no-kernel", "scheduled-publish"]);
        assert!(scheduled.contains(gate), "{scheduled}");
        assert!(scheduled
            .contains("    outputs:\n      publish: ${{ steps.prepare.outputs.publish }}\n"));
        assert!(scheduled.contains("      - name: Prepare environment\n        id: prepare\n"));
        assert!(scheduled.contains(
            "          echo \"PUBLISH=${publish}\" >> \"${GITHUB_ENV}\"\n          echo \"publish=${publish}\" >> \"${GITHUB_OUTPUT}\"\n"
        ));
        assert!(scheduled.contains(
            "      - name: Login to GitHub Container Registry\n        uses: docker/login-action@af1e73f918a031802d376d3c8bbc3fe56130a9b0 # v4.4.0\n"
        ));

        let pushed = render(BODY, None, &["no-kernel", "push-publish"]);
        assert!(!pushed.contains(gate), "{pushed}");
    }

    #[test]
    fn fresh_jobs_fetch_modules_before_reading_the_plan() {
        assert!(BODY
            .contains("run: |\n          ./scripts/tect.sh fetch modules\n          selection="));
        assert!(BODY.contains(
            "set -euo pipefail\n          ./scripts/tect.sh fetch modules\n          ./scripts/tect.sh plan"
        ));
        assert!(BODY.contains(
            "set -euo pipefail\n          ./scripts/tect.sh fetch modules\n          datastream="
        ));
        for shipped in crate::resolve::workflow::SHIPPED {
            if shipped.body.contains("plan --json") {
                assert!(
                    shipped.body.contains("fetch modules"),
                    "{} reads the plan without fetching modules",
                    shipped.stem
                );
            }
        }
    }

    /// Every shipped body, not just the one with regions in it: a marker left
    /// behind anywhere is a comment a repository commits and nobody wrote.
    #[test]
    fn no_marker_reaches_a_generated_file() {
        for shipped in crate::resolve::workflow::SHIPPED {
            for facts in [
                &["kernel", "push-scan"][..],
                &["no-kernel", "scheduled-publish", "scheduled-scan"][..],
            ] {
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

    #[test]
    fn every_shipped_job_declares_its_own_permissions() {
        let is_job = |line: &str| -> bool {
            let Some(rest) = line.strip_prefix("  ") else {
                return false;
            };
            let Some(name) = rest.strip_suffix(':') else {
                return false;
            };
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        };
        for shipped in crate::resolve::workflow::SHIPPED {
            let lines: Vec<&str> = shipped.body.lines().collect();
            let Some(jobs_at) = lines.iter().position(|line| *line == "jobs:") else {
                panic!("{}: has no `jobs:` section", shipped.stem);
            };
            assert!(
                lines[..jobs_at].contains(&"permissions: {}"),
                "{}: no top-level `permissions: {{}}`",
                shipped.stem
            );
            let mut job: Option<&str> = None;
            let mut declared = false;
            for line in &lines[jobs_at + 1..] {
                if is_job(line) {
                    if let Some(prev) = job {
                        assert!(
                            declared,
                            "{}: job `{}` declares no permissions",
                            shipped.stem, prev
                        );
                    }
                    job = Some(&line[2..line.len() - 1]);
                    declared = false;
                } else if !declared && *line == "    permissions:" {
                    declared = true;
                }
            }
            if let Some(prev) = job {
                assert!(
                    declared,
                    "{}: job `{}` declares no permissions",
                    shipped.stem, prev
                );
            }
        }
    }

    #[test]
    fn every_checkout_says_whether_it_keeps_the_token() {
        let mut checkouts = 0;
        let mut persist = 0;
        for shipped in crate::resolve::workflow::SHIPPED {
            let lines: Vec<&str> = shipped.body.lines().collect();
            for (at, line) in lines.iter().enumerate() {
                if !line.contains("uses: actions/checkout@") {
                    continue;
                }
                checkouts += 1;
                let indent = line.len() - line.trim_start().len();
                let declared = lines[at + 1..]
                    .iter()
                    .take_while(|next| {
                        let trimmed = next.trim_start();
                        !(trimmed.starts_with("- ") && next.len() - trimmed.len() <= indent)
                    })
                    .any(|next| next.trim().starts_with("persist-credentials:"));
                assert!(
                    declared,
                    "{}: checkout step does not name `persist-credentials:`",
                    shipped.stem
                );
            }
            persist += lines
                .iter()
                .filter(|line| line.trim().starts_with("persist-credentials:"))
                .count();
        }
        assert_eq!(
            checkouts, persist,
            "every checkout names whether it keeps the token"
        );
    }
}
