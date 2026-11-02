//! Bot personalities: explicit distortions of the solver / chart strategy.
//!
//! Every bot starts from the strategy for its own actual hand (preflop chart or postflop
//! solve) and bends it with these parameters:
//! - `range_width`: scales how many hands it plays when unopened or facing an open
//!   (hands are added or removed at the margin, strongest first in / weakest first out)
//! - `limp`: share of its unopened non-blind entries that limp instead of raising
//! - `aggression`: multiplies bet/raise probability relative to check/call
//! - `fold_to_pressure`: multiplies fold probability when facing a bet or a 3-bet+
//! - `bluff`: multiplies bet/raise probability with weak hands (low equity / low rank)
//! - `size_bias`: shifts bet sizing toward small (-1) or large (+1) sizes
//! - `postflop_call`: extra stickiness facing bets with medium hands (calling stations)

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub description: String,
    pub range_width: f64,
    pub limp: f64,
    pub aggression: f64,
    pub fold_to_pressure: f64,
    pub bluff: f64,
    pub size_bias: f64,
    pub postflop_call: f64,
    /// the Hunter adapts to the hero's observed tendencies with node-locked solves
    pub hunter: bool,
    /// Session mode: tops back up to 100bb when below it (regs); otherwise lets the stack ride
    #[serde(default)]
    pub top_up: bool,
    /// target stat signature (percent), checked by the bot-vs-bot calibration test
    pub expected: Vec<(String, f64)>,
}

impl Profile {
    pub fn is_gto(&self) -> bool {
        self.id == "gto"
    }

    /// Parameters as a name -> value map for the UI.
    pub fn params(&self) -> Vec<(String, f64)> {
        vec![
            ("range_width".into(), self.range_width),
            ("limp".into(), self.limp),
            ("aggression".into(), self.aggression),
            ("fold_to_pressure".into(), self.fold_to_pressure),
            ("bluff".into(), self.bluff),
            ("size_bias".into(), self.size_bias),
            ("postflop_call".into(), self.postflop_call),
        ]
    }
}

fn p(id: &str, name: &str, desc: &str, w: f64, limp: f64, agg: f64, ftp: f64, bluff: f64, size: f64, call: f64, expected: &[(&str, f64)]) -> Profile {
    Profile {
        id: id.into(),
        name: name.into(),
        description: desc.into(),
        range_width: w,
        limp,
        aggression: agg,
        fold_to_pressure: ftp,
        bluff,
        size_bias: size,
        postflop_call: call,
        hunter: false,
        top_up: matches!(id, "gto" | "lag"),
        expected: expected.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
    }
}

/// The built-in profiles. Expected stats are typical 6-max archetype numbers (VPIP / PFR /
/// 3-bet / fold to c-bet / aggression factor / WTSD).
pub fn builtin() -> Vec<Profile> {
    let mut v = vec![
        p("gto", "GTO Reg", "Plays the solver strategy for its hand with no distortion.", 1.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0,
          &[("vpip", 22.0), ("pfr", 16.0), ("three_bet", 9.0), ("fold_to_cbet", 45.0), ("af", 2.0), ("wtsd", 33.0)]),
        p("nit", "Nit", "Plays a narrow range, folds to pressure, rarely bluffs, bets small with value.", 0.72, 0.0, 0.8, 1.15, 0.3, -0.5, 0.0,
          &[("vpip", 14.0), ("pfr", 11.0), ("three_bet", 4.0), ("fold_to_cbet", 58.0), ("af", 2.0), ("wtsd", 22.0)]),
        p("station", "Calling Station", "Plays too many hands, almost never folds, rarely raises.", 2.2, 0.8, 0.2, 0.55, 0.4, -0.3, 0.6,
          &[("vpip", 45.0), ("pfr", 9.0), ("three_bet", 2.5), ("fold_to_cbet", 22.0), ("af", 0.8), ("wtsd", 40.0)]),
        p("maniac", "Maniac", "Plays a wide range very aggressively with big sizes and lots of bluffs.", 3.0, 0.0, 1.6, 0.7, 1.5, 0.9, 0.2,
          &[("vpip", 55.0), ("pfr", 42.0), ("three_bet", 18.0), ("fold_to_cbet", 30.0), ("af", 5.0), ("wtsd", 32.0)]),
        p("fish", "Loose-Passive Fish", "Limps and calls a lot, folds too much to big bets later, bluffs rarely.", 2.8, 0.7, 0.45, 1.05, 0.3, -0.2, 0.25,
          &[("vpip", 40.0), ("pfr", 10.0), ("three_bet", 3.0), ("fold_to_cbet", 45.0), ("af", 1.2), ("wtsd", 33.0)]),
        p("lag", "Loose-Aggressive Reg", "Plays wider than GTO and applies pressure, but stays balanced-ish.", 1.8, 0.0, 1.35, 0.9, 1.25, 0.3, 0.0,
          &[("vpip", 30.0), ("pfr", 25.0), ("three_bet", 11.0), ("fold_to_cbet", 35.0), ("af", 3.3), ("wtsd", 27.0)]),
    ];
    let mut hunter = v[0].clone();
    hunter.id = "hunter".into();
    hunter.name = "The Hunter".into();
    hunter.description = "Starts as a GTO reg, tracks your frequencies by spot and node-locks them into the solver to play a best response once it has enough data.".into();
    hunter.hunter = true;
    v.push(hunter);
    v
}

pub fn by_id(id: &str) -> Option<Profile> {
    builtin().into_iter().find(|p| p.id == id)
}
