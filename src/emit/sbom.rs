//! The pinned payloads one target carries, as the SPDX packages a scan of the
//! built image cannot see.

use crate::emit::plan::{of_target, pinned};
use crate::model::asset::Asset;
use crate::model::component::Component;
use crate::model::image::List;
use common::json::Json;

/// Names an asset's SPDX package. An SPDX id takes letters, digits, `.` and `-`
/// alone, and a component path also holds `/` and the `@` of a renamed copy.
fn spdx_id(component: &str, asset: &str) -> String {
    format!("SPDXRef-Package-asset-{component}-{asset}")
        .chars()
        .map(
            |c| match c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                true => c,
                false => '-',
            },
        )
        .collect()
}

/// None when nothing publishes under that name.
pub fn build(list: &List, target: &str) -> Option<Json> {
    let (_, _, entries) = of_target(list, target)?;
    let components: Vec<&Component> = entries
        .iter()
        .filter_map(|e| e.component.as_ref())
        .collect();
    let assets: Vec<(String, &Component, &Asset, String)> = pinned(&components)
        .into_iter()
        .filter_map(|(component, asset)| {
            let url = asset.pin.url_resolved()?;
            let id = spdx_id(&component.path, &asset.name);
            Some((id, component, asset, url))
        })
        .collect();

    Some(Json::object([
        (
            "packages",
            Json::array(
                assets
                    .iter()
                    .map(|(id, component, asset, url)| package(id, component, asset, url)),
            ),
        ),
        (
            "relationships",
            Json::array(assets.iter().map(|(id, ..)| {
                Json::object([
                    ("spdxElementId", Json::string("SPDXRef-DOCUMENT")),
                    ("relationshipType", Json::string("DESCRIBES")),
                    ("relatedSpdxElement", Json::string(id)),
                ])
            })),
        ),
    ]))
}

fn package(id: &str, component: &Component, asset: &Asset, url: &str) -> Json {
    let mut fields = vec![
        ("SPDXID", Json::string(id)),
        ("name", Json::string(&asset.name)),
        ("downloadLocation", Json::string(url)),
        ("filesAnalyzed", Json::Bool(false)),
    ];
    // A cloned asset declares no sha256, because the commit its version names
    // verifies it. An empty checksum would claim a hash nothing checked.
    fields.extend(asset.pin.sha256.as_ref().map(|sha256| {
        (
            "checksums",
            Json::array([Json::object([
                ("algorithm", Json::string("SHA256")),
                ("checksumValue", Json::string(sha256)),
            ])]),
        )
    }));
    fields.extend([
        ("licenseConcluded", Json::string("NOASSERTION")),
        ("licenseDeclared", Json::string("NOASSERTION")),
        ("copyrightText", Json::string("NOASSERTION")),
        ("supplier", Json::string("NOASSERTION")),
        (
            "comment",
            Json::string(format!(
                "Pinned build input, declared by the {} component",
                component.path
            )),
        ),
    ]);
    if let Some(version) = &asset.pin.version {
        fields.push(("versionInfo", Json::string(version)));
    }
    Json::object(fields)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_renamed_nested_component_names_a_valid_spdx_id() {
        assert_eq!(
            super::spdx_id("kernels/cachyos@1", "kernel.tar"),
            "SPDXRef-Package-asset-kernels-cachyos-1-kernel.tar"
        );
    }
}
