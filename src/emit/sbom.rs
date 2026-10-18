//! The pinned payloads one target carries, as the SPDX packages a scan of the
//! built image cannot see.

use crate::emit::plan::{of_target, pinned};
use crate::model::asset::Asset;
use crate::model::image::List;
use crate::model::module::Module;
use common::json::Json;

/// Names an asset's SPDX package. An SPDX id takes letters, digits, `.` and `-`
/// alone, and a module path also holds `/` and the `@` of a renamed copy.
fn spdx_id(module: &str, asset: &str) -> String {
    format!("SPDXRef-Package-asset-{module}-{asset}")
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
    let modules: Vec<&Module> = entries.iter().filter_map(|e| e.module.as_ref()).collect();
    let assets: Vec<(String, &Module, &Asset, String)> = pinned(&modules)
        .into_iter()
        .filter_map(|(module, asset)| {
            let url = asset.pin.url_resolved()?;
            let id = spdx_id(&module.path, &asset.name);
            Some((id, module, asset, url))
        })
        .collect();

    Some(Json::object([
        (
            "packages",
            Json::array(
                assets
                    .iter()
                    .map(|(id, module, asset, url)| package(id, module, asset, url)),
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

fn package(id: &str, module: &Module, asset: &Asset, url: &str) -> Json {
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
                "Pinned build input, declared by the {} module",
                module.path
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
    fn a_renamed_nested_module_names_a_valid_spdx_id() {
        assert_eq!(
            super::spdx_id("kernels/cachyos@1", "kernel.tar"),
            "SPDXRef-Package-asset-kernels-cachyos-1-kernel.tar"
        );
    }
}
