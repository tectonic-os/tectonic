#!/usr/bin/env bash
# What `scan.sh` decides, against fixtures, so the compliance job's logic is
# checked without a runner, a pushed image or an SSG datastream.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
scan="${here}/../scripts/scan.sh"
fixtures="${here}/scan-fixtures"
failed=0

check() {
    local what="$1" want="$2" got="$3"
    if [ "$want" = "$got" ]; then
        echo "ok   ${what}"
    else
        echo "FAIL ${what}: wanted [${want}], got [${got}]"
        failed=1
    fi
}

check "a benchmark number maps to its rule id" \
    "xccdf_org.ssgproject.content_rule_package_aide_installed" \
    "$("$scan" rule-id "${fixtures}/datastream.xml" 1.1.1.1)"

check "a stig ident maps too" \
    "xccdf_org.ssgproject.content_rule_sshd_disable_root_login" \
    "$("$scan" rule-id "${fixtures}/datastream.xml" RHEL-09-232010)"

# A number that maps to nothing is a failure of the declaration, not the image.
if "$scan" rule-id "${fixtures}/datastream.xml" 9.9.9.9 > /dev/null 2>&1; then
    echo "FAIL a rule that maps to nothing has to fail"
    failed=1
else
    echo "ok   a rule that maps to nothing fails"
fi

check "a measured pass reads back" "pass" \
    "$("$scan" result "${fixtures}/arf.xml" xccdf_org.ssgproject.content_rule_package_aide_installed)"
check "a measured fail reads back" "fail" \
    "$("$scan" result "${fixtures}/arf.xml" xccdf_org.ssgproject.content_rule_sshd_disable_root_login)"
check "a rule the report says nothing about" "notselected" \
    "$("$scan" result "${fixtures}/arf.xml" nosuch)"

check "claims come off the listed modules" \
    "one/hello|cis-fedora|1.1.1.1" \
    "$("$scan" claims "${fixtures}/plan.json" enforced/none | head -1)"

# The claims of a module the base displaced belong to an image this is not.
check "a suppressed module's claims are not read" "" \
    "$("$scan" claims "${fixtures}/suppressed.json" suppressed/none)"

check "a lost overlay names who took it" \
    "/usr/lib/clash.conf|core/second" \
    "$("$scan" blame "${fixtures}/overlay.json" broken-overlay/none core/first)"

check "the posture is read from the plan" "true" \
    "$("$scan" enforced "${fixtures}/plan.json")"
check "and is false where nothing declares it" "false" \
    "$("$scan" enforced "${fixtures}/overlay.json")"

exit "$failed"
