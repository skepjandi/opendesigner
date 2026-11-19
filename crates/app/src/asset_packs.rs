//! The asset packs and recommended MCP servers as the Connect tab shows them. The logic is in
//! `od_agent::assets`; this only turns it into what the window reads
//! (`docs/wiki/store-assets.md#asset-packs`).

use od_agent::assets::{catalog, packs};
use serde_json::{Value, json};

type Res<T> = Result<T, String>;

/// Every pack with its state, the requests agents made and what the cache takes.
pub fn packs_json() -> String {
    match packs::root() {
        Ok(root) => json!({
            "packs": packs::states(&root),
            "installed_bytes": packs::total_bytes(&root),
            "cap_bytes": packs::TOTAL_CAP_BYTES,
            "requests": packs::requests(&root),
        }),
        Err(e) => json!({"packs": [], "error": e.to_string()}),
    }
    .to_string()
}

/// The user's click on Download: approve and fetch in the background.
pub fn install(id: &str) -> Res<()> {
    let root = packs::root().map_err(|e| e.to_string())?;
    packs::approve_and_install_in_background(root, id.to_string()).map_err(|e| e.to_string())
}

/// The user's click on Unload.
pub fn unload(id: &str) -> Res<()> {
    let root = packs::root().map_err(|e| e.to_string())?;
    packs::unload(&root, id)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The user's click on Dismiss for an agent's request.
pub fn dismiss(id: &str) -> Res<()> {
    let root = packs::root().map_err(|e| e.to_string())?;
    packs::dismiss(&root, id);
    Ok(())
}

/// The servers worth adding, each with the command line the Add button saves.
pub fn recommended_json() -> String {
    Value::Array(
        catalog::RECOMMENDED_SERVERS
            .iter()
            .map(|s| {
                json!({
                    "name": s.name,
                    "command": std::iter::once(s.command).chain(s.args.iter().copied()).collect::<Vec<_>>().join(" "),
                    "env": s.env.join("\n"),
                    "about": s.about,
                    "licence": s.licence,
                })
            })
            .collect(),
    )
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::servers;

    #[test]
    fn every_recommended_server_is_one_the_servers_list_accepts() {
        let rows: Vec<Value> = serde_json::from_str(&recommended_json()).unwrap();
        assert_eq!(rows.len(), catalog::RECOMMENDED_SERVERS.len());
        for row in rows {
            let server = servers::parse(
                row["name"].as_str().unwrap(),
                row["command"].as_str().unwrap(),
                row["env"].as_str().unwrap(),
                &[],
            )
            .unwrap();
            assert!(!server.command.is_empty());
        }
    }

    #[test]
    fn the_packs_are_listed_with_sizes() {
        let v: Value = serde_json::from_str(&packs_json()).unwrap();
        assert!(v["packs"].as_array().unwrap().len() >= 10);
        assert!(v["packs"][0]["download_bytes"].as_u64().unwrap() > 0);
    }
}
