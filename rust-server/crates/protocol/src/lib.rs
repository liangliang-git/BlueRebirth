use std::collections::BTreeMap;
use std::io;

use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const MAX_FRAME_SIZE: i32 = 4 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("truncated protobuf {0}")]
    Truncated(&'static str),
    #[error("invalid protobuf: {0}")]
    Invalid(&'static str),
    #[error("protobuf varint is too long")]
    VarintTooLong,
    #[error("invalid game login frame length: {0}")]
    InvalidFrameLength(i64),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TRequest {
    pub method: String,
    pub args: Option<Vec<u8>>,
    pub callback_handler: u32,
    pub token: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TResponse {
    pub err: i32,
    pub err_msg: String,
    pub method: String,
    pub ret: Option<Vec<u8>>,
    pub callback_handler: u32,
    pub time: u32,
    pub token: String,
    pub seq: u32,
    pub is_response: i32,
}

/// Typed protobuf request boundary used by game handlers.
pub trait Decode: Sized {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError>;
}

/// Collects protobuf varint fields while skipping unknown fields.
///
/// Repeated field values are preserved in wire order. Request DTOs decide
/// which fields may repeat and which duplicates are invalid.
pub fn decode_varint_fields(payload: &[u8]) -> Result<BTreeMap<u32, Vec<u64>>, ProtocolError> {
    let mut reader = PbReader::new(payload);
    let mut fields = BTreeMap::<u32, Vec<u64>>::new();
    while let Some((field, wire)) = reader.next_field()? {
        if wire == 0 {
            fields.entry(field).or_default().push(reader.read_varint()?);
        } else {
            reader.skip(wire)?;
        }
    }
    Ok(fields)
}

fn decode_repeated_message_fields(
    payload: &[u8],
    target_field: u32,
) -> Result<Vec<Vec<u8>>, ProtocolError> {
    let mut reader = PbReader::new(payload);
    let mut messages = Vec::new();
    while let Some((field, wire)) = reader.next_field()? {
        if field == target_field && wire == 2 {
            messages.push(reader.read_bytes()?.to_vec());
        } else {
            reader.skip(wire)?;
        }
    }
    Ok(messages)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyStartRequest {
    pub copy_id: i32,
    pub is_running_fight: bool,
    pub battle_mode: i32,
    pub anim_mode: i32,
    pub ex_buffs: Vec<i32>,
    pub match_type: i32,
    pub is_pve_pt_mode: bool,
}

impl Decode for CopyStartRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let copy_values = fields.get(&2).map(Vec::as_slice).unwrap_or_default();
        let copy_id = match copy_values {
            [] => {
                return Err(ProtocolError::Invalid(
                    "copy start request is missing copy id",
                ))
            }
            [_first, _second, ..] => {
                return Err(ProtocolError::Invalid(
                    "copy start request has duplicate copy id",
                ))
            }
            [value] => to_i32(*value, "copy start request copy id is out of range")?,
        };
        Ok(Self {
            copy_id,
            is_running_fight: optional_i32(
                &fields,
                3,
                "copy start request has duplicate running fight",
            )? != 0,
            battle_mode: optional_i32(&fields, 9, "copy start request has duplicate battle mode")?,
            anim_mode: optional_i32(
                &fields,
                10,
                "copy start request has duplicate animation mode",
            )?,
            ex_buffs: fields
                .get(&12)
                .into_iter()
                .flatten()
                .map(|value| to_i32(*value, "copy start request ex buff is out of range"))
                .collect::<Result<Vec<_>, _>>()?,
            match_type: optional_i32(&fields, 15, "copy start request has duplicate match type")?,
            is_pve_pt_mode: optional_i32(&fields, 17, "copy start request has duplicate pve mode")?
                != 0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyAttackRequest {
    pub attack_type: u64,
    pub copy_id: u64,
    pub hero_ids: Vec<u64>,
    pub enemy_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyMiniGamePassRequest {
    pub copy_id: i32,
    pub battle_time: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyStarRewardRequest {
    pub chapter_id: i32,
    pub indexes: Vec<i32>,
}

impl Decode for CopyStarRewardRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let chapter_id = required_field(&fields, 1, "copy star reward is missing chapter id")?;
        let mut indexes = fields
            .get(&3)
            .into_iter()
            .flatten()
            .map(|value| to_i32(*value, "copy star reward index is out of range"))
            .collect::<Result<Vec<_>, _>>()?;
        if indexes.is_empty() {
            let index = optional_i32(&fields, 2, "copy star reward has duplicate index")?;
            if index > 0 {
                indexes.push(index);
            }
        }
        if chapter_id <= 0 || indexes.is_empty() || indexes.iter().any(|index| *index <= 0) {
            return Err(ProtocolError::Invalid(
                "copy star reward request is invalid",
            ));
        }
        Ok(Self {
            chapter_id,
            indexes,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyIdRequest {
    pub copy_id: i32,
}

impl Decode for CopyIdRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let copy_id = required_field(&fields, 1, "copy request is missing copy id")?;
        if copy_id <= 0 {
            return Err(ProtocolError::Invalid("copy request copy id is invalid"));
        }
        Ok(Self { copy_id })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyTypeRequest {
    pub copy_type: i32,
}

impl Decode for CopyTypeRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let copy_type = optional_i32(&fields, 1, "copy type request has duplicate type")?;
        if copy_type < 0 {
            return Err(ProtocolError::Invalid("copy type request is invalid"));
        }
        Ok(Self { copy_type })
    }
}

impl Decode for CopyMiniGamePassRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let copy_id = required_field(&fields, 1, "mini-game pass is missing copy id")?;
        let battle_time = optional_i32(&fields, 12, "mini-game pass has duplicate battle time")?;
        let result = optional_u64(&fields, 19, "mini-game pass has duplicate result")?;
        if copy_id <= 0 || result == 0 {
            return Err(ProtocolError::Invalid("mini-game pass request is invalid"));
        }
        Ok(Self {
            copy_id,
            battle_time,
        })
    }
}

impl Decode for CopyAttackRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let attack_type = required_u64(&fields, 1, "copy attack is missing attack type")?;
        let copy_id = required_u64(&fields, 2, "copy attack is missing copy id")?;
        let enemy_id = required_u64(&fields, 4, "copy attack is missing enemy id")?;
        let values = fields.get(&3).map(Vec::as_slice).unwrap_or_default();
        if values.is_empty() || values.len() > 6 || values.contains(&0) {
            return Err(ProtocolError::Invalid("copy attack hero ids are invalid"));
        }
        let mut hero_ids = values.to_vec();
        hero_ids.sort_unstable();
        hero_ids.dedup();
        if hero_ids.len() != values.len() {
            return Err(ProtocolError::Invalid(
                "copy attack hero ids are duplicated",
            ));
        }
        if attack_type == 0 || copy_id == 0 || enemy_id == 0 {
            return Err(ProtocolError::Invalid("copy attack request is invalid"));
        }
        Ok(Self {
            attack_type,
            copy_id,
            hero_ids: values.to_vec(),
            enemy_id,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyPassHeroResult {
    pub hero_id: u64,
    pub hp: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyPassRequest {
    pub grade: i32,
    pub battle_time: i32,
    pub mvp_hero_id: Option<u64>,
    pub heroes: Vec<CopyPassHeroResult>,
    pub passed_fleet_ids: Vec<u64>,
    pub damage: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRewardRequest {
    pub task_id: u64,
    pub task_type: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskAllRewardRequest {
    pub reward_type: u32,
}

impl Decode for TaskAllRewardRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let reward_type = u32::try_from(optional_u64(
            &fields,
            1,
            "task all reward has duplicate type",
        )?)
        .map_err(|_| ProtocolError::Invalid("task all reward type is out of range"))?;
        Ok(Self { reward_type })
    }
}

impl Decode for TaskRewardRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let task_id = required_u64(&fields, 1, "task reward is missing task id")?;
        if task_id == 0 {
            return Err(ProtocolError::Invalid(
                "task reward task id must be positive",
            ));
        }
        let task_type = u32::try_from(optional_u64(
            &fields,
            2,
            "task reward has duplicate task type",
        )?)
        .map_err(|_| ProtocolError::Invalid("task reward task type is out of range"))?;
        Ok(Self { task_id, task_type })
    }
}

impl Decode for CopyPassRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let grade = optional_i32(&fields, 8, "copy pass has duplicate grade")?;
        let battle_time = optional_i32(&fields, 12, "copy pass has duplicate battle time")?;
        let mvp = optional_u64(&fields, 9, "copy pass has duplicate mvp hero")?;
        if grade < 0 || battle_time < 0 {
            return Err(ProtocolError::Invalid("copy pass request is invalid"));
        }

        let mut reader = PbReader::new(payload);
        let mut heroes = Vec::<CopyPassHeroResult>::new();
        let mut passed_fleet_ids = Vec::new();
        let mut damage = 0_i32;
        while let Some((field, wire)) = reader.next_field()? {
            if wire != 2 || !matches!(field, 11 | 17 | 18 | 20) {
                reader.skip(wire)?;
                continue;
            }
            let nested = decode_varint_fields(reader.read_bytes()?)?;
            match field {
                11 => {
                    let value = optional_u64(&nested, 2, "copy pass has duplicate damage")?;
                    let value = i32::try_from(value)
                        .map_err(|_| ProtocolError::Invalid("copy pass damage is out of range"))?;
                    damage = damage.max(value);
                }
                18 => {
                    let hero_id = optional_u64(&nested, 1, "copy pass hero is missing id")?;
                    let hp = optional_u64(&nested, 2, "copy pass hero has duplicate hp")?;
                    if hero_id == 0 || heroes.iter().any(|hero| hero.hero_id == hero_id) {
                        return Err(ProtocolError::Invalid("copy pass hero result is invalid"));
                    }
                    heroes.push(CopyPassHeroResult { hero_id, hp });
                }
                17 | 20 => {
                    let fleet_id = optional_u64(&nested, 1, "copy pass fleet is missing id")?;
                    if fleet_id == 0 || passed_fleet_ids.contains(&fleet_id) {
                        return Err(ProtocolError::Invalid("copy pass fleet result is invalid"));
                    }
                    passed_fleet_ids.push(fleet_id);
                }
                _ => {}
            }
        }
        Ok(Self {
            grade,
            battle_time,
            mvp_hero_id: (mvp > 0).then_some(mvp),
            heroes,
            passed_fleet_ids,
            damage,
        })
    }
}

macro_rules! single_varint_request {
    ($name:ident, $field_name:ident, $field:expr, $missing:expr, $duplicate:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub $field_name: i32,
        }

        impl Decode for $name {
            fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
                Ok(Self {
                    $field_name: decode_required_varint(payload, $field, $missing, $duplicate)?,
                })
            }
        }
    };
}

single_varint_request!(
    SetSecretaryRequest,
    secretary_id,
    1,
    "secretary request is missing id",
    "secretary request has duplicate id"
);
single_varint_request!(
    SetHeadFrameRequest,
    head_frame,
    1,
    "head frame request is missing id",
    "head frame request has duplicate id"
);
single_varint_request!(
    SetHeadRequest,
    head,
    2,
    "head request is missing id",
    "head request has duplicate id"
);

macro_rules! single_string_request {
    ($name:ident, $field_name:ident, $missing:expr, $duplicate:expr, $too_long:expr, $max:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub $field_name: String,
        }

        impl Decode for $name {
            fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
                Ok(Self {
                    $field_name: decode_required_string(
                        payload, 1, $missing, $duplicate, $too_long, $max,
                    )?,
                })
            }
        }
    };
}

single_string_request!(
    ChangeNameRequest,
    name,
    "name is missing",
    "name has duplicate value",
    "name is too long",
    64
);
single_string_request!(
    SetMessageRequest,
    message,
    "message is missing",
    "message has duplicate value",
    "message is too long",
    256
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingAddRequest {
    pub template_id: i32,
    pub land_index: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategyLearnRequest {
    pub strategy_id: i32,
    pub level: i32,
}

impl Decode for StrategyLearnRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let strategy_id = required_field(&fields, 1, "strategy request is missing strategy id")?;
        let level = optional_i32(&fields, 2, "strategy request has duplicate level")?;
        if strategy_id <= 0 {
            return Err(ProtocolError::Invalid("strategy request is invalid"));
        }
        Ok(Self { strategy_id, level })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategyApplyRequest {
    pub strategy_id: i32,
    pub fleet_id: i32,
    pub tactic_type: i32,
}

impl Decode for StrategyApplyRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let strategy_id = required_field(&fields, 1, "strategy apply is missing strategy id")?;
        let fleet_id = required_field(&fields, 3, "strategy apply is missing fleet id")?;
        let tactic_type = required_field(&fields, 4, "strategy apply is missing tactic type")?;
        if strategy_id <= 0 || fleet_id <= 0 || tactic_type <= 0 {
            return Err(ProtocolError::Invalid("strategy apply request is invalid"));
        }
        Ok(Self {
            strategy_id,
            fleet_id,
            tactic_type,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportStartRequest {
    pub support_id: i32,
    pub hero_ids: Vec<u64>,
}

impl Decode for SupportStartRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let support_id = required_field(&fields, 1, "support request is missing support id")?;
        let hero_ids = fields
            .get(&2)
            .into_iter()
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        if support_id <= 0 || hero_ids.is_empty() || hero_ids.contains(&0) {
            return Err(ProtocolError::Invalid("support request is invalid"));
        }
        Ok(Self {
            support_id,
            hero_ids,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportCompleteRequest {
    pub id: u32,
    pub completion_type: i32,
}

impl Decode for SupportCompleteRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let id = required_u64(&fields, 1, "support completion is missing id")?;
        let completion_type = required_field(&fields, 2, "support completion is missing type")?;
        let id =
            u32::try_from(id).map_err(|_| ProtocolError::Invalid("support id is out of range"))?;
        if id == 0 || !(1..=3).contains(&completion_type) {
            return Err(ProtocolError::Invalid(
                "support completion request is invalid",
            ));
        }
        Ok(Self {
            id,
            completion_type,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplySwitchRequest {
    pub hero_ids: Vec<u64>,
}

impl Decode for SupplySwitchRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let hero_ids = fields
            .get(&1)
            .into_iter()
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        if hero_ids.contains(&0) {
            return Err(ProtocolError::Invalid("supply hero id is invalid"));
        }
        Ok(Self { hero_ids })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MilestoneFetchRequest {
    pub activity_id: i32,
    pub index: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuidePlotRewardRequest {
    pub plot_id: i32,
}

impl Decode for GuidePlotRewardRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let plot_id = required_field(&fields, 1, "guide plot is missing id")?;
        if plot_id <= 0 {
            return Err(ProtocolError::Invalid("guide plot request is invalid"));
        }
        Ok(Self { plot_id })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiniGameScoreEntry {
    pub copy_id: i32,
    pub score: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserMiniGameScoreRequest {
    pub chapter_id: i32,
    pub entries: Vec<MiniGameScoreEntry>,
}

impl Decode for UserMiniGameScoreRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let chapter_fields = decode_varint_fields(payload)?;
        let chapter_id = required_field(&chapter_fields, 1, "mini-game is missing chapter id")?;
        let mut reader = PbReader::new(payload);
        let mut entries = Vec::new();
        while let Some((field, wire)) = reader.next_field()? {
            if field != 3 || wire != 2 {
                reader.skip(wire)?;
                continue;
            }
            if entries.len() >= 99 {
                return Err(ProtocolError::Invalid("mini-game has too many scores"));
            }
            let fields = decode_varint_fields(reader.read_bytes()?)?;
            let copy_id = required_field(&fields, 1, "mini-game score is missing copy id")?;
            let score = optional_u64(&fields, 2, "mini-game score has duplicate value")?;
            if copy_id <= 0 {
                return Err(ProtocolError::Invalid("mini-game score copy id is invalid"));
            }
            entries.push(MiniGameScoreEntry { copy_id, score });
        }
        if chapter_id <= 0 || entries.is_empty() {
            return Err(ProtocolError::Invalid("mini-game score request is invalid"));
        }
        Ok(Self {
            chapter_id,
            entries,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiniGameChapterRequest {
    pub chapter_id: i32,
}

impl Decode for MiniGameChapterRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let chapter_id = required_field(&fields, 1, "mini-game is missing chapter id")?;
        if chapter_id <= 0 {
            return Err(ProtocolError::Invalid("mini-game chapter is invalid"));
        }
        Ok(Self { chapter_id })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiniGameRankRequest {
    pub chapter_id: i32,
    pub start: i32,
    pub end: i32,
}

impl Decode for MiniGameRankRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let chapter_id = required_field(&fields, 1, "mini-game rank is missing chapter id")?;
        let start = optional_i32(&fields, 2, "mini-game rank has duplicate start")?;
        let end = optional_i32(&fields, 3, "mini-game rank has duplicate end")?;
        if chapter_id <= 0 || start < 0 || end < 0 {
            return Err(ProtocolError::Invalid("mini-game rank request is invalid"));
        }
        Ok(Self {
            chapter_id,
            start,
            end,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserOrderRecordRequest {
    pub record_type: i32,
    pub sort: i32,
    pub screen: i32,
    pub order: i32,
}

impl Decode for UserOrderRecordRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        Ok(Self {
            record_type: optional_i32(&fields, 1, "user order has duplicate type")?,
            sort: optional_i32(&fields, 2, "user order has duplicate sort")?,
            screen: optional_i32(&fields, 3, "user order has duplicate screen")?,
            order: optional_i32(&fields, 4, "user order has duplicate order")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserRefreshRequest {
    pub max_power_index: i32,
    pub min_power_index: i32,
}

impl Decode for UserRefreshRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        Ok(Self {
            max_power_index: optional_i32(&fields, 2, "user refresh has duplicate max index")?,
            min_power_index: optional_i32(&fields, 3, "user refresh has duplicate min index")?,
        })
    }
}

impl Decode for MilestoneFetchRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let activity_id = required_field(&fields, 1, "milestone is missing activity id")?;
        let index = required_field(&fields, 2, "milestone is missing index")?;
        if activity_id <= 0 || index <= 0 {
            return Err(ProtocolError::Invalid("milestone request is invalid"));
        }
        Ok(Self { activity_id, index })
    }
}

impl Decode for BuildingAddRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let template_id = required_field(&fields, 1, "building add is missing template id")?;
        let land_index = required_field(&fields, 2, "building add is missing land index")?;
        if template_id <= 0 || land_index <= 0 {
            return Err(ProtocolError::Invalid("building add request is invalid"));
        }
        Ok(Self {
            template_id,
            land_index,
        })
    }
}

single_varint_request!(
    BuildingIdRequest,
    building_id,
    1,
    "building request is missing building id",
    "building request has duplicate building id"
);

single_varint_request!(
    BuildingResourceRequest,
    resource_id,
    1,
    "building resource request is missing resource id",
    "building resource request has duplicate resource id"
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingProduceRequest {
    pub building_id: i32,
    pub recipe_id: i32,
    pub count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingSetHeroRequest {
    pub building_id: i32,
    pub hero_ids: Vec<i32>,
}

impl Decode for BuildingSetHeroRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let building_id = required_field(&fields, 1, "building assignment is missing building id")?;
        let hero_ids = fields
            .get(&2)
            .into_iter()
            .flatten()
            .map(|value| to_i32(*value, "building hero id is out of range"))
            .collect::<Result<Vec<_>, _>>()?;
        if building_id <= 0 || hero_ids.len() > 99 || hero_ids.iter().any(|hero_id| *hero_id <= 0) {
            return Err(ProtocolError::Invalid(
                "building assignment request is invalid",
            ));
        }
        Ok(Self {
            building_id,
            hero_ids,
        })
    }
}

impl Decode for BuildingProduceRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let building_id = required_field(&fields, 1, "building production is missing building id")?;
        let recipe_id = required_field(&fields, 2, "building production is missing recipe id")?;
        let count = required_field(&fields, 3, "building production is missing count")?;
        if building_id <= 0 || recipe_id <= 0 || count <= 0 {
            return Err(ProtocolError::Invalid(
                "building production request is invalid",
            ));
        }
        Ok(Self {
            building_id,
            recipe_id,
            count,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstructionProjectRequest {
    pub gold: i32,
    pub steel: i32,
    pub aluminium: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildFormulaRequest {
    pub projects: Vec<ConstructionProjectRequest>,
}

impl Decode for BuildFormulaRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let mut reader = PbReader::new(payload);
        let mut projects = Vec::new();
        while let Some((field, wire)) = reader.next_field()? {
            if field != 1 || wire != 2 {
                reader.skip(wire)?;
                continue;
            }
            if projects.len() >= 10 {
                return Err(ProtocolError::Invalid(
                    "construction request has too many projects",
                ));
            }
            let project = decode_construction_project(reader.read_bytes()?)?;
            projects.push(project);
        }
        if projects.is_empty() {
            return Err(ProtocolError::Invalid(
                "construction request has no projects",
            ));
        }
        Ok(Self { projects })
    }
}

fn decode_construction_project(
    payload: &[u8],
) -> Result<ConstructionProjectRequest, ProtocolError> {
    let mut reader = PbReader::new(payload);
    let mut gold = None;
    let mut materials = Vec::new();
    while let Some((field, wire)) = reader.next_field()? {
        match (field, wire) {
            (1, 2) => {
                if materials.len() >= 2 {
                    return Err(ProtocolError::Invalid(
                        "construction project has duplicate material",
                    ));
                }
                let fields = decode_varint_fields(reader.read_bytes()?)?;
                materials.push((
                    required_field(&fields, 1, "construction material is missing id")?,
                    required_field(&fields, 2, "construction material is missing count")?,
                ));
            }
            (2, 0) => {
                if gold.is_some() {
                    return Err(ProtocolError::Invalid(
                        "construction project has duplicate gold",
                    ));
                }
                gold = Some(to_i32(
                    reader.read_varint()?,
                    "construction gold is out of range",
                )?);
            }
            (_, wire) => reader.skip(wire)?,
        }
    }
    let gold = gold.ok_or(ProtocolError::Invalid(
        "construction project is missing gold",
    ))?;
    let mut steel = None;
    let mut aluminium = None;
    for (resource_id, count) in materials {
        if !(30..=999).contains(&count) {
            return Err(ProtocolError::Invalid(
                "construction material count is invalid",
            ));
        }
        match resource_id {
            10029 if steel.is_none() => steel = Some(count),
            10030 if aluminium.is_none() => aluminium = Some(count),
            _ => {
                return Err(ProtocolError::Invalid(
                    "construction material id is invalid",
                ))
            }
        }
    }
    if !(30..=999).contains(&gold) || steel.is_none() || aluminium.is_none() {
        return Err(ProtocolError::Invalid("construction project is invalid"));
    }
    Ok(ConstructionProjectRequest {
        gold,
        steel: steel.unwrap_or_default(),
        aluminium: aluminium.unwrap_or_default(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstructionIndexesRequest {
    pub indexes: Vec<i32>,
}

impl Decode for ConstructionIndexesRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let mut reader = PbReader::new(payload);
        let mut indexes = Vec::new();
        while let Some((field, wire)) = reader.next_field()? {
            match (field, wire) {
                (1, 0) => indexes.push(to_i32(
                    reader.read_varint()?,
                    "construction index is out of range",
                )?),
                (1, 2) => {
                    let mut packed = PbReader::new(reader.read_bytes()?);
                    while packed.offset < packed.data.len() {
                        let value = packed.read_varint()?;
                        indexes.push(to_i32(value, "construction index is out of range")?);
                    }
                }
                (_, wire) => reader.skip(wire)?,
            }
        }
        if indexes.is_empty() || indexes.len() > 99 || indexes.iter().any(|index| *index <= 0) {
            return Err(ProtocolError::Invalid(
                "construction indexes request is invalid",
            ));
        }
        Ok(Self { indexes })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstructionReceiveRequest {
    pub indexes: Vec<i32>,
}

impl Decode for ConstructionReceiveRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let mut reader = PbReader::new(payload);
        let mut indexes = Vec::new();
        while let Some((field, wire)) = reader.next_field()? {
            match (field, wire) {
                (1, 0) => indexes.push(to_i32(
                    reader.read_varint()?,
                    "construction index is out of range",
                )?),
                (1, 2) => {
                    let mut packed = PbReader::new(reader.read_bytes()?);
                    while packed.offset < packed.data.len() {
                        indexes.push(to_i32(
                            packed.read_varint()?,
                            "construction index is out of range",
                        )?);
                    }
                }
                (_, wire) => reader.skip(wire)?,
            }
        }
        if indexes.len() > 99 || indexes.iter().any(|index| *index <= 0) {
            return Err(ProtocolError::Invalid(
                "construction receive request is invalid",
            ));
        }
        Ok(Self { indexes })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShopBuyRequest {
    pub shop_id: i32,
    pub good_id: i32,
    pub buy_num: i32,
}

single_varint_request!(
    ShopRefreshRequest,
    shop_id,
    1,
    "shop refresh is missing shop id",
    "shop refresh has duplicate shop id"
);

impl Decode for ShopBuyRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let shop_id = required_field(&fields, 1, "shop buy is missing shop id")?;
        let good_id = required_field(&fields, 2, "shop buy is missing good id")?;
        let buy_num = optional_i32(&fields, 3, "shop buy has duplicate quantity")?;
        if shop_id <= 0 || good_id <= 0 || buy_num < 0 {
            return Err(ProtocolError::Invalid("shop buy request is invalid"));
        }
        Ok(Self {
            shop_id,
            good_id,
            buy_num: buy_num.max(1),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShopQualityBuyRequest {
    pub shop_id: i32,
    pub good_ids: Vec<i32>,
}

impl Decode for ShopQualityBuyRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let shop_id = required_field(&fields, 1, "quality shop buy is missing shop id")?;
        let values = fields.get(&2).map(Vec::as_slice).unwrap_or_default();
        if shop_id <= 0 || values.is_empty() || values.len() > 99 {
            return Err(ProtocolError::Invalid(
                "quality shop buy request is invalid",
            ));
        }
        let mut good_ids = Vec::with_capacity(values.len());
        for value in values {
            let good_id = to_i32(*value, "quality shop good id is out of range")?;
            if good_id <= 0 || good_ids.contains(&good_id) {
                return Err(ProtocolError::Invalid("quality shop good ids are invalid"));
            }
            good_ids.push(good_id);
        }
        Ok(Self { shop_id, good_ids })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BagSaleItemRequest {
    pub template_id: i32,
    pub amount: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BagSaleRequest {
    pub items: Vec<BagSaleItemRequest>,
}

impl Decode for BagSaleRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let mut reader = PbReader::new(payload);
        let mut items = Vec::new();
        while let Some((field, wire)) = reader.next_field()? {
            if field != 1 || wire != 2 {
                reader.skip(wire)?;
                continue;
            }
            if items.len() >= 99 {
                return Err(ProtocolError::Invalid("bag sale has too many items"));
            }
            let fields = decode_varint_fields(reader.read_bytes()?)?;
            let template_id = required_field(&fields, 1, "bag sale is missing template id")?;
            let amount = required_field(&fields, 2, "bag sale is missing amount")?;
            if template_id <= 0 || amount <= 0 {
                return Err(ProtocolError::Invalid("bag sale item is invalid"));
            }
            items.push(BagSaleItemRequest {
                template_id,
                amount,
            });
        }
        if items.is_empty() {
            return Err(ProtocolError::Invalid("bag sale request is empty"));
        }
        Ok(Self { items })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BagCompositeRequest {
    pub template_id: i32,
    pub amount: i32,
}

impl Decode for BagCompositeRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let template_id = required_field(&fields, 1, "bag composite is missing template id")?;
        let amount = required_field(&fields, 2, "bag composite is missing amount")?;
        if template_id <= 0 || amount <= 0 {
            return Err(ProtocolError::Invalid("bag composite request is invalid"));
        }
        Ok(Self {
            template_id,
            amount,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResourcePurchaseRequest;

impl Decode for ResourcePurchaseRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        decode_varint_fields(payload)?;
        Ok(Self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserSupplyRequest {
    pub supply_id: i32,
}

impl Decode for UserSupplyRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let supply_id = required_field(&fields, 1, "supply request is missing id")?;
        if supply_id <= 0 {
            return Err(ProtocolError::Invalid("supply request is invalid"));
        }
        Ok(Self { supply_id })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserOtherInfoRequest {
    pub requested_uid: u64,
}

impl Decode for UserOtherInfoRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        Ok(Self {
            requested_uid: optional_u64(&fields, 1, "other user request has duplicate uid")?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeacherRankRequest {
    pub begin: i32,
    pub offset: i32,
}

impl Decode for TeacherRankRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        Ok(Self {
            begin: optional_i32(&fields, 1, "teacher rank has duplicate begin")?,
            offset: optional_i32(&fields, 2, "teacher rank has duplicate offset")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FriendTargetRequest {
    pub uid: u64,
}

impl Decode for FriendTargetRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let uid = optional_u64(&fields, 1, "friend request has duplicate target")?;
        if uid == 0 {
            return Err(ProtocolError::Invalid("friend request is missing target"));
        }
        Ok(Self { uid })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FriendSearchRequest {
    pub uid: u64,
    pub name: String,
}

impl Decode for FriendSearchRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let uid = optional_u64(&fields, 1, "friend search has duplicate uid")?;
        let name = decode_optional_string(
            payload,
            2,
            "friend search has duplicate name",
            "friend search name is too long",
            64,
        )?;
        if uid == 0 && name.is_empty() {
            return Err(ProtocolError::Invalid("friend search requires uid or name"));
        }
        Ok(Self { uid, name })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FriendUpdateUserStateRequest {
    pub state: i32,
    pub uid: u64,
}

impl Decode for FriendUpdateUserStateRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        Ok(Self {
            state: optional_i32(&fields, 1, "friend user state has duplicate state")?,
            uid: optional_u64(&fields, 2, "friend user state has duplicate uid")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyCopySelectExRequest {
    pub chapter_id: i32,
    pub select_ex: bool,
}

impl Decode for DailyCopySelectExRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let chapter_id = required_field(&fields, 1, "daily copy select is missing chapter")?;
        let select_ex = optional_i32(&fields, 2, "daily copy select has duplicate flag")? != 0;
        if chapter_id <= 0 {
            return Err(ProtocolError::Invalid(
                "daily copy select chapter is invalid",
            ));
        }
        Ok(Self {
            chapter_id,
            select_ex,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeroLockRequest {
    pub hero_id: u64,
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeroRetireRequest {
    pub hero_ids: Vec<i32>,
    pub dismantle_equipment: bool,
}

impl Decode for HeroRetireRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let hero_ids = fields
            .get(&1)
            .into_iter()
            .flatten()
            .map(|value| to_i32(*value, "hero retire id is out of range"))
            .collect::<Result<Vec<_>, _>>()?;
        let dismantle_equipment =
            optional_i32(&fields, 2, "hero retire has duplicate dismantle flag")? != 0;
        if hero_ids.is_empty()
            || hero_ids.len() > 99
            || hero_ids.iter().any(|hero_id| *hero_id <= 0)
        {
            return Err(ProtocolError::Invalid("hero retire request is invalid"));
        }
        Ok(Self {
            hero_ids,
            dismantle_equipment,
        })
    }
}

impl Decode for HeroLockRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let hero_id = optional_u64(&fields, 1, "hero lock has duplicate hero id")?;
        if hero_id == 0 {
            return Err(ProtocolError::Invalid("hero lock is missing hero id"));
        }
        Ok(Self {
            hero_id,
            locked: optional_i32(&fields, 2, "hero lock has duplicate state")? != 0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeroChangeNameRequest {
    pub hero_id: u64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeroExpItemRequest {
    pub template_id: i32,
    pub amount: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeroAddExpRequest {
    pub hero_id: u64,
    pub items: Vec<HeroExpItemRequest>,
}

impl Decode for HeroAddExpRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let mut reader = PbReader::new(payload);
        let mut hero_id = None;
        let mut items = Vec::new();
        while let Some((field, wire)) = reader.next_field()? {
            match (field, wire) {
                (1, 0) => {
                    if hero_id.is_some() {
                        return Err(ProtocolError::Invalid("hero exp has duplicate hero id"));
                    }
                    hero_id = Some(reader.read_varint()?);
                }
                (2, 2) => {
                    if items.len() >= 99 {
                        return Err(ProtocolError::Invalid("hero exp has too many items"));
                    }
                    let fields = decode_varint_fields(reader.read_bytes()?)?;
                    items.push(HeroExpItemRequest {
                        template_id: required_field(
                            &fields,
                            2,
                            "hero exp item is missing template id",
                        )?,
                        amount: required_field(&fields, 3, "hero exp item is missing amount")?,
                    });
                }
                (_, wire) => reader.skip(wire)?,
            }
        }
        let hero_id = hero_id.ok_or(ProtocolError::Invalid("hero exp is missing hero id"))?;
        if hero_id == 0 || items.is_empty() {
            return Err(ProtocolError::Invalid("hero exp request is invalid"));
        }
        Ok(Self { hero_id, items })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquipMaterialRequest {
    pub template_id: i32,
    pub amount: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquipEnhanceRequest {
    pub equip_id: u64,
    pub materials: Vec<EquipMaterialRequest>,
}

impl Decode for EquipEnhanceRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let equip_id = optional_u64(&fields, 1, "equipment enhance has duplicate equipment id")?;
        if equip_id == 0 {
            return Err(ProtocolError::Invalid(
                "equipment enhance is missing equipment id",
            ));
        }
        let mut reader = PbReader::new(payload);
        let mut materials = Vec::new();
        while let Some((field, wire)) = reader.next_field()? {
            if field == 2 && wire == 2 {
                if materials.len() >= 99 {
                    return Err(ProtocolError::Invalid(
                        "equipment enhance has too many materials",
                    ));
                }
                let fields = decode_varint_fields(reader.read_bytes()?)?;
                materials.push(EquipMaterialRequest {
                    template_id: required_field(
                        &fields,
                        1,
                        "equipment material is missing template id",
                    )?,
                    amount: required_field(&fields, 2, "equipment material is missing amount")?,
                });
            } else {
                reader.skip(wire)?;
            }
        }
        Ok(Self {
            equip_id,
            materials,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquipDismantleRequest {
    pub equip_ids: Vec<u64>,
}

impl Decode for EquipDismantleRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let values = fields.get(&1).map(Vec::as_slice).unwrap_or_default();
        if values.is_empty() || values.len() > 99 {
            return Err(ProtocolError::Invalid(
                "equipment dismantle request is invalid",
            ));
        }
        let mut equip_ids = Vec::with_capacity(values.len());
        for value in values {
            if *value == 0 || equip_ids.contains(value) {
                return Err(ProtocolError::Invalid(
                    "equipment dismantle ids are invalid",
                ));
            }
            equip_ids.push(*value);
        }
        Ok(Self { equip_ids })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquipRiseStarRequest {
    pub equip_id: u64,
    pub consume_ids: Vec<u64>,
}

impl Decode for EquipRiseStarRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let equip_id = optional_u64(&fields, 1, "equipment rise star has duplicate equipment id")?;
        let values = fields.get(&2).map(Vec::as_slice).unwrap_or_default();
        if equip_id == 0 || values.is_empty() || values.len() > 99 {
            return Err(ProtocolError::Invalid(
                "equipment rise star request is invalid",
            ));
        }
        let mut consume_ids = Vec::with_capacity(values.len());
        for value in values {
            if *value == 0 || *value == equip_id || consume_ids.contains(value) {
                return Err(ProtocolError::Invalid(
                    "equipment rise star ids are invalid",
                ));
            }
            consume_ids.push(*value);
        }
        Ok(Self {
            equip_id,
            consume_ids,
        })
    }
}

impl Decode for HeroChangeNameRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let hero_id = optional_u64(&fields, 1, "hero name has duplicate hero id")?;
        if hero_id == 0 {
            return Err(ProtocolError::Invalid("hero name is missing hero id"));
        }
        Ok(Self {
            hero_id,
            name: decode_required_string(
                payload,
                2,
                "hero name is missing",
                "hero name has duplicate value",
                "hero name is too long",
                32,
            )?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyCopyEnterRequest {
    pub chapter_id: i32,
    pub copy_id: i32,
    pub tactic_id: i32,
}

impl Decode for DailyCopyEnterRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let chapter_id = required_field(&fields, 1, "daily copy request is missing chapter id")?;
        let copy_id = required_field(&fields, 2, "daily copy request is missing copy id")?;
        let tactic_id = required_field(&fields, 3, "daily copy request is missing tactic id")?;
        if chapter_id <= 0 || copy_id <= 0 || tactic_id <= 0 {
            return Err(ProtocolError::Invalid("daily copy request has invalid id"));
        }
        Ok(Self {
            chapter_id,
            copy_id,
            tactic_id,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyRecordRequest {
    pub copy_id: i32,
    pub index: i32,
}

impl Decode for CopyRecordRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let copy_id = required_field(&fields, 1, "copy record request is missing copy id")?;
        let index = optional_i32(&fields, 2, "copy record request has duplicate index")?;
        if copy_id <= 0 || index < 0 {
            return Err(ProtocolError::Invalid("copy record request has invalid id"));
        }
        Ok(Self { copy_id, index })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeaDifficultyRequest {
    pub copy_id: i32,
    pub difficulty: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeWorldChannelRequest {
    pub channel: i32,
}

impl Decode for ChangeWorldChannelRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let channel = optional_i32(&fields, 1, "chat channel has duplicate value")?;
        if channel < 0 {
            return Err(ProtocolError::Invalid("chat channel is invalid"));
        }
        Ok(Self { channel })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendMessageRequest {
    pub channel: i32,
    pub receive_uid: u64,
    pub message: String,
    pub message_type: i32,
    pub voice: String,
}

impl Decode for SendMessageRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let channel = optional_i32(&fields, 1, "chat message has duplicate channel")?;
        let receive_uid = optional_u64(&fields, 2, "chat message has duplicate receiver")?;
        let message = decode_required_string(
            payload,
            3,
            "chat message is missing content",
            "chat message has duplicate content",
            "chat message is too long",
            512,
        )?;
        let message_type = optional_i32(&fields, 4, "chat message has duplicate type")?;
        let voice = decode_optional_string(
            payload,
            5,
            "chat message has duplicate voice",
            "chat voice is too long",
            2048,
        )?;
        Ok(Self {
            channel,
            receive_uid,
            message,
            message_type,
            voice,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendBarrageRequest {
    pub id: i32,
    pub offset: i32,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetBarrageByIdRequest {
    pub id: i32,
    pub begin: i32,
    pub len: i32,
}

impl Decode for GetBarrageByIdRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let id = optional_i32(&fields, 1, "barrage query has duplicate id")?;
        let begin = optional_i32(&fields, 2, "barrage query has duplicate begin")?;
        let len = optional_i32(&fields, 3, "barrage query has duplicate len")?;
        if id < 0 || begin < 0 || !(0..=100).contains(&len) {
            return Err(ProtocolError::Invalid("barrage query is invalid"));
        }
        Ok(Self { id, begin, len })
    }
}

impl Decode for SendBarrageRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let id = optional_i32(&fields, 1, "barrage has duplicate id")?;
        let offset = optional_i32(&fields, 2, "barrage has duplicate offset")?;
        let content = decode_required_string(
            payload,
            3,
            "barrage is missing content",
            "barrage has duplicate content",
            "barrage is too long",
            512,
        )?;
        if id < 0 || offset < 0 || content.trim().is_empty() {
            return Err(ProtocolError::Invalid("barrage request is invalid"));
        }
        Ok(Self {
            id,
            offset,
            content,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildShipRequest {
    pub pool_id: i32,
    pub pulls: i32,
}

impl Decode for BuildShipRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let pool_id = optional_i32(&fields, 1, "build ship request has duplicate pool id")?;
        let pulls = optional_i32(&fields, 2, "build ship request has duplicate pulls")?;
        if pool_id <= 0 || !(0..=10).contains(&pulls) {
            return Err(ProtocolError::Invalid("build ship request is invalid"));
        }
        Ok(Self { pool_id, pulls })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildShipRewardRequest {
    pub pool_id: i32,
    pub milestone: i32,
}

impl Decode for BuildShipRewardRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let pool_id = required_field(&fields, 1, "build reward is missing pool id")?;
        let milestone = required_field(&fields, 2, "build reward is missing milestone")?;
        if pool_id <= 0 || milestone <= 0 {
            return Err(ProtocolError::Invalid("build reward request is invalid"));
        }
        Ok(Self { pool_id, milestone })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuildBoxIdRequest {
    pub box_id: u64,
}

impl Decode for GuildBoxIdRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let box_id = required_field(&fields, 1, "guild box is missing id")?;
        if box_id == 0 {
            return Err(ProtocolError::Invalid("guild box id is invalid"));
        }
        Ok(Self {
            box_id: u64::try_from(box_id)
                .map_err(|_| ProtocolError::Invalid("guild box id is invalid"))?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuildBoxAnonymousRequest {
    pub anonymous: bool,
}

impl Decode for GuildBoxAnonymousRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let anonymous = optional_i32(&fields, 1, "guild box anonymous has duplicate value")?;
        if !matches!(anonymous, 0 | 1) {
            return Err(ProtocolError::Invalid("guild box anonymous is invalid"));
        }
        Ok(Self {
            anonymous: anonymous != 0,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutpostBuildingRequest {
    pub building_id: u64,
}

impl Decode for OutpostBuildingRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let building_id = required_field(&fields, 1, "outpost is missing building id")?;
        if building_id == 0 {
            return Err(ProtocolError::Invalid("outpost building id is invalid"));
        }
        Ok(Self {
            building_id: u64::try_from(building_id)
                .map_err(|_| ProtocolError::Invalid("outpost building id is invalid"))?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutpostSetHeroRequest {
    pub building_id: u64,
    pub hero_ids: Vec<u64>,
}

impl Decode for OutpostSetHeroRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let building_id = required_field(&fields, 1, "outpost is missing building id")?;
        let hero_ids = fields.get(&2).cloned().unwrap_or_default();
        if building_id == 0 || hero_ids.contains(&0) {
            return Err(ProtocolError::Invalid("outpost hero request is invalid"));
        }
        Ok(Self {
            building_id: u64::try_from(building_id)
                .map_err(|_| ProtocolError::Invalid("outpost building id is invalid"))?,
            hero_ids,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SportsMeetPointsRequest {
    pub points: u64,
}

impl Decode for SportsMeetPointsRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let points = required_field(&fields, 1, "sports meet is missing points")?;
        if points == 0 {
            return Err(ProtocolError::Invalid("sports meet points are invalid"));
        }
        Ok(Self {
            points: u64::try_from(points)
                .map_err(|_| ProtocolError::Invalid("sports meet points are invalid"))?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeachingUserRequest {
    pub uid: u64,
}

impl Decode for TeachingUserRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let uid = required_field(&fields, 1, "teaching request is missing uid")?;
        if uid == 0 {
            return Err(ProtocolError::Invalid("teaching uid is invalid"));
        }
        Ok(Self {
            uid: u64::try_from(uid)
                .map_err(|_| ProtocolError::Invalid("teaching uid is invalid"))?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InviteStateTypeRequest {
    pub state_type: i32,
}

impl Decode for InviteStateTypeRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let state_type = required_field(&fields, 1, "invite state is missing type")?;
        if !(1..=3).contains(&state_type) {
            return Err(ProtocolError::Invalid("invite state type is invalid"));
        }
        Ok(Self { state_type })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InviteRecordVersionRequest {
    pub version: u64,
}

impl Decode for InviteRecordVersionRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let version = required_field(&fields, 1, "invite state is missing version")?;
        Ok(Self {
            version: u64::try_from(version)
                .map_err(|_| ProtocolError::Invalid("invite state version is invalid"))?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShipTaskRewardRequest {
    pub ship_tid: u64,
    pub task_id: u64,
}

impl Decode for ShipTaskRewardRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let ship_tid = required_field(&fields, 1, "ship task is missing ship id")?;
        let task_id = required_field(&fields, 2, "ship task is missing task id")?;
        if ship_tid == 0 || task_id == 0 {
            return Err(ProtocolError::Invalid(
                "ship task reward request is invalid",
            ));
        }
        Ok(Self {
            ship_tid: u64::try_from(ship_tid)
                .map_err(|_| ProtocolError::Invalid("ship task ship id is invalid"))?,
            task_id: u64::try_from(task_id)
                .map_err(|_| ProtocolError::Invalid("ship task id is invalid"))?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShipTaskCurrentShipRequest {
    pub ship_tid: u64,
    pub hero_template_id: u64,
}

impl Decode for ShipTaskCurrentShipRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let ship_tid = required_field(&fields, 1, "ship task is missing ship id")?;
        let hero_template_id = required_field(&fields, 2, "ship task is missing hero template")?;
        if ship_tid == 0 || hero_template_id == 0 {
            return Err(ProtocolError::Invalid("ship task current ship is invalid"));
        }
        Ok(Self {
            ship_tid: u64::try_from(ship_tid)
                .map_err(|_| ProtocolError::Invalid("ship task ship id is invalid"))?,
            hero_template_id: u64::try_from(hero_template_id)
                .map_err(|_| ProtocolError::Invalid("ship task hero template is invalid"))?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuildTaskIdRequest {
    pub task_id: i32,
}

impl Decode for GuildTaskIdRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let task_id = required_field(&fields, 1, "guild task is missing task id")?;
        if task_id <= 0 {
            return Err(ProtocolError::Invalid("guild task id is invalid"));
        }
        Ok(Self { task_id })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuildTaskMemberRequest {
    pub task_id: i32,
}

impl Decode for GuildTaskMemberRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let task_id = required_field(&fields, 2, "guild task is missing task id")?;
        if task_id <= 0 {
            return Err(ProtocolError::Invalid("guild task id is invalid"));
        }
        Ok(Self { task_id })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuildTaskDonationItem {
    pub goods_type: i32,
    pub item_id: i32,
    pub amount: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuildTaskDonateRequest {
    pub task_id: i32,
    pub contribute: i32,
    pub items: Vec<GuildTaskDonationItem>,
}

impl Decode for GuildTaskDonateRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let task_id = required_field(&fields, 2, "guild task is missing task id")?;
        let contribute = required_field(&fields, 4, "guild task is missing contribution")?;
        if task_id <= 0 || contribute <= 0 {
            return Err(ProtocolError::Invalid("guild task donation is invalid"));
        }

        let mut items = Vec::new();
        for payload in decode_repeated_message_fields(payload, 3)? {
            let fields = decode_varint_fields(&payload)?;
            let goods_type = required_field(&fields, 1, "guild donation is missing goods type")?;
            let item_id = required_field(&fields, 2, "guild donation is missing item id")?;
            let amount = required_field(&fields, 3, "guild donation is missing amount")?;
            if goods_type <= 0 || item_id <= 0 || amount <= 0 {
                return Err(ProtocolError::Invalid(
                    "guild task donation item is invalid",
                ));
            }
            items.push(GuildTaskDonationItem {
                goods_type,
                item_id,
                amount,
            });
        }
        if items.is_empty() {
            return Err(ProtocolError::Invalid("guild task donation has no items"));
        }
        Ok(Self {
            task_id,
            contribute,
            items,
        })
    }
}

impl Decode for SeaDifficultyRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let fields = decode_varint_fields(payload)?;
        let copy_id = required_field(&fields, 1, "sea request is missing copy id")?;
        let difficulty = required_field(&fields, 2, "sea request is missing difficulty")?;
        if copy_id <= 0 || !(1..=7).contains(&difficulty) {
            return Err(ProtocolError::Invalid("sea request has invalid value"));
        }
        Ok(Self {
            copy_id,
            difficulty,
        })
    }
}

fn decode_required_varint(
    payload: &[u8],
    field: u32,
    missing: &'static str,
    duplicate: &'static str,
) -> Result<i32, ProtocolError> {
    let mut reader = PbReader::new(payload);
    let mut value = None;
    while let Some((current, wire)) = reader.next_field()? {
        if current == field && wire == 0 {
            if value.is_some() {
                return Err(ProtocolError::Invalid(duplicate));
            }
            value = Some(reader.read_varint()?);
        } else {
            reader.skip(wire)?;
        }
    }
    value
        .map(|value| to_i32(value, "typed request field is out of range"))
        .transpose()?
        .ok_or(ProtocolError::Invalid(missing))
}

fn decode_required_string(
    payload: &[u8],
    field: u32,
    missing: &'static str,
    duplicate: &'static str,
    too_long: &'static str,
    max_length: usize,
) -> Result<String, ProtocolError> {
    let mut reader = PbReader::new(payload);
    let mut value = None;
    while let Some((current, wire)) = reader.next_field()? {
        if current == field && wire == 2 {
            if value.is_some() {
                return Err(ProtocolError::Invalid(duplicate));
            }
            let text = reader.read_string()?;
            if text.chars().count() > max_length {
                return Err(ProtocolError::Invalid(too_long));
            }
            value = Some(text);
        } else {
            reader.skip(wire)?;
        }
    }
    value.ok_or(ProtocolError::Invalid(missing))
}

fn decode_optional_string(
    payload: &[u8],
    field: u32,
    duplicate: &'static str,
    too_long: &'static str,
    max_length: usize,
) -> Result<String, ProtocolError> {
    let mut reader = PbReader::new(payload);
    let mut value = None;
    while let Some((current, wire)) = reader.next_field()? {
        if current == field && wire == 2 {
            if value.is_some() {
                return Err(ProtocolError::Invalid(duplicate));
            }
            let text = reader.read_string()?;
            if text.chars().count() > max_length {
                return Err(ProtocolError::Invalid(too_long));
            }
            value = Some(text);
        } else {
            reader.skip(wire)?;
        }
    }
    Ok(value.unwrap_or_default())
}

fn optional_i32(
    fields: &BTreeMap<u32, Vec<u64>>,
    field: u32,
    duplicate_error: &'static str,
) -> Result<i32, ProtocolError> {
    match fields.get(&field).map(Vec::as_slice).unwrap_or_default() {
        [] => Ok(0),
        [value] => to_i32(*value, "typed request field is out of range"),
        [_first, _second, ..] => Err(ProtocolError::Invalid(duplicate_error)),
    }
}

fn optional_u64(
    fields: &BTreeMap<u32, Vec<u64>>,
    field: u32,
    duplicate_error: &'static str,
) -> Result<u64, ProtocolError> {
    match fields.get(&field).map(Vec::as_slice).unwrap_or_default() {
        [] => Ok(0),
        [value] => Ok(*value),
        [_first, _second, ..] => Err(ProtocolError::Invalid(duplicate_error)),
    }
}

fn required_u64(
    fields: &BTreeMap<u32, Vec<u64>>,
    field: u32,
    missing: &'static str,
) -> Result<u64, ProtocolError> {
    match fields.get(&field).map(Vec::as_slice).unwrap_or_default() {
        [] => Err(ProtocolError::Invalid(missing)),
        [value] => Ok(*value),
        [_first, _second, ..] => Err(ProtocolError::Invalid("typed request has duplicate field")),
    }
}

fn required_field(
    fields: &BTreeMap<u32, Vec<u64>>,
    field: u32,
    missing: &'static str,
) -> Result<i32, ProtocolError> {
    match fields.get(&field).map(Vec::as_slice).unwrap_or_default() {
        [] => Err(ProtocolError::Invalid(missing)),
        [value] => to_i32(*value, "typed request field is out of range"),
        [_first, _second, ..] => Err(ProtocolError::Invalid("typed request has duplicate field")),
    }
}

fn to_i32(value: u64, error: &'static str) -> Result<i32, ProtocolError> {
    i32::try_from(value).map_err(|_| ProtocolError::Invalid(error))
}

pub struct TMessageCodec;

impl TMessageCodec {
    pub fn encode_request(value: &TRequest) -> Vec<u8> {
        let mut output = Vec::new();
        if !value.method.is_empty() {
            write_bytes(&mut output, 1, value.method.as_bytes());
        }
        if let Some(args) = &value.args {
            write_bytes(&mut output, 2, args);
        }
        if value.callback_handler != 0 {
            write_varint_field(&mut output, 3, value.callback_handler as u64);
        }
        if !value.token.is_empty() {
            write_bytes(&mut output, 4, value.token.as_bytes());
        }
        output
    }

    pub fn decode_request(payload: &[u8]) -> Result<TRequest, ProtocolError> {
        let mut reader = PbReader::new(payload);
        let mut value = TRequest::default();
        while let Some((field, wire)) = reader.next_field()? {
            match (field, wire) {
                (1, 2) => value.method = reader.read_string()?,
                (2, 2) => value.args = Some(reader.read_bytes()?.to_vec()),
                (3, 0) => value.callback_handler = reader.read_varint()? as u32,
                (4, 2) => value.token = reader.read_string()?,
                (_, wire) => reader.skip(wire)?,
            }
        }
        Ok(value)
    }

    pub fn encode_response(value: &TResponse) -> Vec<u8> {
        let mut output = Vec::new();
        if value.err != 0 {
            write_varint_field(&mut output, 1, value.err as u32 as u64);
        }
        if !value.err_msg.is_empty() {
            write_bytes(&mut output, 2, value.err_msg.as_bytes());
        }
        if !value.method.is_empty() {
            write_bytes(&mut output, 3, value.method.as_bytes());
        }
        if let Some(ret) = &value.ret {
            write_bytes(&mut output, 4, ret);
        }
        if value.callback_handler != 0 {
            write_varint_field(&mut output, 5, value.callback_handler as u64);
        }
        if value.time != 0 {
            write_varint_field(&mut output, 6, value.time as u64);
        }
        if !value.token.is_empty() {
            write_bytes(&mut output, 7, value.token.as_bytes());
        }
        if value.seq != 0 {
            write_varint_field(&mut output, 8, value.seq as u64);
        }
        if value.is_response != 0 {
            write_varint_field(&mut output, 9, value.is_response as u32 as u64);
        }
        output
    }

    pub fn decode_response(payload: &[u8]) -> Result<TResponse, ProtocolError> {
        let mut reader = PbReader::new(payload);
        let mut value = TResponse::default();
        while let Some((field, wire)) = reader.next_field()? {
            match (field, wire) {
                (1, 0) => value.err = reader.read_varint()? as i32,
                (2, 2) => value.err_msg = reader.read_string()?,
                (3, 2) => value.method = reader.read_string()?,
                (4, 2) => value.ret = Some(reader.read_bytes()?.to_vec()),
                (5, 0) => value.callback_handler = reader.read_varint()? as u32,
                (6, 0) => value.time = reader.read_varint()? as u32,
                (7, 2) => value.token = reader.read_string()?,
                (8, 0) => value.seq = reader.read_varint()? as u32,
                (9, 0) => value.is_response = reader.read_varint()? as i32,
                (_, wire) => reader.skip(wire)?,
            }
        }
        Ok(value)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TSampleInfo {
    pub uuid: String,
    pub model: String,
    pub release: String,
    pub network: String,
    pub platform: String,
    pub pkg_name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TArgLogin {
    pub pid: String,
    pub timestamp: i32,
    pub open_date_time: String,
    pub hash: String,
    pub sample_info: Option<TSampleInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TRetLogin {
    pub ret: String,
    pub feign_role_id: String,
    pub err_code: i32,
}

pub struct GameLoginCodec;

impl GameLoginCodec {
    pub fn encode_login(value: &TArgLogin) -> Vec<u8> {
        let mut output = Vec::new();
        write_string(&mut output, 1, &value.pid);
        write_int32(&mut output, 2, value.timestamp);
        write_string(&mut output, 3, &value.open_date_time);
        write_string(&mut output, 4, &value.hash);
        if let Some(sample) = &value.sample_info {
            let mut nested = Vec::new();
            write_string(&mut nested, 1, &sample.uuid);
            write_string(&mut nested, 2, &sample.model);
            write_string(&mut nested, 3, &sample.release);
            write_string(&mut nested, 4, &sample.network);
            write_string(&mut nested, 5, &sample.platform);
            write_string(&mut nested, 6, &sample.pkg_name);
            write_bytes(&mut output, 5, &nested);
        }
        output
    }

    pub fn decode_login(payload: &[u8]) -> Result<TArgLogin, ProtocolError> {
        let mut reader = PbReader::new(payload);
        let mut value = TArgLogin::default();
        while let Some((field, wire)) = reader.next_field()? {
            match (field, wire) {
                (1, 2) => value.pid = reader.read_string()?,
                (2, 0) => value.timestamp = reader.read_varint()? as i32,
                (3, 2) => value.open_date_time = reader.read_string()?,
                (4, 2) => value.hash = reader.read_string()?,
                (5, 2) => value.sample_info = Some(decode_sample_info(reader.read_bytes()?)?),
                (_, wire) => reader.skip(wire)?,
            }
        }
        Ok(value)
    }

    pub fn encode_response(value: &TRetLogin) -> Vec<u8> {
        let mut output = Vec::new();
        write_string(&mut output, 1, &value.ret);
        write_string(&mut output, 2, &value.feign_role_id);
        // Client checks ErrCode explicitly, so field 3 is always present.
        write_varint_field(&mut output, 3, value.err_code as u32 as u64);
        output
    }

    pub fn decode_login_response(payload: &[u8]) -> Result<TRetLogin, ProtocolError> {
        let mut reader = PbReader::new(payload);
        let mut value = TRetLogin::default();
        while let Some((field, wire)) = reader.next_field()? {
            match (field, wire) {
                (1, 2) => value.ret = reader.read_string()?,
                (2, 2) => value.feign_role_id = reader.read_string()?,
                (3, 0) => value.err_code = reader.read_varint()? as i32,
                (_, wire) => reader.skip(wire)?,
            }
        }
        Ok(value)
    }
}

pub struct UserLoginCodec;

impl UserLoginCodec {
    pub fn encode_response(ret: &str, ban_msg: &str, ban_time: i32) -> Vec<u8> {
        let mut output = Vec::new();
        if !ret.is_empty() {
            write_bytes(&mut output, 1, ret.as_bytes());
        }
        if !ban_msg.is_empty() {
            write_bytes(&mut output, 2, ban_msg.as_bytes());
        }
        if ban_time != 0 {
            write_varint_field(&mut output, 3, ban_time as u32 as u64);
        }
        output
    }
}

pub struct GuideInfoCodec;

impl GuideInfoCodec {
    pub fn encode_initial_progress_completed() -> Vec<u8> {
        let mut output = Vec::new();
        write_varint_field(&mut output, 1, 0);
        write_varint_field(&mut output, 2, 0);
        // Skip login/startup tutorial stages. Keep feature-unlock stages
        // incomplete so their guides can still play when unlocked later.
        const INITIAL_DONE_STAGES: [&str; 6] =
            ["10000", "100000", "1000000", "99995", "99998", "99992"];
        let done_stages = format!(
            "{{{}}}",
            INITIAL_DONE_STAGES
                .iter()
                .map(|id| format!("[\"{id}\"]=1"))
                .collect::<Vec<_>>()
                .join(",")
        );
        for (key, value) in [
            ("GUIDE_DONE_STAGES", done_stages.as_str()),
            ("GUIDE_DOING_STAGE", ""),
        ] {
            let mut setting = Vec::new();
            write_bytes(&mut setting, 1, key.as_bytes());
            write_bytes(&mut setting, 2, value.as_bytes());
            write_bytes(&mut output, 3, &setting);
        }
        write_bytes(&mut output, 4, &[0x08, 0x00, 0x10, 0x00]);
        output
    }
}

pub struct CopyInfoCodec;

impl CopyInfoCodec {
    pub fn encode(copy_type: i32, copy_ids: &[i32], max_copy_id: i32) -> Vec<u8> {
        Self::encode_with_progress(copy_type, copy_ids, max_copy_id, copy_ids)
    }

    pub fn encode_with_progress(
        copy_type: i32,
        copy_ids: &[i32],
        max_copy_id: i32,
        passed_copy_ids: &[i32],
    ) -> Vec<u8> {
        let passed_copy_counts = passed_copy_ids
            .iter()
            .copied()
            .map(|copy_id| (copy_id, 1))
            .collect::<Vec<_>>();
        Self::encode_with_progress_and_difficulty_for_type(
            copy_type,
            copy_ids,
            max_copy_id,
            passed_copy_ids,
            &passed_copy_counts,
            1,
        )
    }

    /// Encodes sea-copy progress with selected safe-area difficulty.
    pub fn encode_with_progress_and_difficulty(
        copy_ids: &[i32],
        max_copy_id: i32,
        passed_copy_ids: &[i32],
        difficulty: i32,
    ) -> Vec<u8> {
        let passed_copy_counts = passed_copy_ids
            .iter()
            .copied()
            .map(|copy_id| (copy_id, 1))
            .collect::<Vec<_>>();
        Self::encode_with_progress_and_difficulty_and_counts(
            copy_ids,
            max_copy_id,
            passed_copy_ids,
            &passed_copy_counts,
            difficulty,
        )
    }

    /// Encodes sea-copy progress with actual repeat-clear counts.
    pub fn encode_with_progress_and_difficulty_and_counts(
        copy_ids: &[i32],
        max_copy_id: i32,
        passed_copy_ids: &[i32],
        passed_copy_counts: &[(i32, i32)],
        difficulty: i32,
    ) -> Vec<u8> {
        Self::encode_with_progress_and_difficulty_for_type(
            2,
            copy_ids,
            max_copy_id,
            passed_copy_ids,
            passed_copy_counts,
            difficulty,
        )
    }

    fn encode_with_progress_and_difficulty_for_type(
        copy_type: i32,
        copy_ids: &[i32],
        max_copy_id: i32,
        passed_copy_ids: &[i32],
        passed_copy_counts: &[(i32, i32)],
        difficulty: i32,
    ) -> Vec<u8> {
        let mut output = Vec::new();
        for copy_id in copy_ids {
            let mut entry = Vec::new();
            write_varint_field(&mut entry, 1, *copy_id as u32 as u64);
            write_varint_field(&mut entry, 2, 0);
            let passed = passed_copy_ids.contains(copy_id);
            write_varint_field(&mut entry, 3, u64::from(passed) * 7);
            write_varint_field(&mut entry, 4, 0);
            write_varint_field(&mut entry, 5, 0);
            write_varint_field(&mut entry, 6, u64::from(passed));
            if copy_type == 2 || copy_type == 9 {
                write_varint_field(&mut entry, 8, 1);
                write_fixed32_field(&mut entry, 9, 0);
                write_varint_field(&mut entry, 12, difficulty.max(1) as u64);
            }
            write_bytes(&mut output, 1, &entry);
        }
        for copy_id in passed_copy_ids {
            let mut count = Vec::new();
            write_varint_field(&mut count, 1, *copy_id as u32 as u64);
            let pass_count = passed_copy_counts
                .iter()
                .find(|(id, _)| id == copy_id)
                .map(|(_, count)| *count)
                .unwrap_or(1);
            write_varint_field(&mut count, 2, pass_count.max(1) as u64);
            write_bytes(&mut output, 5, &count);
        }
        write_varint_field(&mut output, 2, max_copy_id as u32 as u64);
        write_varint_field(&mut output, 3, copy_type as u32 as u64);
        output
    }
}

pub struct DailyCopyCodec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyCopyProgress {
    pub chapter_id: i32,
    pub challenge_times: i32,
    pub pass_copy: Vec<i32>,
    pub select_ex: bool,
    pub ex_star: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyCopyGroupProgress {
    pub group_id: i32,
    pub success_times: i32,
}

impl DailyCopyCodec {
    pub fn encode_default() -> Vec<u8> {
        Self::encode(&[(1, 1)], &[1])
    }

    pub fn encode(chapters: &[(i32, i32)], groups: &[i32]) -> Vec<u8> {
        Self::encode_with_progress(chapters, groups, &[], &[], &[])
    }

    pub fn encode_with_progress(
        chapters: &[(i32, i32)],
        groups: &[i32],
        chapter_progress: &[DailyCopyProgress],
        group_progress: &[DailyCopyGroupProgress],
        extra_group_progress: &[DailyCopyGroupProgress],
    ) -> Vec<u8> {
        let mut chapter = Vec::new();
        let mut output = Vec::new();
        for (chapter_id, _group_id) in chapters {
            let progress = chapter_progress
                .iter()
                .find(|item| item.chapter_id == *chapter_id);
            chapter.clear();
            write_varint_field(&mut chapter, 1, *chapter_id as u32 as u64);
            write_varint_field(
                &mut chapter,
                2,
                progress.map_or(0, |item| item.challenge_times) as u32 as u64,
            );
            if let Some(progress) = progress {
                for copy_id in &progress.pass_copy {
                    write_varint_field(&mut chapter, 3, *copy_id as u32 as u64);
                }
            }
            write_varint_field(
                &mut chapter,
                4,
                u64::from(progress.is_some_and(|item| item.select_ex)),
            );
            write_varint_field(
                &mut chapter,
                5,
                progress.map_or(0, |item| item.ex_star) as u32 as u64,
            );
            write_bytes(&mut output, 1, &chapter);
        }
        for field in [2, 3] {
            for group_id in groups {
                let mut group = Vec::new();
                write_varint_field(&mut group, 1, *group_id as u32 as u64);
                let progress = if field == 2 {
                    group_progress
                        .iter()
                        .find(|item| item.group_id == *group_id)
                } else {
                    extra_group_progress
                        .iter()
                        .find(|item| item.group_id == *group_id)
                };
                write_varint_field(
                    &mut group,
                    2,
                    progress.map_or(0, |item| item.success_times) as u32 as u64,
                );
                write_bytes(&mut output, field, &group);
            }
        }
        output
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MedalAcquiredTime {
    pub medal_id: i32,
    pub time: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserInfo {
    pub uid: u64,
    pub uname: String,
    pub level: i32,
    pub class_id: i32,
    pub secretary_id: u32,
    pub create_time: i32,
    pub gold: i32,
    pub diamond: i32,
    pub supply: i32,
    pub pve_pt: i32,
    pub head: i32,
    pub head_frame: i32,
    pub exp: i32,
    pub buy_gold_num: i32,
    pub buy_gold_time: i32,
    pub buy_supply_num: i32,
    pub buy_supply_time: i32,
    pub head_show: i32,
    pub new_task_stage: i32,
    pub server_id: i32,
    pub bath: i32,
    pub main_gun: i32,
    pub torpedo: i32,
    pub plane: i32,
    pub other: i32,
    pub retire: i32,
    pub strategy: i32,
    pub medal: i32,
    pub tower: i32,
    pub copy_train_point: i32,
    pub fashion_point: i32,
    pub guild_contri: i32,
    pub lucky: i32,
    pub teacher_medal: i32,
    pub teacher_prestige: i32,
    pub battle_pass_exp: i32,
    pub battle_pass_gold: i32,
    pub guild_coin_ii: i32,
    pub ur_equip_coin: i32,
    pub activity_battle_pass_exp: i32,
    pub get_hero_count: i32,
    pub attack_count: i32,
    pub married_num: i32,
    pub achieve_point: i32,
    pub message: String,
    pub medal_acquired_times: Vec<MedalAcquiredTime>,
}

pub struct UserInfoCodec;

impl UserInfoCodec {
    pub fn encode(value: &UserInfo) -> Vec<u8> {
        let mut output = Vec::new();
        if value.uid != 0 {
            write_varint_field(&mut output, 1, value.uid);
        }
        write_string(&mut output, 2, &value.uname);
        write_int32(&mut output, 4, value.head);
        write_int32(&mut output, 5, value.head_frame);
        write_int32(&mut output, 7, value.class_id);
        write_int32(&mut output, 10, value.level);
        write_varint_field(&mut output, 11, value.exp as u32 as u64);
        write_varint_field(&mut output, 12, value.diamond as u32 as u64);
        write_varint_field(&mut output, 13, value.gold as u32 as u64);
        write_varint_field(&mut output, 14, value.supply as u32 as u64);
        write_varint_field(&mut output, 15, value.main_gun as u32 as u64);
        write_varint_field(&mut output, 16, value.torpedo as u32 as u64);
        write_varint_field(&mut output, 17, value.plane as u32 as u64);
        write_varint_field(&mut output, 18, value.other as u32 as u64);
        write_varint_field(&mut output, 22, value.create_time as u32 as u64);
        write_varint_field(&mut output, 23, u64::from(value.secretary_id));
        write_string(&mut output, 25, &value.message);
        write_varint_field(&mut output, 26, value.buy_gold_num as u32 as u64);
        write_varint_field(&mut output, 27, value.buy_gold_time as u32 as u64);
        write_varint_field(&mut output, 28, value.buy_supply_num as u32 as u64);
        write_varint_field(&mut output, 29, value.buy_supply_time as u32 as u64);
        write_varint_field(&mut output, 30, value.retire as u32 as u64);
        write_varint_field(&mut output, 32, value.achieve_point as u32 as u64);
        write_varint_field(&mut output, 35, value.bath as u32 as u64);
        write_varint_field(&mut output, 37, value.strategy as u32 as u64);
        write_varint_field(&mut output, 39, value.medal as u32 as u64);
        write_varint_field(&mut output, 40, value.attack_count as u32 as u64);
        write_varint_field(&mut output, 41, value.get_hero_count as u32 as u64);
        write_varint_field(&mut output, 45, value.married_num as u32 as u64);
        write_varint_field(&mut output, 44, value.head_show as u32 as u64);
        write_varint_field(&mut output, 46, value.new_task_stage.max(7) as u32 as u64);
        write_varint_field(&mut output, 47, value.copy_train_point as u32 as u64);
        write_varint_field(&mut output, 48, value.tower as u32 as u64);
        write_varint_field(&mut output, 49, value.fashion_point as u32 as u64);
        write_varint_field(&mut output, 50, value.lucky as u32 as u64);
        write_varint_field(&mut output, 51, value.teacher_medal as u32 as u64);
        write_varint_field(&mut output, 52, value.teacher_prestige as u32 as u64);
        write_varint_field(&mut output, 53, value.guild_contri as u32 as u64);
        write_varint_field(&mut output, 56, value.server_id.max(1) as u32 as u64);
        for medal in &value.medal_acquired_times {
            if medal.medal_id <= 0 || medal.time <= 0 {
                continue;
            }
            let mut entry = Vec::new();
            write_varint_field(&mut entry, 1, medal.medal_id as u32 as u64);
            write_varint_field(&mut entry, 2, medal.time as u32 as u64);
            write_bytes(&mut output, 57, &entry);
        }
        write_varint_field(&mut output, 58, value.battle_pass_exp as u32 as u64);
        write_varint_field(&mut output, 59, value.battle_pass_gold as u32 as u64);
        write_varint_field(&mut output, 62, value.pve_pt as u32 as u64);
        write_varint_field(&mut output, 63, value.guild_coin_ii as u32 as u64);
        write_varint_field(&mut output, 64, value.ur_equip_coin as u32 as u64);
        write_varint_field(
            &mut output,
            65,
            value.activity_battle_pass_exp as u32 as u64,
        );
        output
    }

    pub fn decode(payload: &[u8]) -> Result<UserInfo, ProtocolError> {
        let mut reader = PbReader::new(payload);
        let mut value = UserInfo::default();
        while let Some((field, wire)) = reader.next_field()? {
            match (field, wire) {
                (1, 0) => value.uid = reader.read_varint()?,
                (2, 2) => value.uname = reader.read_string()?,
                (4, 0) => value.head = reader.read_varint()? as i32,
                (5, 0) => value.head_frame = reader.read_varint()? as i32,
                (7, 0) => value.class_id = reader.read_varint()? as i32,
                (10, 0) => value.level = reader.read_varint()? as i32,
                (11, 0) => value.exp = reader.read_varint()? as i32,
                (12, 0) => value.diamond = reader.read_varint()? as i32,
                (13, 0) => value.gold = reader.read_varint()? as i32,
                (14, 0) => value.supply = reader.read_varint()? as i32,
                (15, 0) => value.main_gun = reader.read_varint()? as i32,
                (16, 0) => value.torpedo = reader.read_varint()? as i32,
                (17, 0) => value.plane = reader.read_varint()? as i32,
                (18, 0) => value.other = reader.read_varint()? as i32,
                (22, 0) => value.create_time = reader.read_varint()? as i32,
                (23, 0) => value.secretary_id = reader.read_varint()? as u32,
                (25, 2) => value.message = reader.read_string()?,
                (26, 0) => value.buy_gold_num = reader.read_varint()? as i32,
                (27, 0) => value.buy_gold_time = reader.read_varint()? as i32,
                (28, 0) => value.buy_supply_num = reader.read_varint()? as i32,
                (29, 0) => value.buy_supply_time = reader.read_varint()? as i32,
                (30, 0) => value.retire = reader.read_varint()? as i32,
                (32, 0) => value.achieve_point = reader.read_varint()? as i32,
                (35, 0) => value.bath = reader.read_varint()? as i32,
                (37, 0) => value.strategy = reader.read_varint()? as i32,
                (39, 0) => value.medal = reader.read_varint()? as i32,
                (40, 0) => value.attack_count = reader.read_varint()? as i32,
                (41, 0) => value.get_hero_count = reader.read_varint()? as i32,
                (45, 0) => value.married_num = reader.read_varint()? as i32,
                (44, 0) => value.head_show = reader.read_varint()? as i32,
                (46, 0) => value.new_task_stage = reader.read_varint()? as i32,
                (47, 0) => value.copy_train_point = reader.read_varint()? as i32,
                (48, 0) => value.tower = reader.read_varint()? as i32,
                (49, 0) => value.fashion_point = reader.read_varint()? as i32,
                (50, 0) => value.lucky = reader.read_varint()? as i32,
                (51, 0) => value.teacher_medal = reader.read_varint()? as i32,
                (52, 0) => value.teacher_prestige = reader.read_varint()? as i32,
                (53, 0) => value.guild_contri = reader.read_varint()? as i32,
                (56, 0) => value.server_id = reader.read_varint()? as i32,
                (57, 2) => {
                    let payload = reader.read_bytes()?.to_vec();
                    let mut nested = PbReader::new(&payload);
                    let mut medal = MedalAcquiredTime::default();
                    while let Some((nested_field, nested_wire)) = nested.next_field()? {
                        match (nested_field, nested_wire) {
                            (1, 0) => medal.medal_id = nested.read_varint()? as i32,
                            (2, 0) => medal.time = nested.read_varint()? as i32,
                            (_, nested_wire) => nested.skip(nested_wire)?,
                        }
                    }
                    if medal.medal_id > 0 && medal.time > 0 {
                        value.medal_acquired_times.push(medal);
                    }
                }
                (62, 0) => value.pve_pt = reader.read_varint()? as i32,
                (58, 0) => value.battle_pass_exp = reader.read_varint()? as i32,
                (59, 0) => value.battle_pass_gold = reader.read_varint()? as i32,
                (63, 0) => value.guild_coin_ii = reader.read_varint()? as i32,
                (64, 0) => value.ur_equip_coin = reader.read_varint()? as i32,
                (65, 0) => value.activity_battle_pass_exp = reader.read_varint()? as i32,
                (_, wire) => reader.skip(wire)?,
            }
        }
        Ok(value)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PSkillEntry {
    pub pskill_id: u32,
    pub pskill_exp: u32,
    pub level: i32,
    pub replace: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttrIntensify {
    pub attr_type: i32,
    pub intensify_level: i32,
    pub cur_exp: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeroEquipSlot {
    pub equip_id: u32,
    pub state: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeroEquipGroup {
    pub equip_type: i32,
    pub slots: Vec<HeroEquipSlot>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EquipEffectInfo {
    pub effect_type: i32,
    pub effect_ids: Vec<i32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeroCombinationInfo {
    pub com_lv: i32,
    pub com_grade: i32,
    pub combine: u32,
    pub be_combined: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeroGrid {
    pub hero_id: u32,
    pub template_id: i32,
    pub level: i32,
    pub fashioning: i32,
    pub exp: i32,
    pub create_time: i32,
    pub update_time: i32,
    pub affection: i32,
    pub marry_time: i32,
    pub cur_hp: i64,
    pub mood: i32,
    pub marry_type: i32,
    pub equip_slots: Vec<u32>,
    pub name: String,
    pub change_name_time: i32,
    pub lock: bool,
    pub advance: i32,
    pub adv_lv: i32,
    pub remould_effects: Vec<i32>,
    pub remould_level: i32,
    pub pskills: Vec<PSkillEntry>,
    pub intensify: Vec<AttrIntensify>,
    pub equip_groups: Vec<HeroEquipGroup>,
    pub equip_effects: Vec<EquipEffectInfo>,
    pub combination_info: HeroCombinationInfo,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeroBag {
    pub heroes: Vec<HeroGrid>,
    pub bag_size: i32,
}

pub struct HeroBagCodec;

impl HeroBagCodec {
    pub fn encode(value: &HeroBag) -> Vec<u8> {
        let mut output = Vec::new();
        for hero in &value.heroes {
            write_bytes(&mut output, 1, &Self::encode_hero(hero));
        }
        if value.bag_size != 0 {
            write_varint_field(&mut output, 2, value.bag_size as u32 as u64);
        }
        output
    }

    fn encode_hero(value: &HeroGrid) -> Vec<u8> {
        let mut output = Vec::new();
        if value.hero_id != 0 {
            write_varint_field(&mut output, 1, u64::from(value.hero_id));
        }
        write_varint_field(&mut output, 2, value.template_id as u32 as u64);

        let normal_states = value
            .equip_groups
            .iter()
            .find(|group| group.equip_type == 1)
            .map(|group| group.slots.as_slice())
            .unwrap_or(&[]);
        let mut equip_groups = value.equip_groups.clone();
        if !equip_groups.iter().any(|group| group.equip_type == 1) {
            equip_groups.insert(
                0,
                HeroEquipGroup {
                    equip_type: 1,
                    slots: (0..6)
                        .map(|index| HeroEquipSlot {
                            equip_id: value.equip_slots.get(index).copied().unwrap_or_default(),
                            state: 0,
                        })
                        .collect(),
                },
            );
        }
        for group in equip_groups {
            let mut equips_by_type = Vec::new();
            write_varint_field(&mut equips_by_type, 1, group.equip_type as u32 as u64);
            for index in 0..6 {
                let slot = group
                    .slots
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| HeroEquipSlot {
                        equip_id: if group.equip_type == 1 {
                            value.equip_slots.get(index).copied().unwrap_or_default()
                        } else {
                            0
                        },
                        state: normal_states
                            .get(index)
                            .map(|slot| slot.state)
                            .unwrap_or_default(),
                    });
                let mut equip = Vec::new();
                write_varint_field(&mut equip, 1, u64::from(slot.equip_id));
                write_varint_field(&mut equip, 2, slot.state as u32 as u64);
                write_bytes(&mut equips_by_type, 2, &equip);
            }
            write_bytes(&mut output, 3, &equips_by_type);
        }

        if value.level != 0 {
            write_varint_field(&mut output, 4, value.level as u32 as u64);
        }
        write_varint_field(&mut output, 5, value.exp as u32 as u64);
        write_varint_field(&mut output, 6, value.advance as u32 as u64);
        if value.create_time != 0 {
            write_varint_field(&mut output, 8, value.create_time as u32 as u64);
        }
        if value.cur_hp != 0 {
            write_varint_field(&mut output, 9, value.cur_hp as u64);
        }
        for attr in &value.intensify {
            let mut body = Vec::new();
            write_varint_field(&mut body, 1, attr.attr_type as u32 as u64);
            write_varint_field(&mut body, 2, attr.intensify_level as u32 as u64);
            write_varint_field(&mut body, 3, attr.cur_exp as u32 as u64);
            write_bytes(&mut output, 7, &body);
        }
        if value.pskills.is_empty() {
            // Client reads at least one skill entry during hero page initialization.
            write_bytes(
                &mut output,
                13,
                &[0x08, 0xFA, 0xC1, 0x02, 0x10, 0x00, 0x18, 0x00, 0x20, 0x00],
            );
        } else {
            for skill in &value.pskills {
                let mut body = Vec::new();
                let skill_id = if skill.pskill_id == 0 {
                    41_210
                } else {
                    skill.pskill_id
                };
                write_varint_field(&mut body, 1, u64::from(skill_id));
                write_varint_field(&mut body, 2, u64::from(skill.pskill_exp));
                write_varint_field(
                    &mut body,
                    3,
                    u64::from(if skill.level > 0 { skill.level } else { 1 } as u32),
                );
                write_varint_field(&mut body, 4, skill.replace as u32 as u64);
                write_bytes(&mut output, 13, &body);
            }
        }
        write_varint_field(&mut output, 12, u64::from(value.lock));
        write_varint_field(&mut output, 16, value.change_name_time as u32 as u64);
        write_varint_field(&mut output, 17, value.affection as u32 as u64);
        write_varint_field(&mut output, 18, value.mood as u32 as u64);
        write_varint_field(&mut output, 19, value.marry_time as u32 as u64);
        if value.update_time != 0 {
            write_varint_field(&mut output, 20, value.update_time as u32 as u64);
        }
        write_varint_field(&mut output, 21, value.marry_type as u32 as u64);
        if value.fashioning != 0 {
            write_varint_field(&mut output, 22, value.fashioning as u32 as u64);
        }
        for effect in &value.remould_effects {
            write_varint_field(&mut output, 23, *effect as u32 as u64);
        }
        write_varint_field(&mut output, 24, value.remould_level as u32 as u64);
        write_varint_field(&mut output, 25, value.adv_lv as u32 as u64);
        for effect in &value.equip_effects {
            let mut body = Vec::new();
            write_varint_field(&mut body, 1, effect.effect_type as u32 as u64);
            for effect_id in &effect.effect_ids {
                write_varint_field(&mut body, 2, *effect_id as u32 as u64);
            }
            write_bytes(&mut output, 26, &body);
        }
        let mut combination = Vec::new();
        write_varint_field(
            &mut combination,
            1,
            value.combination_info.com_lv as u32 as u64,
        );
        write_varint_field(
            &mut combination,
            2,
            value.combination_info.com_grade as u32 as u64,
        );
        write_varint_field(
            &mut combination,
            3,
            u64::from(value.combination_info.combine),
        );
        write_varint_field(
            &mut combination,
            4,
            u64::from(value.combination_info.be_combined),
        );
        write_bytes(&mut output, 27, &combination);
        write_string_always(&mut output, 15, &value.name);
        output
    }
}

pub struct UserListCodec;

impl UserListCodec {
    pub fn encode(users: &[UserInfo]) -> Vec<u8> {
        let mut output = Vec::new();
        for user in users {
            write_bytes(&mut output, 1, &PlayerUserCodec::encode(user));
        }
        output
    }
}

pub struct PlayerUserCodec;

impl PlayerUserCodec {
    pub fn encode(value: &UserInfo) -> Vec<u8> {
        let mut output = Vec::new();
        if value.uid != 0 {
            write_varint_field(&mut output, 1, value.uid);
        }
        write_string(&mut output, 2, &value.uname);
        write_int32(&mut output, 3, value.level);
        write_int32(&mut output, 4, value.class_id);
        output
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BagGrid {
    pub template_id: i32,
    pub num: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BagInfo {
    pub bag_type: i32,
    pub bag_size: i32,
    pub items: Vec<BagGrid>,
}

pub struct BagInfoCodec;

impl BagInfoCodec {
    pub fn encode(value: &BagInfo) -> Vec<u8> {
        let mut output = Vec::new();
        if value.bag_type != 0 {
            write_varint_field(&mut output, 1, value.bag_type as u32 as u64);
        }
        if value.bag_size != 0 {
            write_varint_field(&mut output, 2, value.bag_size as u32 as u64);
        }
        for item in &value.items {
            let mut body = Vec::new();
            if item.template_id != 0 {
                write_varint_field(&mut body, 1, item.template_id as u32 as u64);
            }
            // Zero is deletion marker; always emit Num to avoid nil on Lua client.
            write_varint_field(&mut body, 2, item.num as u32 as u64);
            write_bytes(&mut output, 3, &body);
        }
        output
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FashionInfo {
    pub sf_id: i32,
    pub fashion_tids: Vec<i32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FashionList {
    pub items: Vec<FashionInfo>,
}

pub struct FashionListCodec;

impl FashionListCodec {
    pub fn encode(value: &FashionList) -> Vec<u8> {
        let mut output = Vec::new();
        for item in &value.items {
            let mut body = Vec::new();
            if item.sf_id != 0 {
                write_varint_field(&mut body, 1, item.sf_id as u32 as u64);
            }
            for fashion_tid in &item.fashion_tids {
                write_varint_field(&mut body, 2, *fashion_tid as u32 as u64);
            }
            write_bytes(&mut output, 1, &body);
        }
        output
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EquipPSkill {
    pub pskill_id: i32,
    pub level: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EquipInfo {
    pub equip_id: u32,
    pub template_id: i32,
    pub enhance_level: i32,
    pub star: i32,
    pub hero_id: u32,
    pub enhance_exp: i32,
    pub pskills: Vec<EquipPSkill>,
    pub rise_common_equips: Vec<EquipNum>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EquipNum {
    pub template_id: i32,
    pub num: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EquipList {
    pub bag_size: i32,
    pub items: Vec<EquipInfo>,
    pub nums: Vec<EquipNum>,
}

pub struct EquipListCodec;

impl EquipListCodec {
    pub fn encode(value: &EquipList) -> Vec<u8> {
        let mut output = Vec::new();
        write_varint_field(&mut output, 1, value.bag_size as u32 as u64);
        for item in &value.items {
            write_bytes(&mut output, 2, &Self::encode_item(item));
        }
        for item in &value.nums {
            let mut body = Vec::new();
            if item.template_id != 0 {
                write_varint_field(&mut body, 1, item.template_id as u32 as u64);
            }
            // Zero is deletion marker; always emit Num so client clears stale
            // one-click enhancement material counts without relogin.
            write_varint_field(&mut body, 2, item.num as u32 as u64);
            write_bytes(&mut output, 3, &body);
        }
        output
    }

    /// Encode one TEquipInfo payload (used by equipment operation responses).
    pub fn encode_item(value: &EquipInfo) -> Vec<u8> {
        let mut output = Vec::new();
        if value.equip_id != 0 {
            write_varint_field(&mut output, 1, u64::from(value.equip_id));
        }
        write_varint_field(&mut output, 2, value.template_id as u32 as u64);
        write_varint_field(&mut output, 3, value.enhance_level as u32 as u64);
        write_varint_field(&mut output, 4, value.star as u32 as u64);
        write_varint_field(&mut output, 5, u64::from(value.hero_id));
        write_varint_field(&mut output, 6, value.enhance_exp as u32 as u64);
        for skill in &value.pskills {
            let mut body = Vec::new();
            if skill.pskill_id != 0 {
                write_varint_field(&mut body, 1, skill.pskill_id as u32 as u64);
            }
            if skill.level != 0 {
                write_varint_field(&mut body, 2, skill.level as u32 as u64);
            }
            write_bytes(&mut output, 7, &body);
        }
        for item in &value.rise_common_equips {
            let mut body = Vec::new();
            write_varint_field(&mut body, 1, item.template_id as u32 as u64);
            write_varint_field(&mut body, 2, item.num as u32 as u64);
            write_bytes(&mut output, 8, &body);
        }
        output
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildingInfo {
    pub id: i32,
    pub template_id: i32,
    pub level: i32,
    pub hero_ids: Vec<u32>,
    pub productivity: i32,
    pub produce_speed: i32,
    pub product_count: i32,
    pub status: i32,
    pub last_update_time: i64,
    pub recipe_id: i32,
    pub item_count: i32,
    pub last_mood_update_time: i64,
    pub last_build_update_time: i64,
    pub recipe_time: i32,
    pub float_count: i32,
    pub tactic_list: Vec<BuildingTactic>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildingTactic {
    pub building_id: i32,
    pub name: String,
    pub hero_ids: Vec<u32>,
    pub index: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildingLandInfo {
    pub index: i32,
    pub building_id: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserBuildingInfo {
    pub buildings: Vec<BuildingInfo>,
    pub lands: Vec<BuildingLandInfo>,
    pub worker_strength: i32,
    pub worker_recover: i32,
    pub food_max: i32,
    pub electric_max: i32,
    pub worker_update_time: i64,
}

pub struct UserBuildingInfoCodec;

impl UserBuildingInfoCodec {
    pub fn encode(value: &UserBuildingInfo) -> Vec<u8> {
        let mut output = Vec::new();
        for building in &value.buildings {
            write_bytes(&mut output, 1, &Self::encode_building(building));
        }
        for land in &value.lands {
            let mut body = Vec::new();
            write_varint_field(&mut body, 1, land.index as u32 as u64);
            write_varint_field(&mut body, 2, land.building_id as u32 as u64);
            write_bytes(&mut output, 2, &body);
        }
        write_varint_field(&mut output, 3, value.worker_strength as u32 as u64);
        write_varint_field(&mut output, 4, value.worker_recover as u32 as u64);
        write_varint_field(&mut output, 5, 0);
        write_varint_field(&mut output, 6, value.food_max as u32 as u64);
        write_varint_field(&mut output, 7, 0);
        write_varint_field(&mut output, 8, value.electric_max as u32 as u64);
        write_varint_field(&mut output, 9, value.worker_update_time as u64);
        write_varint_field(&mut output, 10, value.worker_update_time as u64);
        output
    }

    fn encode_building(value: &BuildingInfo) -> Vec<u8> {
        let mut output = Vec::new();
        write_varint_field(&mut output, 1, value.id as u32 as u64);
        write_varint_field(&mut output, 2, value.template_id as u32 as u64);
        write_varint_field(&mut output, 3, value.level as u32 as u64);
        for hero_id in &value.hero_ids {
            write_varint_field(&mut output, 4, u64::from(*hero_id));
        }
        write_varint_field(&mut output, 5, value.productivity as u32 as u64);
        write_varint_field(&mut output, 6, value.produce_speed as u32 as u64);
        write_varint_field(&mut output, 7, value.product_count as u32 as u64);
        write_varint_field(&mut output, 8, value.status as u32 as u64);
        write_varint_field(&mut output, 9, value.last_update_time as u64);
        write_varint_field(&mut output, 10, value.recipe_id as u32 as u64);
        write_varint_field(&mut output, 11, value.item_count as u32 as u64);
        write_varint_field(&mut output, 12, value.last_mood_update_time as u64);
        write_varint_field(&mut output, 13, value.last_build_update_time as u64);
        write_varint_field(&mut output, 15, value.recipe_time as u32 as u64);
        write_varint_field(&mut output, 16, value.float_count as u32 as u64);
        for tactic in &value.tactic_list {
            let mut body = Vec::new();
            write_varint_field(&mut body, 1, tactic.building_id as u32 as u64);
            write_string_always(&mut body, 2, &tactic.name);
            for hero_id in &tactic.hero_ids {
                write_varint_field(&mut body, 3, u64::from(*hero_id));
            }
            write_varint_field(&mut body, 4, tactic.index as u32 as u64);
            write_bytes(&mut output, 17, &body);
        }
        output
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FleetTactic {
    pub tactic_name: String,
    pub hero_ids: Vec<i32>,
    pub mode_id: i32,
    pub strategy_id: i32,
    pub formation_id: i32,
    pub tactic_type: i32,
    pub ex_hero_ids: Vec<i32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FleetInfo {
    pub tactics: Vec<FleetTactic>,
    pub max_power: i32,
    pub min_power: i32,
}

pub struct FleetInfoCodec;

impl FleetInfoCodec {
    pub fn encode(value: &FleetInfo) -> Vec<u8> {
        let mut output = Vec::new();
        for tactic in &value.tactics {
            let mut body = Vec::new();
            write_string_always(&mut body, 1, &tactic.tactic_name);
            for hero_id in &tactic.hero_ids {
                write_varint_field(&mut body, 2, *hero_id as u32 as u64);
            }
            write_varint_field(&mut body, 3, tactic.mode_id as u32 as u64);
            write_varint_field(&mut body, 4, tactic.strategy_id as u32 as u64);
            write_varint_field(&mut body, 5, tactic.formation_id as u32 as u64);
            write_varint_field(&mut body, 6, tactic.tactic_type as u32 as u64);
            for hero_id in &tactic.ex_hero_ids {
                write_varint_field(&mut body, 7, *hero_id as u32 as u64);
            }
            write_bytes(&mut output, 1, &body);
        }
        if value.max_power != 0 {
            write_varint_field(&mut output, 2, value.max_power as u32 as u64);
        }
        if value.min_power != 0 {
            write_varint_field(&mut output, 3, value.min_power as u32 as u64);
        }
        output
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CopyRecordEquip {
    pub template_id: i32,
    pub level: i32,
    pub star_level: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CopyRecordHero {
    pub template_id: i32,
    pub level: i32,
    pub advance_level: i32,
    pub cur_hp: u64,
    pub equips: Vec<CopyRecordEquip>,
    pub point: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CopyRecord {
    pub uid: u64,
    pub user_name: String,
    pub level: i32,
    pub pass_time: i32,
    pub secret_id: i32,
    pub strategy_id: i32,
    pub tactics: Vec<CopyRecordHero>,
    pub power: i32,
    pub record_time: i32,
    pub ex_buff: Vec<i32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CopyRecordList {
    pub copy_id: i32,
    pub records: Vec<CopyRecord>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CopyInfoResponse {
    pub first: Option<CopyRecord>,
    pub fast: Option<CopyRecord>,
    pub atk_grad: Option<CopyRecord>,
    pub max_ex_star: i32,
    pub max_ex_star_first: Option<CopyRecord>,
    pub max_ex_star_fast: Option<CopyRecord>,
}

pub struct CopyRecordListCodec;

impl CopyRecordListCodec {
    pub fn encode(value: &CopyRecordList) -> Vec<u8> {
        let mut output = Vec::new();
        write_varint_field(&mut output, 1, value.copy_id.max(0) as u64);
        for record in &value.records {
            let mut body = Vec::new();
            write_varint_field(&mut body, 1, record.uid);
            write_string(&mut body, 2, &record.user_name);
            write_varint_field(&mut body, 3, record.level.max(0) as u64);
            write_varint_field(&mut body, 4, record.pass_time.max(0) as u64);
            write_varint_field(&mut body, 5, record.secret_id.max(0) as u64);
            write_varint_field(&mut body, 6, record.strategy_id.max(0) as u64);
            for hero in &record.tactics {
                let mut hero_body = Vec::new();
                write_varint_field(&mut hero_body, 1, hero.template_id.max(0) as u64);
                write_varint_field(&mut hero_body, 2, hero.level.max(0) as u64);
                write_varint_field(&mut hero_body, 3, hero.advance_level.max(0) as u64);
                write_varint_field(&mut hero_body, 4, hero.cur_hp);
                for equip in &hero.equips {
                    let mut equip_body = Vec::new();
                    write_varint_field(&mut equip_body, 1, equip.template_id.max(0) as u64);
                    write_varint_field(&mut equip_body, 2, equip.level.max(0) as u64);
                    write_varint_field(&mut equip_body, 3, equip.star_level.max(0) as u64);
                    write_bytes(&mut hero_body, 5, &equip_body);
                }
                write_varint_field(&mut hero_body, 6, hero.point.max(0) as u64);
                write_bytes(&mut body, 7, &hero_body);
            }
            write_varint_field(&mut body, 8, record.power.max(0) as u64);
            write_varint_field(&mut body, 9, record.record_time.max(0) as u64);
            write_bytes(&mut output, 2, &body);
        }
        output
    }

    pub fn decode_copy_id(payload: &[u8]) -> Option<i32> {
        let mut reader = PbReader::new(payload);
        while let Ok(Some((field, wire))) = reader.next_field() {
            match (field, wire) {
                (1, 0) => return i32::try_from(reader.read_varint().ok()?).ok(),
                (_, wire) => reader.skip(wire).ok()?,
            }
        }
        None
    }
}

impl CopyInfoCodec {
    pub fn encode_record_response(value: &CopyInfoResponse) -> Vec<u8> {
        let mut output = Vec::new();
        for (field, record) in [
            (1, value.first.as_ref()),
            (2, value.fast.as_ref()),
            (3, value.atk_grad.as_ref()),
            (5, value.max_ex_star_first.as_ref()),
            (6, value.max_ex_star_fast.as_ref()),
        ] {
            if let Some(record) = record {
                write_bytes(&mut output, field, &Self::encode_record(record));
            }
        }
        if value.max_ex_star > 0 {
            write_varint_field(&mut output, 4, value.max_ex_star as u32 as u64);
        }
        output
    }

    fn encode_record(record: &CopyRecord) -> Vec<u8> {
        let mut body = Vec::new();
        write_varint_field(&mut body, 1, record.uid);
        write_string(&mut body, 2, &record.user_name);
        write_varint_field(&mut body, 3, record.level.max(0) as u64);
        write_varint_field(&mut body, 4, record.pass_time.max(0) as u64);
        write_varint_field(&mut body, 5, record.secret_id.max(0) as u64);
        write_varint_field(&mut body, 6, record.strategy_id.max(0) as u64);
        for hero in &record.tactics {
            let mut hero_body = Vec::new();
            write_varint_field(&mut hero_body, 1, hero.template_id.max(0) as u64);
            write_varint_field(&mut hero_body, 2, hero.level.max(0) as u64);
            write_varint_field(&mut hero_body, 3, hero.advance_level.max(0) as u64);
            write_varint_field(&mut hero_body, 4, hero.cur_hp);
            for equip in &hero.equips {
                let mut equip_body = Vec::new();
                write_varint_field(&mut equip_body, 1, equip.template_id.max(0) as u64);
                write_varint_field(&mut equip_body, 2, equip.level.max(0) as u64);
                write_varint_field(&mut equip_body, 3, equip.star_level.max(0) as u64);
                write_bytes(&mut hero_body, 5, &equip_body);
            }
            write_varint_field(&mut hero_body, 6, hero.point.max(0) as u64);
            write_bytes(&mut body, 7, &hero_body);
        }
        write_varint_field(&mut body, 8, record.power.max(0) as u64);
        write_varint_field(&mut body, 9, record.record_time.max(0) as u64);
        for ex_buff in &record.ex_buff {
            write_varint_field(&mut body, 10, *ex_buff as u32 as u64);
        }
        body
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PresetFleet {
    pub name: String,
    pub hero_ids: Vec<i32>,
    pub ex_hero_ids: Vec<i32>,
    pub mode_id: i32,
    pub strategy_id: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PresetFleetInfo {
    pub fleets: Vec<PresetFleet>,
    pub name_num: i32,
    pub red_dot: i32,
}

pub struct PresetFleetCodec;

impl PresetFleetCodec {
    pub fn encode(value: &PresetFleetInfo) -> Vec<u8> {
        let mut output = Vec::new();
        for fleet in &value.fleets {
            let mut body = Vec::new();
            write_string(&mut body, 1, &fleet.name);
            for hero_id in &fleet.hero_ids {
                write_varint_field(&mut body, 2, *hero_id as u32 as u64);
            }
            write_int32(&mut body, 3, fleet.mode_id);
            write_int32(&mut body, 4, fleet.strategy_id);
            for hero_id in &fleet.ex_hero_ids {
                write_varint_field(&mut body, 5, *hero_id as u32 as u64);
            }
            write_bytes(&mut output, 1, &body);
        }
        write_varint_field(&mut output, 2, value.name_num.max(0) as u64);
        write_varint_field(&mut output, 3, value.red_dot.max(0) as u64);
        output
    }

    pub fn decode(payload: &[u8]) -> Result<PresetFleetInfo, ProtocolError> {
        let mut reader = PbReader::new(payload);
        let mut value = PresetFleetInfo::default();
        while let Some((field, wire)) = reader.next_field()? {
            match (field, wire) {
                (1, 2) => value.fleets.push(Self::decode_fleet(reader.read_bytes()?)?),
                (2, 0) => value.name_num = reader.read_varint()? as i32,
                (3, 0) => value.red_dot = reader.read_varint()? as i32,
                (_, wire) => reader.skip(wire)?,
            }
        }
        Ok(value)
    }

    fn decode_fleet(payload: &[u8]) -> Result<PresetFleet, ProtocolError> {
        let mut reader = PbReader::new(payload);
        let mut value = PresetFleet::default();
        while let Some((field, wire)) = reader.next_field()? {
            match (field, wire) {
                (1, 2) => value.name = reader.read_string()?,
                (2, 0) => value.hero_ids.push(reader.read_varint()? as i32),
                (3, 0) => value.mode_id = reader.read_varint()? as i32,
                (4, 0) => value.strategy_id = reader.read_varint()? as i32,
                (5, 0) => value.ex_hero_ids.push(reader.read_varint()? as i32),
                (_, wire) => reader.skip(wire)?,
            }
        }
        Ok(value)
    }
}

fn decode_sample_info(payload: &[u8]) -> Result<TSampleInfo, ProtocolError> {
    let mut reader = PbReader::new(payload);
    let mut value = TSampleInfo::default();
    while let Some((field, wire)) = reader.next_field()? {
        match (field, wire) {
            (1, 2) => value.uuid = reader.read_string()?,
            (2, 2) => value.model = reader.read_string()?,
            (3, 2) => value.release = reader.read_string()?,
            (4, 2) => value.network = reader.read_string()?,
            (5, 2) => value.platform = reader.read_string()?,
            (6, 2) => value.pkg_name = reader.read_string()?,
            (_, wire) => reader.skip(wire)?,
        }
    }
    Ok(value)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientGameMessage {
    pub channel: u8,
    pub operation: u8,
    pub session_id: i64,
    pub state: u8,
    pub payload: Vec<u8>,
}

pub struct ClientGameWireCodec;

impl ClientGameWireCodec {
    pub fn encode_client_request(
        operation: u8,
        payload: &[u8],
        session_id: i64,
        state: u8,
    ) -> Vec<u8> {
        let mut packet = Vec::with_capacity(11 + payload.len());
        packet.extend_from_slice(&[0, operation]);
        packet.extend_from_slice(&session_id.to_le_bytes());
        packet.push(state);
        packet.extend_from_slice(payload);
        packet
    }

    pub fn decode_client_request(packet: &[u8]) -> Result<ClientGameMessage, ProtocolError> {
        if packet.len() < 11 {
            return Err(ProtocolError::Truncated("client game packet"));
        }
        Ok(ClientGameMessage {
            channel: packet[0],
            operation: packet[1],
            session_id: i64::from_le_bytes(
                packet[2..10]
                    .try_into()
                    .map_err(|_| ProtocolError::Invalid("invalid client session id"))?,
            ),
            state: packet[10],
            payload: packet[11..].to_vec(),
        })
    }

    pub fn encode_server_response(operation: u8, payload: &[u8]) -> Vec<u8> {
        let mut operation_payload = Vec::new();
        write_int32(&mut operation_payload, 1, 0);
        write_int32(&mut operation_payload, 2, 0);
        write_int32(&mut operation_payload, 3, i32::from(operation));
        write_bytes(&mut operation_payload, 4, payload);
        let mut envelope = Vec::new();
        write_int32(&mut envelope, 1, 0);
        write_bytes(&mut envelope, 2, &operation_payload);
        let mut packet = Vec::with_capacity(2 + envelope.len());
        packet.extend_from_slice(&[0, 5]);
        packet.extend_from_slice(&envelope);
        packet
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameLoginFrame {
    pub operation: i32,
    pub payload: Vec<u8>,
}

pub struct GameLoginFrameCodec;

impl GameLoginFrameCodec {
    pub fn encode(frame: &GameLoginFrame) -> Result<Vec<u8>, ProtocolError> {
        if frame.payload.len() > MAX_FRAME_SIZE as usize {
            return Err(ProtocolError::InvalidFrameLength(frame.payload.len() as i64));
        }
        let length = i32::try_from(frame.payload.len() + 4)
            .map_err(|_| ProtocolError::InvalidFrameLength(frame.payload.len() as i64))?;
        let mut packet = Vec::with_capacity(frame.payload.len() + 8);
        packet.extend_from_slice(&length.to_be_bytes());
        packet.extend_from_slice(&frame.operation.to_be_bytes());
        packet.extend_from_slice(&frame.payload);
        Ok(packet)
    }

    pub fn decode(packet: &[u8]) -> Result<GameLoginFrame, ProtocolError> {
        if packet.len() < 8 {
            return Err(ProtocolError::Truncated("game login frame"));
        }
        let length = i32::from_be_bytes([packet[0], packet[1], packet[2], packet[3]]);
        if !(4..=MAX_FRAME_SIZE).contains(&length) || packet.len() < length as usize + 4 {
            return Err(ProtocolError::InvalidFrameLength(length as i64));
        }
        let operation = i32::from_be_bytes([packet[4], packet[5], packet[6], packet[7]]);
        Ok(GameLoginFrame {
            operation,
            payload: packet[8..4 + length as usize].to_vec(),
        })
    }

    pub async fn write<W>(writer: &mut W, frame: &GameLoginFrame) -> Result<(), ProtocolError>
    where
        W: AsyncWrite + Unpin,
    {
        let packet = Self::encode(frame)?;
        writer.write_all(&packet).await?;
        writer.flush().await?;
        Ok(())
    }

    pub async fn read<R>(reader: &mut R) -> Result<Option<GameLoginFrame>, ProtocolError>
    where
        R: AsyncRead + Unpin,
    {
        let mut header = [0_u8; 8];
        match reader.read(&mut header[..1]).await? {
            0 => return Ok(None),
            1 => {}
            _ => unreachable!("single-byte read cannot return more than one byte"),
        }
        reader
            .read_exact(&mut header[1..])
            .await
            .map_err(|error| match error.kind() {
                io::ErrorKind::UnexpectedEof => ProtocolError::Truncated("game login frame header"),
                _ => ProtocolError::Io(error),
            })?;
        let length = i32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        if !(4..=MAX_FRAME_SIZE).contains(&length) {
            return Err(ProtocolError::InvalidFrameLength(length as i64));
        }
        let mut payload = vec![0_u8; length as usize - 4];
        reader
            .read_exact(&mut payload)
            .await
            .map_err(|error| match error.kind() {
                io::ErrorKind::UnexpectedEof => {
                    ProtocolError::Truncated("game login frame payload")
                }
                _ => ProtocolError::Io(error),
            })?;
        Ok(Some(GameLoginFrame {
            operation: i32::from_be_bytes([header[4], header[5], header[6], header[7]]),
            payload,
        }))
    }
}

struct PbReader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> PbReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn next_field(&mut self) -> Result<Option<(u32, u8)>, ProtocolError> {
        if self.offset == self.data.len() {
            return Ok(None);
        }
        let key = self.read_varint()?;
        let field = (key >> 3) as u32;
        let wire = (key & 7) as u8;
        if field == 0 {
            return Err(ProtocolError::Invalid("field number is zero"));
        }
        Ok(Some((field, wire)))
    }

    fn read_varint(&mut self) -> Result<u64, ProtocolError> {
        let mut value = 0_u64;
        for shift in (0..64).step_by(7) {
            let byte = *self
                .data
                .get(self.offset)
                .ok_or(ProtocolError::Truncated("varint"))?;
            self.offset += 1;
            value |= u64::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(ProtocolError::VarintTooLong)
    }

    fn read_bytes(&mut self) -> Result<&'a [u8], ProtocolError> {
        let length = usize::try_from(self.read_varint()?)
            .map_err(|_| ProtocolError::Invalid("length does not fit usize"))?;
        let end = self
            .offset
            .checked_add(length)
            .ok_or(ProtocolError::Invalid("length overflows"))?;
        let value = self
            .data
            .get(self.offset..end)
            .ok_or(ProtocolError::Truncated("length-delimited field"))?;
        self.offset = end;
        Ok(value)
    }

    fn read_string(&mut self) -> Result<String, ProtocolError> {
        let bytes = self.read_bytes()?;
        String::from_utf8(bytes.to_vec()).map_err(|_| ProtocolError::Invalid("string is not UTF-8"))
    }

    fn skip(&mut self, wire: u8) -> Result<(), ProtocolError> {
        match wire {
            0 => self.read_varint().map(|_| ()),
            1 => self.skip_bytes(8),
            2 => self.read_bytes().map(|_| ()),
            5 => self.skip_bytes(4),
            _ => Err(ProtocolError::Invalid("unsupported wire type")),
        }
    }

    fn skip_bytes(&mut self, length: usize) -> Result<(), ProtocolError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(ProtocolError::Invalid("field length overflows"))?;
        if end > self.data.len() {
            return Err(ProtocolError::Truncated("fixed-width field"));
        }
        self.offset = end;
        Ok(())
    }
}

fn write_varint_field(output: &mut Vec<u8>, field: u32, value: u64) {
    write_varint(output, u64::from(field) << 3);
    write_varint(output, value);
}

fn write_bytes(output: &mut Vec<u8>, field: u32, value: &[u8]) {
    write_varint(output, (u64::from(field) << 3) | 2);
    write_varint(output, value.len() as u64);
    output.extend_from_slice(value);
}

fn write_fixed32_field(output: &mut Vec<u8>, field: u32, value: u32) {
    write_varint(output, (u64::from(field) << 3) | 5);
    output.extend_from_slice(&value.to_le_bytes());
}

fn write_string(output: &mut Vec<u8>, field: u32, value: &str) {
    if !value.is_empty() {
        write_bytes(output, field, value.as_bytes());
    }
}

fn write_string_always(output: &mut Vec<u8>, field: u32, value: &str) {
    write_bytes(output, field, value.as_bytes());
}

fn write_int32(output: &mut Vec<u8>, field: u32, value: i32) {
    if value != 0 {
        write_varint_field(output, field, value as u32 as u64);
    }
}

fn write_varint(output: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        output.push((value as u8 & 0x7F) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

#[cfg(test)]
mod request_decode_tests {
    use super::{CopyMiniGamePassRequest, CopyStarRewardRequest, Decode, SupportCompleteRequest};

    fn varint_field(field: u32, value: u64) -> Vec<u8> {
        let mut payload = Vec::new();
        super::write_varint_field(&mut payload, field, value);
        payload
    }

    #[test]
    fn copy_star_reward_preserves_repeated_indexes() {
        let mut payload = varint_field(1, 30001);
        payload.extend(varint_field(3, 1));
        payload.extend(varint_field(3, 2));
        let request = CopyStarRewardRequest::decode(&payload).expect("valid star reward request");
        assert_eq!(request.chapter_id, 30001);
        assert_eq!(request.indexes, vec![1, 2]);
    }

    #[test]
    fn mini_game_pass_requires_success_marker() {
        let payload = varint_field(1, 1001);
        assert!(CopyMiniGamePassRequest::decode(&payload).is_err());
    }

    #[test]
    fn support_completion_rejects_zero_identifier() {
        let mut payload = varint_field(1, 0);
        payload.extend(varint_field(2, 1));
        assert!(SupportCompleteRequest::decode(&payload).is_err());
    }
}
