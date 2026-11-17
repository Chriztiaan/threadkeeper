use std::path::PathBuf;
use tracker_core::{Backend, BackendOptions};
fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut directory = None;
    let mut legacy = None;
    let mut output = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => directory = args.next().map(PathBuf::from),
            "--import-db" => legacy = args.next().map(PathBuf::from),
            "--snapshot-output" => output = args.next().map(PathBuf::from),
            _ => anyhow::bail!("Unknown argument: {arg}"),
        }
    }
    let backend = Backend::start(BackendOptions {
        directory: directory.ok_or_else(|| {
            anyhow::anyhow!("Pass --data-dir PATH (inspection always runs offline)")
        })?,
        legacy,
        config: None,
        token: None,
        discover_credentials: false,
        polling: false,
    })?;
    let result = (|| -> anyhow::Result<()> {
        let data = backend.snapshot()?;
        if let Some(path) = output {
            std::fs::write(path, serde_json::to_vec_pretty(&data)?)?;
        }
        println!(
            "{}",
            serde_json::json!({"external":data["total"],"maintainers":data["maintainerCount"],"repositories":data["repositories"],"pens":data["pens"].as_array().into_iter().flatten().map(|p|serde_json::json!({"id":p["id"],"total":p["total"]})).collect::<Vec<_>>(),"sync":data["sync"],"backend":data["backend"]})
        );
        if let Some(key) = data["pens"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|p| p["items"].as_array().into_iter().flatten())
            .find_map(|i| i["id"].as_str())
        {
            let detail = backend.detail(key.to_owned())?;
            println!("Detail OK: {} #{}", detail["repo"], detail["number"]);
        }
        Ok(())
    })();
    backend.shutdown();
    result
}
