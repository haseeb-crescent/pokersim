//! Aggregated stats from per-hand seat flags.

use engine::stats::SeatFlags;
use serde_json::{json, Value};

#[derive(Default, Clone, Debug)]
pub struct Agg {
    pub hands: u64,
    pub vpip: u64,
    pub pfr: u64,
    pub tb_opp: u64,
    pub tb: u64,
    pub f3_opp: u64,
    pub f3: u64,
    pub cb_opp: u64,
    pub cb: u64,
    pub fcb_opp: u64,
    pub fcb: u64,
    pub saw_flop: u64,
    pub wtsd: u64,
    pub wsd: u64,
    pub bets: u64,
    pub raises: u64,
    pub calls: u64,
    pub net: f64,
    pub net_adj: f64,
    pub sumsq: f64,
}

impl Agg {
    pub fn add(&mut self, f: &SeatFlags) {
        if !f.dealt {
            return;
        }
        self.hands += 1;
        self.vpip += f.vpip as u64;
        self.pfr += f.pfr as u64;
        self.tb_opp += f.three_bet_opp as u64;
        self.tb += f.three_bet as u64;
        self.f3_opp += f.fold_to_3bet_opp as u64;
        self.f3 += f.fold_to_3bet as u64;
        self.cb_opp += f.cbet_opp as u64;
        self.cb += f.cbet as u64;
        self.fcb_opp += f.fold_to_cbet_opp as u64;
        self.fcb += f.fold_to_cbet as u64;
        self.saw_flop += f.saw_flop as u64;
        self.wtsd += f.wtsd as u64;
        self.wsd += f.wsd as u64;
        self.bets += f.post_bets as u64;
        self.raises += f.post_raises as u64;
        self.calls += f.post_calls as u64;
        let bb = f.net as f64 / 100.0;
        self.net += bb;
        self.net_adj += f.net_allin_adj / 100.0;
        self.sumsq += bb * bb;
    }

    pub fn merge(&mut self, o: &Agg) {
        self.hands += o.hands;
        self.vpip += o.vpip;
        self.pfr += o.pfr;
        self.tb_opp += o.tb_opp;
        self.tb += o.tb;
        self.f3_opp += o.f3_opp;
        self.f3 += o.f3;
        self.cb_opp += o.cb_opp;
        self.cb += o.cb;
        self.fcb_opp += o.fcb_opp;
        self.fcb += o.fcb;
        self.saw_flop += o.saw_flop;
        self.wtsd += o.wtsd;
        self.wsd += o.wsd;
        self.bets += o.bets;
        self.raises += o.raises;
        self.calls += o.calls;
        self.net += o.net;
        self.net_adj += o.net_adj;
        self.sumsq += o.sumsq;
    }

    pub fn bb100(&self) -> f64 {
        if self.hands == 0 { 0.0 } else { 100.0 * self.net / self.hands as f64 }
    }

    /// 95% confidence half-width of bb/100.
    pub fn ci95(&self) -> f64 {
        if self.hands < 2 {
            return 0.0;
        }
        let n = self.hands as f64;
        let mean = self.net / n;
        let var = (self.sumsq / n - mean * mean).max(0.0);
        1.96 * var.sqrt() * 100.0 / n.sqrt()
    }

    pub fn json(&self) -> Value {
        let pct = |a: u64, b: u64| if b == 0 { Value::Null } else { json!((1000.0 * a as f64 / b as f64).round() / 10.0) };
        json!({
            "hands": self.hands,
            "vpip": pct(self.vpip, self.hands),
            "pfr": pct(self.pfr, self.hands),
            "three_bet": pct(self.tb, self.tb_opp),
            "fold_to_3bet": pct(self.f3, self.f3_opp),
            "cbet": pct(self.cb, self.cb_opp),
            "fold_to_cbet": pct(self.fcb, self.fcb_opp),
            "wtsd": pct(self.wtsd, self.saw_flop),
            "wsd": pct(self.wsd, self.wtsd),
            "af": if self.calls == 0 { Value::Null } else { json!(((self.bets + self.raises) as f64 / self.calls as f64 * 100.0).round() / 100.0) },
            "net_bb": (self.net * 100.0).round() / 100.0,
            "bb100": (self.bb100() * 100.0).round() / 100.0,
            "bb100_adj": if self.hands == 0 { 0.0 } else { (10000.0 * self.net_adj / self.hands as f64).round() / 100.0 },
            "ci95": (self.ci95() * 100.0).round() / 100.0,
            "samples": {"three_bet": self.tb_opp, "fold_to_3bet": self.f3_opp, "cbet": self.cb_opp, "fold_to_cbet": self.fcb_opp, "wtsd": self.saw_flop},
        })
    }
}
