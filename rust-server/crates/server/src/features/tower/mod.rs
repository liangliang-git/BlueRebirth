//! Tower feature boundary.

#[path = "service.rs"]
pub(crate) mod service;

#[cfg(test)]
pub(crate) use service::{activity_tower_payload_typed, tower_info_payload_typed};
pub(crate) use service::{handle_activity_typed, handle_typed};
