#!/usr/bin/env python3
"""Check whether the bieszczaders/kernel-cachyos COPR is keeping up with
upstream stable kernel releases.

Compares the COPR's latest successful kernel-cachyos build against
kernel.org's releases.json, within the same X.Y series so a deliberate
hold on the previous series doesn't false-alarm. Additionally checks
CISA's Known Exploited Vulnerabilities catalog for kernel CVEs whose
same-series fix version is newer than the COPR build.

Staleness is judged on two axes because releases.json only lists the
newest release per series: the age of that newest unbuilt release (which
resets every time upstream releases again, so it alone can't see a long
stall) and the count of point releases skipped (which only grows).

Statuses (GitHub Actions outputs, consumed by kernel-freshness.yml):
  current  newest same-series release, or briefly/harmlessly behind
  lagging  newest unbuilt release >= LAG_WARN_DAYS old, or >= 2 point
           releases behind                     -> tracking issue
  stale    newest unbuilt release >= LAG_STALE_DAYS old, >= 4 point
           releases behind, series EOL, or an
           unpatched KEV-listed CVE            -> fallback PR

Usage: kernel_freshness.py <report.md path>
Stdlib only; runs on the stock GitHub runner python3.
"""

import datetime
import json
import os
import sys
import urllib.request

COPR_BUILDS = (
    "https://copr.fedorainfracloud.org/api_3/build/list/"
    "?ownername=bieszczaders&projectname=kernel-cachyos"
    "&packagename=kernel-cachyos&limit=30"
)
KERNEL_RELEASES = "https://www.kernel.org/releases.json"
KEV_CATALOG = (
    "https://www.cisa.gov/sites/default/files/feeds/"
    "known_exploited_vulnerabilities.json"
)
CVE_API = "https://cveawg.mitre.org/api/cve/{}"

LAG_WARN_DAYS = int(os.environ.get("LAG_WARN_DAYS", "7"))
LAG_STALE_DAYS = int(os.environ.get("LAG_STALE_DAYS", "14"))
SKIP_WARN = int(os.environ.get("SKIP_WARN", "2"))
SKIP_STALE = int(os.environ.get("SKIP_STALE", "4"))
KEV_LOOKBACK_DAYS = int(os.environ.get("KEV_LOOKBACK_DAYS", "180"))

def fetch_json(url):
    req = urllib.request.Request(
        url, headers={"User-Agent": "kernel-freshness-probe"}
    )
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.load(resp)

def vertuple(version):
    return tuple(int(p) for p in version.split("."))

def copr_kernel_version():
    """Upstream version (e.g. 7.1.3) of the newest successful COPR build."""
    builds = fetch_json(COPR_BUILDS)["items"]
    for build in builds:  # newest first
        if build.get("state") != "succeeded":
            continue
        full = build["source_package"]["version"]  # e.g. 7.1.3-cachyos1
        return full.split(":")[-1].split("-")[0], full
    raise RuntimeError("no successful kernel-cachyos build found in COPR")

def same_series_releases(series):
    """kernel.org stable/longterm releases in the given X.Y series."""
    releases = fetch_json(KERNEL_RELEASES)["releases"]
    return [
        r
        for r in releases
        if r["moniker"] in ("stable", "longterm")
        and (r["version"] == series or r["version"].startswith(series + "."))
    ]

def kev_unpatched(series, copr_ver):
    """KEV-listed kernel CVEs fixed upstream in our series after copr_ver.

    The kernel CNA records per-branch fix versions in the CVE entry, so a
    same-series 'lessThan' newer than the COPR build means the COPR is
    shipping a known-exploited, already-fixed vulnerability.
    """
    today = datetime.date.today()
    hits = []
    catalog = fetch_json(KEV_CATALOG)["vulnerabilities"]
    for vuln in catalog:
        if vuln.get("vendorProject", "").lower() != "linux":
            continue
        if "kernel" not in vuln.get("product", "").lower():
            continue
        added = datetime.date.fromisoformat(vuln["dateAdded"])
        if (today - added).days > KEV_LOOKBACK_DAYS:
            continue
        cve_id = vuln["cveID"]
        try:
            cve = fetch_json(CVE_API.format(cve_id))
            affected = cve["containers"]["cna"].get("affected", [])
        except Exception as exc:  # noqa: BLE001 - one bad record shouldn't abort
            print(f"warning: could not evaluate {cve_id}: {exc}", file=sys.stderr)
            continue
        for aff in affected:
            for ver in aff.get("versions", []):
                fix = ver.get("lessThan") or ""
                if not (fix == series or fix.startswith(series + ".")):
                    continue
                try:
                    if vertuple(fix) > vertuple(copr_ver):
                        hits.append((cve_id, fix, vuln.get("shortDescription", "")))
                except ValueError:
                    continue
    return sorted(set(hits))

def gha_output(**kwargs):
    path = os.environ.get("GITHUB_OUTPUT")
    if not path:
        return
    with open(path, "a", encoding="utf-8") as fh:
        for key, value in kwargs.items():
            fh.write(f"{key}={value}\n")

def main():
    report_path = sys.argv[1]
    today = datetime.date.today()

    copr_ver, copr_full = copr_kernel_version()
    series = ".".join(copr_ver.split(".")[:2])
    releases = same_series_releases(series)

    newer = [r for r in releases if vertuple(r["version"]) > vertuple(copr_ver)]
    lag_days = 0
    skipped = 0
    if not releases:
        lag_days = None
        newest = None
        reason = f"series {series} is EOL (gone from kernel.org releases.json)"
    elif newer:
        newest_rel = max(newer, key=lambda r: vertuple(r["version"]))
        newest = newest_rel["version"]
        released = datetime.date.fromisoformat(newest_rel["released"]["isodate"])
        lag_days = (today - released).days
        skipped = (vertuple(newest) + (0,))[2] - (vertuple(copr_ver) + (0,))[2]
        reason = (
            f"{newest} released {released}, COPR still on {copr_ver}"
            f" ({skipped} point release(s) behind)"
        )
    else:
        newest = copr_ver
        reason = f"COPR has the newest {series} release"

    kev_error = None
    try:
        kev_hits = kev_unpatched(series, copr_ver)
    except Exception as exc:  # noqa: BLE001 - KEV is an accelerator, not the core check
        kev_hits = []
        kev_error = str(exc)
        print(f"warning: KEV check failed: {exc}", file=sys.stderr)

    if (
        not releases
        or (lag_days or 0) >= LAG_STALE_DAYS
        or skipped >= SKIP_STALE
        or kev_hits
    ):
        status = "stale"
    elif (lag_days or 0) >= LAG_WARN_DAYS or skipped >= SKIP_WARN:
        status = "lagging"
    else:
        status = "current"

    lines = [
        f"**Status: {status}** ({reason})",
        "",
        f"| | version |",
        f"|---|---|",
        f"| COPR `kernel-cachyos` | {copr_full} |",
        f"| newest same-series stable | {newest or 'series EOL'} |",
        "",
        f"Newest unbuilt release age: **{lag_days if lag_days is not None else 'n/a (EOL)'}"
        f" day(s)** (issue at {LAG_WARN_DAYS}, fallback PR at {LAG_STALE_DAYS}); "
        f"point releases behind: **{skipped}**"
        f" (issue at {SKIP_WARN}, fallback PR at {SKIP_STALE})",
    ]
    if kev_hits:
        lines += ["", "### Known-exploited CVEs unpatched in the COPR build", ""]
        lines += [
            f"- **{cve}** fixed in {fix}: {desc}" for cve, fix, desc in kev_hits
        ]
    if kev_error:
        lines += ["", f"_KEV check errored ({kev_error}); lag check still valid._"]
    lines += [
        "",
        "Sources: [COPR builds](https://copr.fedorainfracloud.org/coprs/"
        "bieszczaders/kernel-cachyos/builds/) · "
        "[kernel.org releases](https://www.kernel.org/releases.json) · "
        "[CISA KEV](https://www.cisa.gov/known-exploited-vulnerabilities-catalog)",
        "",
        f"_Checked {today} by the kernel-freshness workflow._",
    ]
    report = "\n".join(lines)

    with open(report_path, "w", encoding="utf-8") as fh:
        fh.write(report + "\n")
    print(report)

    out = {
        "status": status,
        "kernel_id": "cachyos",
        "module_path": "modules/kernel/cachyos-kernel",
        "copr_version": copr_ver,
        "upstream_version": newest or "",
        "lag_days": lag_days if lag_days is not None else 0,
        "kev_count": len(kev_hits),
    }
    sidecar = report_path.replace(".md", ".json")
    with open(sidecar, "w", encoding="utf-8") as fh:
        json.dump(out, fh)

    gha_output(
        status=status,
        kernel_id="cachyos",
        module_path="modules/kernel/cachyos-kernel",
        copr_version=copr_ver,
        upstream_version=newest or "",
        lag_days=lag_days if lag_days is not None else "",
        kev_count=len(kev_hits),
    )

if __name__ == "__main__":
    main()
