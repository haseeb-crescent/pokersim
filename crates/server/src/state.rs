//! Shared server state: loaded strategy resources, the database, the live table and sims.

use crate::db::Db;
use crate::llm::Llm;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use strategy::bots::profile::{builtin, Profile};
use strategy::postflop::SolveCache;
use strategy::preflop::charts::Charts;
use strategy::preflop::equity_table::EquityTable;
use tokio::sync::{broadcast, mpsc};

pub struct Resources {
    pub root: PathBuf,
    /// the 100bb charts
    pub charts: Arc<Charts>,
    /// all chart depths available (bb, charts), including 100bb
    pub depth_charts: Vec<(u32, Arc<Charts>)>,
    pub eq: EquityTable,
    pub strength: Vec<f64>,
    pub cache: Arc<SolveCache>,
    pub profiles: Vec<Profile>,
    /// the preflop model's realization factors (used by off-chart heuristic grades)
    pub realization: strategy::preflop::cfr::Realization,
}

impl Resources {
    pub fn load(root: PathBuf) -> anyhow::Result<Resources> {
        let data = root.join("data/preflop");
        let charts = Arc::new(Charts::load(&data.join("charts.json"))?);
        let mut depth_charts = vec![(100u32, charts.clone())];
        if let Ok(rd) = std::fs::read_dir(&data) {
            for f in rd.flatten() {
                let name = f.file_name().to_string_lossy().to_string();
                if let Some(d) = name.strip_prefix("charts_").and_then(|x| x.strip_suffix("bb.json")).and_then(|x| x.parse::<u32>().ok()) {
                    if let Ok(c) = Charts::load(&f.path()) {
                        depth_charts.push((d, Arc::new(c)));
                    }
                }
            }
        }
        depth_charts.sort_by_key(|x| x.0);
        let eq = EquityTable::load(&data.join("equity169.bin"))?;
        let strength = strategy::bots::distort::preflop_strength(&eq);
        let cache = Arc::new(SolveCache::new(root.join("data/cache/solves")));
        let realization = std::fs::read_to_string(data.join("realization.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_else(strategy::preflop::cfr::Realization::heuristic);
        Ok(Resources { root, charts, depth_charts, eq, strength, cache, profiles: builtin(), realization })
    }

    /// The chart whose depth is nearest (in ratio) to an effective stack in chips.
    pub fn charts_for(&self, eff_chips: i64) -> (u32, Arc<Charts>) {
        let eff = (eff_chips.max(1) as f64) / 100.0;
        self.depth_charts
            .iter()
            .min_by(|a, b| (eff / a.0 as f64).ln().abs().partial_cmp(&(eff / b.0 as f64).ln().abs()).unwrap())
            .map(|(d, c)| (*d, c.clone()))
            .unwrap_or((100, self.charts.clone()))
    }

    pub fn profile(&self, id: &str) -> Option<Profile> {
        self.profiles.iter().find(|p| p.id == id).cloned()
    }
}

pub struct TableHandle {
    pub cmd: mpsc::UnboundedSender<crate::table::TableCmd>,
    pub latest: Arc<Mutex<Option<serde_json::Value>>>,
    pub session_id: i64,
}

pub struct AppState {
    pub res: Arc<Resources>,
    pub db: Arc<Db>,
    pub llm: Option<Arc<Llm>>,
    pub table: Mutex<Option<TableHandle>>,
    pub tx: broadcast::Sender<String>,
    pub sims: Mutex<HashMap<String, Arc<Mutex<crate::sim::SimState>>>>,
    pub lab_solves: Mutex<HashMap<String, Arc<strategy::postflop::Spot>>>,
}

pub type App = Arc<AppState>;
