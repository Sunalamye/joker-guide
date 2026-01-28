//! 服務層模組
//!
//! 提供 gRPC 服務所需的狀態管理、觀測構建、動作遮罩和計分功能

#![allow(unused_imports)]

pub mod action_mask;
pub mod observation;
pub mod scoring;
pub mod state;

pub use action_mask::action_mask_from_state;
pub use observation::observation_from_state;
pub use scoring::{build_selected_hand, calculate_play_score, CardScoreResult};
pub use state::EnvState;

#[cfg(test)]
mod integration_tests;
