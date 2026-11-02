//! Preflop strategy: the exact class equity table, the abstract 6-max preflop game and its
//! CFR solver, calibration against postflop-solver, and the chart data the bots and the coach
//! read.
pub mod calibrate;
pub mod cfr;
pub mod charts;
pub mod equity_table;
pub mod game;
