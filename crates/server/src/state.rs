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
    pub charts: Charts,
    pub eq: EquityTable,
    pub strength: Vec<f64>,
    pub cache: Arc<SolveCache>,
    pub profiles: Vec<Profile>,
}

impl Resources {
    pub fn load(root: PathBuf) -> anyhow::Result<Resources> {
        let data = root.join("data/preflop");
        let charts = Charts::load(&data.join("charts.json"))?;
        let eq = EquityTable::load(&data.join("equity169.bin"))?;
        let strength = strategy::bots::distort::preflop_strength(&eq);
        let cache = Arc::new(SolveCache::new(root.join("data/cache/solves")));
        Ok(Resources { root, charts, eq, strength, cache, profiles: builtin() })
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
