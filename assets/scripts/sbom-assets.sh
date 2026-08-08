#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

target="${1:?usage: sbom-assets.sh <target>}"

./scripts/tect.sh plan --json | jq --arg target "$target" '
    [ .images[].targets[]
      | select(.name == $target)
      | .assets[]
      | select(.url != null)
      | { module, name, version, sha256: (.sha256 // ""), url }
      | . + { id: ("SPDXRef-Package-asset-"
                   + (.module | gsub("/"; "-")) + "-" + .name) }
    ] as $assets
    | {
        packages: [ $assets[]
          | {
              SPDXID: .id,
              name: .name,
              downloadLocation: .url,
              filesAnalyzed: false,
              checksums: [ { algorithm: "SHA256", checksumValue: .sha256 } ],
              licenseConcluded: "NOASSERTION",
              licenseDeclared: "NOASSERTION",
              copyrightText: "NOASSERTION",
              supplier: "NOASSERTION",
              comment: ("Pinned build input, declared by the " + .module + " module")
            }
            + (if .version == null then {} else { versionInfo: .version } end)
        ],
        relationships: [ $assets[]
          | {
              spdxElementId: "SPDXRef-DOCUMENT",
              relationshipType: "DESCRIBES",
              relatedSpdxElement: .id
            }
        ]
      }
'
