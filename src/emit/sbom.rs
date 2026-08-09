//! The pinned payloads one target carries, as the SPDX packages a scan of the
//! built image cannot see.

use crate::emit::json::Json;
use crate::emit::plan::{of_target, pinned};
use crate::model::asset::Asset;
use crate::model::image::List;
use crate::model::module::Module;

/// None when nothing publishes under that name.
pub fn build(list: &List, target: &str) -> Option<Json> {
    let (_, _, entries) = of_target(list, target)?;
    let modules: Vec<&Module> = entries.iter().filter_map(|e| e.module.as_ref()).collect();
    let assets: Vec<(String, &Module, &Asset, String)> = pinned(&modules)
        .into_iter()
        .filter_map(|(module, asset)| {
            let url = asset.url_resolved()?;
            let id = format!(
                "SPDXRef-Package-asset-{}-{}",
                module.path.replace('/', "-"),
                asset.name
            );
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
        (
            "checksums",
            Json::array([Json::object([
                ("algorithm", Json::string("SHA256")),
                (
                    "checksumValue",
                    Json::string(asset.sha256.clone().unwrap_or_default()),
                ),
            ])]),
        ),
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
    ];
    if let Some(version) = &asset.version {
        fields.push(("versionInfo", Json::string(version)));
    }
    Json::object(fields)
}
