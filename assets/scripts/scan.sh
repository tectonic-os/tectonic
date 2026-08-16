#!/usr/bin/env bash
# The declared-versus-measured half of the compliance scan, apart from the
# workflow that installs oscap and runs it, so the logic is testable without a
# runner, a pushed image or an SSG datastream.
set -euo pipefail

usage() {
    cat >&2 << 'EOF'
usage: scan.sh <command> [args...]

  claims <plan.json> <target>
      Every rule this target's modules claim, as module|benchmark|rule. Reads
      .modules[] and never .suppressed[], whose claims belong to modules the
      base displaced.

  rule-id <datastream.xml> <number>
      The XCCDF rule id that benchmark numbering maps to, through the
      datastream's own references. Exit 1 when it maps to nothing, which is a
      failure of the declaration rather than of the image.

  result <arf.xml> <rule-id>
      What the scan measured for one rule: pass, fail, notapplicable, and so
      on. `notselected` when the report says nothing about it.

  blame <plan.json> <target> <module>
      Every overlay path this module ships that another module replaced, as
      path|by. What tells a false claim from a composition that defeats a true
      one.

  enforced <plan.json>
      true when the repository declares audit { enforce #true }.
EOF
    exit 1
}

need() {
    [ "$1" -eq "$2" ] || usage
}

# `.modules[]`, never `.suppressed[]`: a suppressed module contributes no layer,
# so its claims are about an image this is not.
claims() {
    jq -r --arg t "$2" '
        .images[].targets[] | select(.name == $t) | .modules[]
        | . as $m | .satisfies[]
        | . as $s | $s.rules[]
        | [$m.path, $s.benchmark, .] | join("|")
    ' "$1"
}

blame() {
    jq -r --arg t "$2" --arg m "$3" '
        .images[].targets[] | select(.name == $t)
        | .overlay_overridden[] | select(.module == $m)
        | [.path, .by] | join("|")
    ' "$1"
}

enforced() {
    jq -r '.audit.enforce // false' "$1"
}

# XCCDF and ARF are namespaced and the namespace moves between revisions, so
# every match here is on the local name.
xml() {
    python3 - "$@" << 'PY'
import sys, xml.etree.ElementTree as ET

def local(tag):
    return tag.rsplit("}", 1)[-1]

what, path, wanted = sys.argv[1], sys.argv[2], sys.argv[3]
try:
    root = ET.parse(path).getroot()
except (OSError, ET.ParseError) as err:
    print(f"scan: {path}: {err}", file=sys.stderr)
    sys.exit(2)

if what == "rule-id":
    # The declared number is benchmark numbering; only the datastream knows
    # which rule it refers to.
    for node in root.iter():
        if local(node.tag) != "Rule":
            continue
        for child in node:
            if local(child.tag) not in ("reference", "ident", "version"):
                continue
            if (child.text or "").strip() == wanted:
                print(node.get("id", ""))
                sys.exit(0)
    print(f"scan: {wanted} maps to no rule in the datastream", file=sys.stderr)
    sys.exit(1)

if what == "result":
    for node in root.iter():
        if local(node.tag) != "rule-result" or node.get("idref") != wanted:
            continue
        for child in node:
            if local(child.tag) == "result":
                print((child.text or "").strip())
                sys.exit(0)
    print("notselected")
    sys.exit(0)

sys.exit(2)
PY
}

[ $# -ge 1 ] || usage
command="$1"
shift
case "$command" in
    claims)
        need $# 2
        claims "$@"
        ;;
    rule-id)
        need $# 2
        xml rule-id "$@"
        ;;
    result)
        need $# 2
        xml result "$@"
        ;;
    blame)
        need $# 3
        blame "$@"
        ;;
    enforced)
        need $# 1
        enforced "$@"
        ;;
    *) usage ;;
esac
