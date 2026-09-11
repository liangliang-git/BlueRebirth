#![allow(dead_code)]

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SupportSettlement {
    pub(super) reward_type: i32,
    pub(super) hero_ids: Vec<u64>,
    pub(super) base_rewards: Vec<(i32, i32, i32)>,
    pub(super) random_rewards: Vec<(i32, i32, i32)>,
}
