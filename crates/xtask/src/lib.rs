//! VRT adversarial proof harness (`cargo xtask proof`).
//!
//! Burden of proof is on VRT: this harness executes real baseline command sets
//! against real fixtures, measures wall-clock truth, and refuses to let skipped
//! work, stale evidence, or release over-claims pass unnoticed (Canvas §1).

pub mod agent;
pub mod detectors;
pub mod model;
pub mod proof;
pub mod report;
pub mod runner;
pub mod scenarios;
