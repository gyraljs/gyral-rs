//! Measure the real Gyral fixture with a fresh isolate for every request.
#![forbid(unsafe_code)]
use gyral_renderer::{AppArtifacts, BundleManifest, PageId, RenderRequest, Renderer};
use serde_json::json;
use std::{io::Write, path::Path, time::Instant};

#[derive(Debug, thiserror::Error)]
enum MeasurementError {
    #[error("could not read fixture artifacts or write the report")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Manifest(#[from] gyral_renderer::ManifestError),
    #[error(transparent)]
    Artifacts(#[from] gyral_renderer::ArtifactError),
    #[error(transparent)]
    Worker(#[from] gyral_renderer::WorkerError),
    #[error(transparent)]
    Contract(#[from] gyral_renderer::ContractError),
    #[error(transparent)]
    Render(#[from] gyral_renderer::RenderError),
    #[error("could not encode the measurement report")]
    Encode(#[from] serde_json::Error),
    #[error("the fixed measurement sample was empty")]
    EmptySample,
}

fn main() -> Result<(), MeasurementError> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/axum-app/dist");
    let text = std::fs::read_to_string(root.join("manifest.json"))?;
    let manifest = BundleManifest::try_from(text.as_str())?;
    let source = std::fs::read_to_string(root.join("server.js"))?;
    let source_bytes = source.len();
    let browser = std::fs::read(
        root.join("assets")
            .join(format!("client.{}.js", manifest.client_hash().hex())),
    )?;
    let app = AppArtifacts::try_new(manifest, source, browser)?;
    let starting = Instant::now();
    let (renderer, worker) = Renderer::start(app.bundle().clone())?;
    let worker_start_us = starting.elapsed().as_micros();
    let host = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    host.block_on(async {
        let mut durations = Vec::with_capacity(50);
        let mut output_bytes = 0;
        for index in 0..50 {
            let request = RenderRequest::try_new(PageId::try_from("home".to_owned())?, json!({"name":format!("Visitor {index}"),"visits":index}))?;
            let starting = Instant::now();
            let result = renderer.render(request).await?;
            durations.push(starting.elapsed().as_micros());
            output_bytes = result.as_str().len();
        }
        let first_us = durations.first().copied().ok_or(MeasurementError::EmptySample)?;
        durations.sort_unstable();
        let lower_us = durations.get(24).copied().ok_or(MeasurementError::EmptySample)?;
        let upper_us = durations.get(25).copied().ok_or(MeasurementError::EmptySample)?;
        let median_us = (lower_us + upper_us) / 2;
        let p95_us = durations.get(47).copied().ok_or(MeasurementError::EmptySample)?;
        worker.shutdown().await?;
        let report = json!({
            "samples":50,"profile":"release","mode":"one worker, fresh isolate per render",
            "architecture":std::env::consts::ARCH,"server_bytes":source_bytes,"last_output_bytes":output_bytes,
            "worker_start_us":worker_start_us,"first_render_us":first_us,"median_us":median_us,"p95_us":p95_us,
        });
        writeln!(std::io::stdout().lock(), "{}", serde_json::to_string_pretty(&report)?)?;
        Ok(())
    })
}
