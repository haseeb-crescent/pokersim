//! Solver Trainer server: serves the UI and the API on http://127.0.0.1:8787.
//! Flags: --no-open (don't open the browser), --port N.

mod api;
mod coach;
mod db;
mod heuristic;
mod llm;
mod sim;
mod state;
mod stats;
mod table;
mod view;

use state::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tower_http::services::{ServeDir, ServeFile};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize()?;
    let _ = dotenvy::from_path(root.join(".env"));
    let args: Vec<String> = std::env::args().collect();
    let port: u16 = args.iter().position(|a| a == "--port").and_then(|i| args.get(i + 1)).and_then(|p| p.parse().ok()).unwrap_or(8787);
    let open = !args.iter().any(|a| a == "--no-open");
    // --db PATH keeps a separate logbook (demos, experiments); default data/trainer.sqlite
    let db_path = args.iter().position(|a| a == "--db").and_then(|i| args.get(i + 1)).map(PathBuf::from).unwrap_or_else(|| root.join("data/trainer.sqlite"));

    let res = Arc::new(Resources::load(root.clone())?);
    let db = Arc::new(db::Db::open(&db_path)?);
    let llm = llm::Llm::detect().await.map(Arc::new);
    let (tx, _) = tokio::sync::broadcast::channel(512);
    let app: App = Arc::new(AppState { res, db, llm, table: Mutex::new(None), tx, sims: Mutex::new(HashMap::new()), lab_solves: Mutex::new(HashMap::new()) });

    let dist = root.join("ui/dist");
    let static_files = ServeDir::new(&dist).fallback(ServeFile::new(dist.join("index.html")));
    let router = api::router(app.clone()).fallback_service(static_files);

    let (n, bytes) = app.res.cache.stats();
    println!("Solver Trainer");
    println!("  charts: {} nodes ({})", app.res.charts.nodes.len(), app.res.charts.meta.method);
    println!("  chart depths: {}", app.res.depth_charts.iter().map(|(d, _)| format!("{d}bb")).collect::<Vec<_>>().join(", "));
    println!("  solve cache: {n} solves, {:.1} MB", bytes as f64 / 1e6);
    println!(
        "  explanations: {}",
        match &app.llm {
            Some(l) => l.describe(),
            None => "off (log in to the claude CLI, or set ANTHROPIC_API_KEY in .env)".into(),
        }
    );
    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    let url = format!("http://{addr}");
    println!("  listening on {url}");
    if open && dist.exists() {
        let _ = std::process::Command::new("open").arg(&url).spawn();
    }
    axum::serve(listener, router).await?;
    Ok(())
}
