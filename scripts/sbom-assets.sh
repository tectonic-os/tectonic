#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

target="${1:?usage: sbom-assets.sh <target>}"

./scripts/manifest.sh assets "$target" | jq -Rs '
    [ split("\n")[]
      | select(length > 0)
      | split("|")
      | { module: .[0], name: .[1], version: .[3], sha256: .[4], url: .[6] }
      | select(.url != "")
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
            + (if .version == "" then {} else { versionInfo: .version } end)
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
