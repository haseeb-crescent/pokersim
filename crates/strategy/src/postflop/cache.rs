//! Disk cache of finished solves, keyed by the canonical spot key.
//!
//! Each entry is the postflop-solver game file (zstd-compressed bincode; flop solves keep only
//! the flop street) plus a small JSON sidecar with the solve status and spot summary.

use super::spot::SpotSpec;
use super::worker::SpotStatus;
use postflop_solver as ps;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub struct SolveCache {
    pub dir: PathBuf,
    pub enabled: bool,
}

#[derive(Serialize, Deserialize)]
struct Sidecar {
    status: SpotStatus,
    board: String,
    pot: i32,
    stack: i32,
    preset: String,
    combos: [f32; 2],
}

impl SolveCache {
    pub fn new(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        SolveCache { dir, enabled: true }
    }

    pub fn disabled() -> Self {
        SolveCache { dir: PathBuf::new(), enabled: false }
    }

    fn paths(&self, key: &str) -> (PathBuf, PathBuf) {
        let shard = &key[..key.len().min(6)];
        let d = self.dir.join(shard);
        (d.join(format!("{key}.bin")), d.join(format!("{key}.json")))
    }

    pub fn contains(&self, key: &str) -> bool {
        self.enabled && self.paths(key).0.exists()
    }

    pub fn load(&self, key: &str) -> Option<(ps::PostFlopGame, SpotStatus)> {
        if !self.enabled {
            return None;
        }
        let (bin, json) = self.paths(key);
        if !bin.exists() {
            return None;
        }
        let (game, _memo): (ps::PostFlopGame, String) = ps::load_data_from_file(&bin, None).ok()?;
        let status = std::fs::read_to_string(&json)
            .ok()
            .and_then(|s| serde_json::from_str::<Sidecar>(&s).ok())
            .map(|s| s.status)
            .unwrap_or_default();
        Some((game, status))
    }

    pub fn save(&self, key: &str, game: &mut ps::PostFlopGame, street: ps::BoardState, status: &SpotStatus, spec: &SpotSpec) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        let (bin, json) = self.paths(key);
        std::fs::create_dir_all(bin.parent().unwrap()).map_err(|e| e.to_string())?;
        game.set_target_storage_mode(street)?;
        let tmp = bin.with_extension("tmp");
        ps::save_data_to_file(game, key, &tmp, Some(3))?;
        std::fs::rename(&tmp, &bin).map_err(|e| e.to_string())?;
        let side = Sidecar {
            status: status.clone(),
            board: engine::card::cards_to_string(&spec.board),
            pot: spec.pot,
            stack: spec.stack,
            preset: format!("{:?}", spec.preset),
            combos: [spec.ranges[0].combos(0), spec.ranges[1].combos(0)],
        };
        std::fs::write(json, serde_json::to_string(&side).unwrap()).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Number of cached solves and their total size on disk.
    pub fn stats(&self) -> (usize, u64) {
        let mut n = 0;
        let mut bytes = 0;
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            for shard in rd.flatten() {
                if let Ok(files) = std::fs::read_dir(shard.path()) {
                    for f in files.flatten() {
                        if f.path().extension().map(|e| e == "bin").unwrap_or(false) {
                            n += 1;
                            bytes += f.metadata().map(|m| m.len()).unwrap_or(0);
                        }
                    }
                }
            }
        }
        (n, bytes)
    }
}
