use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

pub(crate) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    match method {
        "outpost.GetOutPostInfo" => reply(method, outpost_info_payload_typed(account)),
        "outpost.UpgradeBuilding" | "outpost.DegradeBuilding" => {
            let request = match OutpostBuildingRequest::decode(request_args) {
                Ok(request) => request,
                Err(_) => return invalid("outpost building id is invalid"),
            };
            let building_id = request.building_id;
            let delta = if method.ends_with("UpgradeBuilding") {
                1
            } else {
                -1
            };
            let Some(level) = account.buildings.levels.get_mut(&building_id) else {
                return invalid("outpost building was not found");
            };
            let next = i64::from(*level).saturating_add(delta);
            if next <= 0 {
                return invalid("outpost building is at minimum level");
            }
            *level = u32::try_from(next).unwrap_or(u32::MAX);
            reply(method, outpost_info_payload_typed(account))
        }
        "outpost.SetHero" => {
            let request = match OutpostSetHeroRequest::decode(request_args) {
                Ok(request) => request,
                Err(_) => return invalid("outpost hero request is invalid"),
            };
            let building_id = request.building_id;
            if !account.buildings.levels.contains_key(&building_id) {
                return invalid("outpost building was not found");
            }
            let mut hero_ids = Vec::new();
            for hero_id in request.hero_ids {
                let Ok(hero_id) = blueoath_domain::HeroId::new(hero_id) else {
                    return invalid("outpost hero id is invalid");
                };
                if !account.dock.heroes.contains_key(&hero_id) || hero_ids.contains(&hero_id) {
                    return invalid("outpost hero is not owned or duplicated");
                }
                hero_ids.push(hero_id);
            }
            account
                .buildings
                .hero_assignments
                .insert(building_id, hero_ids);
            reply(method, outpost_info_payload_typed(account))
        }
        "outpost.SetUseCoin"
        | "outpost.SaveTactic"
        | "outpost.RemoveTactic"
        | "outpost.ChangeTacticName"
        | "outpost.ReceiveItem"
        | "outpost.ReceiveAll"
        | "outpost.SpeedUpProduction" => invalid("outpost operation is not supported"),
        _ => HandlerResult::Empty,
    }
}

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

fn invalid(message: &'static str) -> HandlerResult {
    HandlerResult::Error(GameError::InvalidRequest(message))
}

pub(crate) fn outpost_info_payload_typed(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let mut output = Vec::new();
    for (building_id, level) in &account.buildings.levels {
        let mut building = Vec::new();
        append_varint_field(&mut building, 1, *building_id);
        append_varint_field(&mut building, 2, u64::from(*level));
        if let Some(hero_ids) = account.buildings.hero_assignments.get(building_id) {
            for hero_id in hero_ids {
                append_varint_field(&mut building, 3, hero_id.get());
            }
        }
        append_varint_field(&mut building, 4, 1);
        append_varint_field(&mut building, 5, 0);
        append_message_field(&mut output, 1, &building);
    }
    append_varint_field(&mut output, 2, u64::from(current_unix_seconds()));
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_outpost_updates_building_domain_state() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("outpost").unwrap(),
            "Outpost",
        );
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 1);
        append_varint_field(&mut args, 2, 1);
        assert!(matches!(
            handle_typed(&mut account, "outpost.SetHero", &args),
            HandlerResult::Reply(_)
        ));
        assert_eq!(account.buildings.hero_assignments[&1].len(), 1);
        assert!(matches!(
            handle_typed(&mut account, "outpost.UpgradeBuilding", &[8, 1],),
            HandlerResult::Reply(_)
        ));
        assert_eq!(account.buildings.levels[&1], 3);
    }
}
