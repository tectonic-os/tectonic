#!/usr/bin/env python3
"""Unit tests for kernel_freshness.py.

Runs main() end-to-end against canned API responses (fetch_json is
mocked), asserting on the status written to GITHUB_OUTPUT and on the
report. Dates are generated relative to today so age thresholds are
deterministic without freezing the clock.

Usage: python3 -m unittest discover -s modules/kernel/cachyos-kernel
Stdlib only, like the script under test.
"""

import contextlib
import datetime
import io
import os
import tempfile
import unittest
from unittest import mock

import kernel_freshness as kf

def days_ago(n):
    return (datetime.date.today() - datetime.timedelta(days=n)).isoformat()

def copr_payload(*builds):
    """Each build is (state, version) newest-first, e.g. ('succeeded', '7.1.3-cachyos1')."""
    return {
        "items": [
            {"state": state, "source_package": {"version": version}}
            for state, version in builds
        ]
    }

def releases_payload(*releases):
    """Each release is (version, isodate) or (version, isodate, moniker)."""
    return {
        "releases": [
            {
                "moniker": r[2] if len(r) > 2 else "stable",
                "version": r[0],
                "released": {"isodate": r[1]},
            }
            for r in releases
        ]
    }

def kev_payload(*vulns):
    """Each vuln is (cve_id, date_added); vendor/product preset to linux kernel."""
    return {
        "vulnerabilities": [
            {
                "cveID": cve_id,
                "dateAdded": date_added,
                "vendorProject": "Linux",
                "product": "Linux Kernel",
                "shortDescription": "a kernel bug",
            }
            for cve_id, date_added in vulns
        ]
    }

def cve_payload(*less_thans):
    return {
        "containers": {
            "cna": {
                "affected": [
                    {"versions": [{"lessThan": lt} for lt in less_thans]}
                ]
            }
        }
    }

EMPTY_KEV = {"vulnerabilities": []}

def run_main(copr, releases, kev=EMPTY_KEV, cves=None):
    """Runs kf.main() with mocked fetches; returns (report, outputs dict)."""
    cves = cves or {}

    def fetch(url):
        if url == kf.COPR_BUILDS:
            return copr
        if url == kf.KERNEL_RELEASES:
            return releases
        if url == kf.KEV_CATALOG:
            if isinstance(kev, Exception):
                raise kev
            return kev
        for cve_id, payload in cves.items():
            if url == kf.CVE_API.format(cve_id):
                return payload
        raise AssertionError(f"unexpected fetch: {url}")

    with tempfile.TemporaryDirectory() as tmp:
        report_path = os.path.join(tmp, "report.md")
        output_path = os.path.join(tmp, "output.txt")
        with (
            mock.patch.object(kf, "fetch_json", side_effect=fetch),
            mock.patch.object(kf.sys, "argv", ["kernel_freshness.py", report_path]),
            mock.patch.dict(os.environ, {"GITHUB_OUTPUT": output_path}),
            contextlib.redirect_stdout(io.StringIO()),
            contextlib.redirect_stderr(io.StringIO()),
        ):
            kf.main()
        with open(report_path, encoding="utf-8") as fh:
            report = fh.read()
        outputs = {}
        with open(output_path, encoding="utf-8") as fh:
            for line in fh:
                key, _, value = line.strip().partition("=")
                outputs[key] = value
    return report, outputs

class VertupleTest(unittest.TestCase):
    def test_orders_numerically_not_lexically(self):
        self.assertGreater(kf.vertuple("7.1.10"), kf.vertuple("7.1.9"))

    def test_handles_two_part_versions(self):
        self.assertLess(kf.vertuple("7.1"), kf.vertuple("7.1.1"))

class CoprVersionTest(unittest.TestCase):
    def test_skips_unsuccessful_builds(self):
        copr = copr_payload(("failed", "7.1.4-cachyos1"), ("succeeded", "7.1.3-cachyos1"))
        _, outputs = run_main(copr, releases_payload(("7.1.3", days_ago(1))))
        self.assertEqual(outputs["copr_version"], "7.1.3")

    def test_strips_rpm_epoch(self):
        copr = copr_payload(("succeeded", "1:7.1.3-cachyos1"))
        _, outputs = run_main(copr, releases_payload(("7.1.3", days_ago(1))))
        self.assertEqual(outputs["copr_version"], "7.1.3")

class StatusTest(unittest.TestCase):
    COPR = copr_payload(("succeeded", "7.1.3-cachyos1"))

    def test_current_when_copr_has_newest(self):
        _, outputs = run_main(self.COPR, releases_payload(("7.1.3", days_ago(3))))
        self.assertEqual(outputs["status"], "current")

    def test_current_ignores_older_releases(self):
        _, outputs = run_main(self.COPR, releases_payload(("7.1.2", days_ago(30))))
        self.assertEqual(outputs["status"], "current")

    def test_current_when_briefly_behind(self):
        _, outputs = run_main(self.COPR, releases_payload(("7.1.4", days_ago(2))))
        self.assertEqual(outputs["status"], "current")

    def test_lagging_when_unbuilt_release_ages(self):
        _, outputs = run_main(
            self.COPR, releases_payload(("7.1.4", days_ago(kf.LAG_WARN_DAYS + 1)))
        )
        self.assertEqual(outputs["status"], "lagging")

    def test_lagging_when_point_releases_skipped(self):
        _, outputs = run_main(
            self.COPR,
            releases_payload((f"7.1.{3 + kf.SKIP_WARN}", days_ago(1))),
        )
        self.assertEqual(outputs["status"], "lagging")

    def test_stale_when_unbuilt_release_is_old(self):
        _, outputs = run_main(
            self.COPR, releases_payload(("7.1.4", days_ago(kf.LAG_STALE_DAYS + 1)))
        )
        self.assertEqual(outputs["status"], "stale")

    def test_stale_when_many_point_releases_skipped(self):
        _, outputs = run_main(
            self.COPR,
            releases_payload((f"7.1.{3 + kf.SKIP_STALE}", days_ago(1))),
        )
        self.assertEqual(outputs["status"], "stale")

    def test_stale_when_series_is_eol(self):
        report, outputs = run_main(self.COPR, releases_payload(("7.2.1", days_ago(1))))
        self.assertEqual(outputs["status"], "stale")
        self.assertIn("EOL", report)

    def test_other_series_does_not_affect_lag(self):
        _, outputs = run_main(
            self.COPR,
            releases_payload(("7.2.5", days_ago(30)), ("7.1.3", days_ago(3))),
        )
        self.assertEqual(outputs["status"], "current")

class KevTest(unittest.TestCase):
    COPR = copr_payload(("succeeded", "7.1.3-cachyos1"))
    CURRENT_RELEASES = releases_payload(("7.1.3", days_ago(3)))

    def test_stale_on_unpatched_kev_cve(self):
        report, outputs = run_main(
            self.COPR,
            self.CURRENT_RELEASES,
            kev=kev_payload(("CVE-2026-0001", days_ago(10))),
            cves={"CVE-2026-0001": cve_payload("7.1.4")},
        )
        self.assertEqual(outputs["status"], "stale")
        self.assertEqual(outputs["kev_count"], "1")
        self.assertIn("CVE-2026-0001", report)

    def test_fix_in_other_series_is_ignored(self):
        _, outputs = run_main(
            self.COPR,
            self.CURRENT_RELEASES,
            kev=kev_payload(("CVE-2026-0002", days_ago(10))),
            cves={"CVE-2026-0002": cve_payload("7.2.4")},
        )
        self.assertEqual(outputs["status"], "current")

    def test_already_patched_cve_is_ignored(self):
        _, outputs = run_main(
            self.COPR,
            self.CURRENT_RELEASES,
            kev=kev_payload(("CVE-2026-0003", days_ago(10))),
            cves={"CVE-2026-0003": cve_payload("7.1.3")},
        )
        self.assertEqual(outputs["status"], "current")

    def test_old_kev_entries_outside_lookback_are_skipped(self):
        _, outputs = run_main(
            self.COPR,
            self.CURRENT_RELEASES,
            kev=kev_payload(("CVE-2020-9999", days_ago(kf.KEV_LOOKBACK_DAYS + 1))),
        )
        self.assertEqual(outputs["status"], "current")

    def test_kev_failure_is_nonfatal(self):
        report, outputs = run_main(
            self.COPR,
            self.CURRENT_RELEASES,
            kev=RuntimeError("KEV feed down"),
        )
        self.assertEqual(outputs["status"], "current")
        self.assertIn("KEV check errored", report)

if __name__ == "__main__":
    unittest.main()
