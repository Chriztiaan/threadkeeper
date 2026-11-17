use serde_json::json;
use std::{collections::HashMap, path::PathBuf};
use tracker_core::{
    config::{Config, token},
    github::{GitHub, Source},
    store::{Store, human, id, string},
};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let name = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("Usage: tracker-smoke owner/repo NEW_DATA_DIRECTORY"))?;
    let directory = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("Pass a new data directory"))?,
    );
    if directory.join("tracker.sqlite").exists() {
        anyhow::bail!("Smoke test requires a new database");
    }
    let mut config = Config::load(&directory, None)?;
    config.repositories.retain(|r| r.name == name);
    config.validate()?;
    tracker_core::repository_config::write_file(
        &directory.join("repositories.json"),
        &config.repositories,
    )?;
    let mut source =
        GitHub::new(token().ok_or_else(|| {
            anyhow::anyhow!("Provide GITHUB_TOKEN")
        })?)?;
    let store = Store::open(&directory.join("tracker.sqlite")).await?;
    store.configure(&config).await?;
    let repo = store
        .save_repository(&name, &source.repository(&name).await?)
        .await?;
    let page = source.issues(&name, "open", 1, None, "created").await?;
    let external = page
        .items
        .into_iter()
        .filter(|i| {
            i["user"].is_null()
                || human(&i["user"])
                    && !config
                        .maintainers
                        .iter()
                        .any(|m| m.github_id == id(&i["user"]["id"]))
        })
        .collect::<Vec<_>>();
    let mut sample = vec![];
    for pr in [false, true] {
        if let Some(item) = external.iter().find(|i| !i["pull_request"].is_null() == pr) {
            sample.push(item.clone());
        }
    }
    if sample.is_empty() {
        anyhow::bail!("No external entries in the first page; choose another repository");
    }
    let mut pulls = HashMap::new();
    for item in &sample {
        if !item["pull_request"].is_null() {
            pulls.insert(
                item["number"].as_u64().unwrap(),
                source.pull(&name, item["number"].as_u64().unwrap()).await?,
            );
        }
    }
    store.save_items(&repo, &sample, &pulls).await?;
    for item in &sample {
        let kind = if item["pull_request"].is_null() {
            "issue"
        } else {
            "pr"
        };
        let events = source
            .engagement(&name, item["number"].as_u64().unwrap(), kind)
            .await?;
        store
            .save_engagement(&id(&item["id"]), &events, string(item, "updated_at"))
            .await?;
    }
    let snapshot = store.followup().await?;
    let count = store.one("SELECT count(*) AS n FROM ps_crud", &[]).await?;
    println!(
        "{}",
        json!({"sampled":sample.len(),"external":snapshot["total"],"pending":snapshot["pending"]["total"],"pendingUploads":count["n"],"nodeRequired":false})
    );
    let reopened = Store::open(&directory.join("tracker.sqlite")).await?;
    if reopened.followup().await?["total"] != snapshot["total"] {
        anyhow::bail!("Persistence verification failed");
    }
    Ok(())
}
