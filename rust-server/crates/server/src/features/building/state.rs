#![allow(dead_code)]

use super::*;

pub(crate) fn bathroom_end_payload(hero_id: u64, bath_time: i64) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, 0);
    append_varint_field(&mut output, 2, bath_time.max(0) as u64);
    append_varint_field(&mut output, 3, hero_id);
    output
}

pub(crate) fn decode_building_assignments(
    request: &BuildingSetHeroListRequest,
) -> Vec<(i32, Vec<i32>)> {
    let building_ids = &request.building_ids;
    let hero_ids = &request.hero_ids;
    let mut assignments = Vec::new();
    let mut cursor = 0;
    for building_id in building_ids {
        let mut assigned = Vec::new();
        while cursor < hero_ids.len() && hero_ids[cursor] != -1 {
            assigned.push(hero_ids[cursor]);
            cursor += 1;
        }
        if cursor >= hero_ids.len() {
            return Vec::new();
        }
        cursor += 1;
        assignments.push((*building_id, assigned));
    }
    if cursor != hero_ids.len() {
        return Vec::new();
    }
    assignments
}

pub(crate) fn building_capacity(
    template_id: i32,
    level: i32,
    catalog: Option<&BuildingCatalog>,
) -> usize {
    // Bundled config_buildinginfo uses five slots for dormitory/production buildings;
    // office templates use level as capacity. Keep fallback deterministic when a
    // deployment omits optional building catalog rows.
    if let Some(capacity) = catalog.and_then(|catalog| catalog.capacities.get(&template_id)) {
        *capacity
    } else if (41..=45).contains(&template_id) {
        5
    } else if (1..=5).contains(&template_id) {
        level.max(1) as usize
    } else {
        0
    }
}

pub(crate) fn bathroom_service_payload(
    hero_id: u64,
    pos: i64,
    buff_id: u32,
    is_crit: bool,
) -> Vec<u8> {
    let mut output = Vec::new();
    if pos > 0 {
        append_varint_field(&mut output, 1, pos as u64);
    }
    append_varint_field(&mut output, 2, hero_id);
    append_varint_field(&mut output, 3, u64::from(buff_id));
    append_varint_field(&mut output, 4, u64::from(is_crit));
    output
}
