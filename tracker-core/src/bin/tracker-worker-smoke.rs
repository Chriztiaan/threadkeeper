//! Explicit live acceptance check for automatic startup, always using a new isolated directory.
use anyhow::{Context, Result, bail};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use tracker_core::{Backend, BackendOptions, config::Config};
fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let repo = args
        .next()
        .context("Usage: tracker-worker-smoke owner/repo NEW_DATA_DIRECTORY")?;
    let directory = PathBuf::from(args.next().context("Pass a new data directory")?);
    if directory.join("tracker.sqlite").exists() {
        bail!("Use a new isolated data directory");
    }
    let mut config = Config::load(&directory, None)?;
    config.repositories.retain(|r| r.name == repo);
    config.validate()?;
    tracker_core::repository_config::write_file(
        &directory.join("repositories.json"),
        &config.repositories,
    )?;
    std::fs::write(
        directory.join("tracker.config.json"),
        serde_json::to_vec_pretty(&config)?,
    )?;
    let backend = Backend::start(BackendOptions {
        directory,
        legacy: None,
        config: None,
        token: None,
        discover_credentials: true,
        polling: true,
    })?;
    let result = (|| -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            let snapshot = backend.snapshot()?;
            if snapshot["backend"]["polling"] != true {
                bail!("GitHub credentials unavailable");
            }
            if snapshot["total"].as_u64().unwrap_or(0) > 0 && snapshot["pending"]["total"] == 0 {
                println!(
                    "{}",
                    serde_json::json!({"automaticStartup":true,"nodeRequired":false,"external":snapshot["total"],"pending":snapshot["pending"]["total"],"repository":repo})
                );
                return Ok(());
            }
            if Instant::now() > deadline {
                bail!("Startup import did not finish within the bounded smoke window");
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    })();
    backend.shutdown();
    result
}
