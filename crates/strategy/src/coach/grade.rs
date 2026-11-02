//! Grades for one decision.

use super::facts::FactSheet;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActionMix {
    pub label: String,
    pub freq: f64,
    pub ev_bb: Option<f64>,
    pub range_freq: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExploitView {
    pub profile: String,
    pub actions: Vec<ActionMix>,
    pub best: String,
    pub note: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionGrade {
    pub hand_id: i64,
    pub index: usize,
    pub street: String,
    pub spot: String,
    pub spot_key: String,
    pub kind: String,
    pub hero_hand: String,
    pub board: Vec<String>,
    pub actions: Vec<ActionMix>,
    pub chosen: String,
    pub chosen_freq: Option<f64>,
    pub best: Option<String>,
    pub ev_loss_bb: Option<f64>,
    pub grade: String,
    pub precision_pct: Option<f64>,
    pub note: Option<String>,
    pub facts: Option<FactSheet>,
    pub explanation: Option<String>,
    pub exploit: Option<ExploitView>,
}

/// Grade from solver EVs: EV loss relative to the pot.
pub fn label_from_ev(chosen_freq: f64, is_top_freq: bool, ev_loss_bb: f64, pot_bb: f64) -> &'static str {
    let loss_pct = if pot_bb > 0.0 { ev_loss_bb / pot_bb } else { 0.0 };
    if is_top_freq || loss_pct < 0.005 || ev_loss_bb < 0.02 {
        "Best"
    } else if chosen_freq >= 0.1 || loss_pct < 0.02 {
        "Correct"
    } else if loss_pct < 0.05 {
        "Inaccuracy"
    } else if loss_pct < 0.12 {
        "Mistake"
    } else {
        "Blunder"
    }
}

/// Grade from chart frequency alone (preflop: charts carry no EVs).
pub fn label_from_freq(chosen_freq: f64, is_top_freq: bool) -> &'static str {
    if is_top_freq || chosen_freq >= 0.5 {
        "Best"
    } else if chosen_freq >= 0.15 {
        "Correct"
    } else if chosen_freq >= 0.03 {
        "Inaccuracy"
    } else if chosen_freq >= 0.005 {
        "Mistake"
    } else {
        "Blunder"
    }
}
