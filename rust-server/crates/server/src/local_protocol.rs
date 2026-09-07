use serde_json::{json, Value};

use super::*;

/// Dispatches one local JSON protocol message. Payload is intentionally ignored for login,
/// matching the C# server's selected-launcher-profile behavior.
pub fn dispatch(
    state: &mut ServerState,
    message_type: &str,
    payload: Value,
) -> Result<Value, ServerError> {
    match message_type {
        "login" => Ok(json!({
            "profileId": state.profile_id,
            "version": state.version,
        })),
        "state" => Ok(json!({
            "profileId": state.profile_id,
            "name": state.name,
            "level": state.level,
            "fuel": state.fuel,
            "coins": state.coins,
            "ships": state.ships,
            "formation": state.formation,
            "completedStages": state.completed_stages,
        })),
        "set_formation" => {
            let ship_ids = parse_ship_ids(&payload)?;
            if ship_ids.is_empty()
                || ship_ids.len() > 6
                || ship_ids
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len()
                    != ship_ids.len()
                || ship_ids
                    .iter()
                    .any(|id| state.ships.iter().all(|ship| ship.id != *id))
            {
                return Err(ServerError::InvalidMessage(
                    "Formation contains invalid ships".to_owned(),
                ));
            }
            state.formation = Formation { ship_ids };
            Ok(state.snapshot())
        }
        "enter_stage" => {
            let stage = stage_from_payload(&payload, state)?;
            Ok(json!(stage))
        }
        "battle_result" => {
            let stage = stage_from_payload(&payload, state)?;
            let win = payload
                .get("win")
                .and_then(Value::as_bool)
                .ok_or_else(|| ServerError::InvalidMessage("Missing win".to_owned()))?;
            state.fuel -= stage.fuel_cost;
            let coins_gained = if win {
                scale_reward(stage.coin_reward, state.drop_multiplier)
            } else {
                0
            };
            state.coins += coins_gained;
            if win {
                state.completed_stages = state.completed_stages.max(stage.id);
            }
            let outcome = BattleOutcome {
                victory: win,
                fuel_spent: stage.fuel_cost,
                coins_gained,
                completed_stages: state.completed_stages,
                message: if win { "Victory" } else { "Defeat" }.to_owned(),
            };
            Ok(json!({"state": state.snapshot(), "outcome": outcome}))
        }
        other => Err(ServerError::UnknownMessage(other.to_owned())),
    }
}

fn parse_ship_ids(payload: &Value) -> Result<Vec<i32>, ServerError> {
    payload
        .get("shipIds")
        .and_then(Value::as_array)
        .ok_or_else(|| ServerError::InvalidMessage("Missing shipIds".to_owned()))?
        .iter()
        .map(|value| {
            value
                .as_i64()
                .and_then(|id| i32::try_from(id).ok())
                .ok_or_else(|| ServerError::InvalidMessage("Invalid shipIds".to_owned()))
        })
        .collect()
}

fn stage_from_payload(payload: &Value, state: &ServerState) -> Result<Stage, ServerError> {
    let stage_id = payload
        .get("stageId")
        .and_then(Value::as_i64)
        .and_then(|id| i32::try_from(id).ok())
        .ok_or_else(|| ServerError::InvalidMessage("Missing stageId".to_owned()))?;
    let stage = StageCatalog::get(stage_id)?;
    if state.formation.ship_ids.is_empty() {
        return Err(ServerError::InvalidMessage("Formation is empty".to_owned()));
    }
    if state.fuel < stage.fuel_cost {
        return Err(ServerError::InvalidMessage("Not enough fuel".to_owned()));
    }
    Ok(stage)
}

struct StageCatalog;

impl StageCatalog {
    fn get(id: i32) -> Result<Stage, ServerError> {
        match id {
            1 => Ok(Stage {
                id: 1,
                name: "Tutorial Waters".to_owned(),
                enemies: vec![Ship {
                    id: 9001,
                    name: "Training Target".to_owned(),
                    level: 1,
                    power: 20,
                }],
                fuel_cost: 10,
                coin_reward: 100,
            }),
            _ => Err(ServerError::InvalidMessage("Stage not found".to_owned())),
        }
    }
}
