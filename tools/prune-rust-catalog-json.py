#!/usr/bin/env python3
"""Audit and conservatively prune unused top-level fields from JSON catalogs.

Only tables with typed server readers are allow-listed. Tables retained as raw
JSON for gameplay endpoints, battle/ship compatibility, or forward-compatible
features are deliberately left untouched.
"""

from __future__ import annotations

import argparse
import json
import tempfile
from collections import Counter
from pathlib import Path


# Raw values are consumed by handlers or retained for future/forward-compatible
# endpoints. Removing fields from these tables can change response payloads or
# break dynamic gameplay rules.
PRESERVE_ALL = {
    "activity",
    "activity_extract",
    "activity_extract_ur",
    "anniversary_video",
    "battlepass_level",
    "battlepass_level_activity",
    "battlepass_param",
    "battlepass_param_activity",
    "battlepass_task",
    "battlepass_task_activity",
    "buildinginfo",
    "drop_item",
    "food_compose",
    "guildboxscore",
    "guildoffer_info",
    "guildoffer_perscorereward",
    "guildoffer_scorereward",
    "guildwar_base_info",
    "guildwar_rank",
    "guildwar_reward",
    "interaction_figurte",
    "interaction_item",
    "interaction_item_bag",
    "interaction_paper_cut_fomula",
    "item_exchange",
    "item_valentine_gift",
    "magazine_info",
    "magazine_page",
    "outpost_info",
    "outpost_level",
    "parameter",
    "recipe",
    "ship_advance",
    "ship_break",
    "ship_info",
    "ship_remould_effect",
    "ship_remould_template",
    "sportsmeet_award",
    "task_guild",
    "task_magazine",
    "testship_reward",
    "testship_task",
    "world_event",
    "world_event_task",
}


# Field names are the union of every key read by current Rust catalog loaders.
# Aliases are retained where client versions use both spellings.
FIELDS = {
    "affection_change": {
        "affection_change_sort",
        "affection_add",
        "affection_flagship_add",
        "affection_mvp_add",
        "affection_reduce",
        "mood_reduce",
        "mood_shipwrecks_reduce",
    },
    "affection_item": {"affection_exp", "affectionExp", "exp"},
    "achievement": {
        "goal",
        "playerLevelMin",
        "player_level_min",
        "playerLevelMax",
        "player_level_max",
        "abandoned",
        "nextTaskId",
        "next_task_id",
        "medalId",
        "medal_id",
        "point",
        "rewards",
        "reward_id",
        "reward",
        "rewardsList",
        "lastAchievement",
        "last_achievement",
    },
    "build_ship": {"res1", "res2", "res3", "ship_list"},
    "building_character_story": {"ship_fleet_id"},
    "chapter": {
        "level_list",
        "star_cond",
        "star_reward",
        "class_type",
        "chapter_plot_type",
        "memory_id",
        "treaty_copy",
        "dailygroup_id",
        "chapter_type",
    },
    "chapter_type": {
        "start_chapter",
        "affection_change_sort",
        "affection_add",
        "affection_flagship_add",
        "affection_mvp_add",
        "affection_reduce",
        "mood_reduce",
        "mood_shipwrecks_reduce",
        "not_effect_task",
    },
    "combination_ship": {
        "level",
        "sf_id",
        "next_id",
        "star",
        "levelup_item",
        "break_item",
    },
    "copy": {"copy_id", "fleet_id", "copy_type", "blood_range_lower", "random_weight"},
    "copy_display": {
        "search_3d",
        "search3d",
        "random_factor_sets",
        "drop_info_id",
        "dropInfoId",
        "supply_basic_cost",
        "supple_cost_argu",
        "first_reward",
        "firstReward",
        "copy_must_drop_reward",
        "copy_must_drop_num",
        "rank_drop",
    },
    "copy_rank_drop": {"drop_group"},
    "drop_info": {
        "item_info",
        "type",
        "drop_rate",
        "show_num",
    },
    "equip": {
        "quality",
        "equipTypeId",
        "equip_type_id",
        "equip_prop",
        "equipProp",
        "enhance_prop",
        "enhanceProp",
        "enhanceLevelMax",
        "enhance_level_max",
        "starMax",
        "star_max",
        "dismantlingGet",
        "dismantling_get",
        "activity_equip",
        "activityEquip",
        "reward",
        "reward_id",
        "noResolve",
        "no_resolve",
        "renovateSkill",
        "renovate_skill",
    },
    "equip_enhance_item": {"exp", "enhance_level_limit", "enhanceLevelLimit"},
    "equip_enhance_level_exp": {"exp"},
    "equip_enhance_level_ur": {"enchance_level", "item_cost"},
    "equip_enhance_renovate": {"item_array", "equip_self_count", "need_enhance_level"},
    "equip_levelbreak_item": {"level_rank", "levelRank", "item_cost"},
    "extract_ship": {
        "extract_type",
        "drop_item_id",
        "expend",
        "new_ten_expend",
        "twenty_drop",
        "hundred_reward",
    },
    "fashion": {"belongToShip", "belong_to_ship"},
    "fleet": {
        "copy_attacheds",
        "copy_enemys",
        "is_last_fleet",
        "ship_exp",
        "player_exp",
        "drop_id",
        "other_drop_id",
        "other_drop_ids",
        "settle_drop_id",
        "settle_drop_ids",
    },
    "handbook_behaviour_index": set(),
    "item_info": {"drop_id"},
    "item_selected": {"item_id", "drop_id"},
    "main_line_reward_arg": {"id", "exp_ratio", "settle_drop_ratio", "other_drop_ratio"},
    "minigame_copy": set(),
    "player_levelup": {"level", "exp"},
    "pskill_dict_group": {"maxLevel", "max_level", "upgrade_materials", "skill_id_array"},
    "random_factor_group": {"factor"},
    "random_factor_set": {"factor_groups"},
    "recharge": {"reward"},
    "rewards": {"reward", "rewards"},
    "ship_fleet": {"combination_open"},
    "ship_exp_item": {"exp"},
    "ship_handbook": {"show_tag", "show_state"},
    "ship_main": {
        "fixed_money",
        "hp",
        "hp_levelup",
        "attack",
        "attack_levelup",
        "defense",
        "defense_levelup",
        "torpedo_attack",
        "torpedo_attack_levelup",
        "torpedo_defense",
        "torpedo_defense_levelup",
        "to_air_attack",
        "to_air_attack_levelup",
        "to_torpedo_attack",
        "to_torpedo_attack_levelup",
        "ship_bomb_attack",
        "ship_bomb_attack_levelup",
        "ship_torpedo_attack",
        "ship_torpedo_attack_levelup",
        "ship_air_control",
        "ship_air_control_levelup",
        "carry_plane_count",
        "hit",
        "dodge",
        "crit",
        "anti_crit",
        "pskill_show_id",
        "direct_activate_talent_id",
        "condition_activate_talent_id",
        "break_down_get",
        "supple_cost",
    },
    "ship_max_power": {"max_power_prop"},
    "ship_need_power_exp": {"enhance_type", "need_power_exp"},
    "ship_provide_power_exp": {"provide_power_exp"},
    "ship_levelup": {"exp"},
    "shop": {"shelf_list"},
    "shop_goods": {"currency", "price"},
    "ship_enemy": {
        "hp",
        "ship_info_id",
        "attack",
        "defense",
        "hit",
        "dodge",
        "crit",
        "anti_crit",
        "torpedo",
        "torpedo_defense",
    },
    "support_fleet_item": {
        "time",
        "duration",
        "base_award",
        "big_sucess_base_award",
        "extra_drop_id",
        "extraDropId",
        "big_sucess_extra_drop_id",
        "bigSuccessExtraDropId",
        "big_sucess_ratio",
        "bigSuccessRatio",
        "consumption",
        "complete_item",
    },
    "talent": {"belongtalent", "nexttalent", "precondition", "levelup"},
    "talentmain": {"talentlist"},
    "teaching_achievement": {"rewards"},
    "task_activity": {
        "goal",
        "playerLevelMin",
        "player_level_min",
        "playerLevelMax",
        "player_level_max",
        "abandoned",
        "nextTaskId",
        "next_task_id",
        "medalId",
        "medal_id",
        "point",
        "rewards",
        "reward_id",
        "reward",
        "rewardsList",
    },
    "task_daily": {
        "goal",
        "playerLevelMin",
        "player_level_min",
        "playerLevelMax",
        "player_level_max",
        "abandoned",
        "nextTaskId",
        "next_task_id",
        "medalId",
        "medal_id",
        "point",
        "rewards",
        "reward_id",
        "reward",
        "rewardsList",
    },
    "task_grow": {
        "goal",
        "playerLevelMin",
        "player_level_min",
        "playerLevelMax",
        "player_level_max",
        "abandoned",
        "nextTaskId",
        "next_task_id",
        "medalId",
        "medal_id",
        "point",
        "rewards",
        "reward_id",
        "reward",
        "rewardsList",
    },
    "task_main": {
        "goal",
        "playerLevelMin",
        "player_level_min",
        "playerLevelMax",
        "player_level_max",
        "abandoned",
        "nextTaskId",
        "next_task_id",
        "medalId",
        "medal_id",
        "point",
        "rewards",
        "reward_id",
        "reward",
        "rewardsList",
    },
    "task_return": {
        "goal",
        "playerLevelMin",
        "player_level_min",
        "playerLevelMax",
        "player_level_max",
        "abandoned",
        "nextTaskId",
        "next_task_id",
        "medalId",
        "medal_id",
        "point",
        "rewards",
        "reward_id",
        "reward",
        "rewardsList",
    },
    "task_teaching": {"goal", "rewards"},
    "task_teaching_group": {"task_daily_id", "task_assess_id"},
    "task_treaty": {
        "goal",
        "playerLevelMin",
        "player_level_min",
        "playerLevelMax",
        "player_level_max",
        "abandoned",
        "nextTaskId",
        "next_task_id",
        "medalId",
        "medal_id",
        "point",
        "rewards",
        "reward_id",
        "reward",
        "rewardsList",
    },
    "task_weekly": {
        "goal",
        "playerLevelMin",
        "player_level_min",
        "playerLevelMax",
        "player_level_max",
        "abandoned",
        "nextTaskId",
        "next_task_id",
        "medalId",
        "medal_id",
        "point",
        "rewards",
        "reward_id",
        "reward",
        "rewardsList",
    },
}

# Never prune a dynamic table unless it is either in raw set or explicitly in
# allow-list. This turns future catalog additions into audit warnings.
KNOWN_TABLES = PRESERVE_ALL | set(FIELDS)


def atomic_write(path: Path, document: dict[str, object]) -> None:
    with tempfile.NamedTemporaryFile(
        "w", encoding="utf-8", dir=path.parent, delete=False, suffix=".tmp"
    ) as handle:
        json.dump(document, handle, ensure_ascii=False, separators=(",", ":"))
        handle.write("\n")
        temporary = Path(handle.name)
    temporary.replace(path)


def process(path: Path, apply: bool) -> tuple[str, int, int, Counter[str], set[str]]:
    table = path.stem.removeprefix("config_")
    document = json.loads(path.read_text(encoding="utf-8"))
    rows = document.get("rows")
    if not isinstance(rows, list):
        raise RuntimeError(f"{path}: rows is not an array")
    if table in PRESERVE_ALL:
        return table, 0, 0, Counter(), set()
    allowed = FIELDS.get(table)
    if allowed is None:
        return table, 0, 0, Counter(), {table}
    removed = Counter()
    changed_rows = 0
    old_size = path.stat().st_size
    for row in rows:
        if not isinstance(row, dict) or not isinstance(row.get("value"), dict):
            continue
        value = row["value"]
        extra = set(value) - allowed
        if not extra:
            continue
        changed_rows += 1
        for key in extra:
            removed[key] += 1
        row["value"] = {key: value[key] for key in value if key in allowed}
    if apply and changed_rows:
        atomic_write(path, document)
    new_size = path.stat().st_size if apply else len(
        json.dumps(document, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    ) + 1
    return table, changed_rows, old_size - new_size, removed, set()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directories", nargs="+", type=Path)
    parser.add_argument("--apply", action="store_true", help="write pruned JSON files")
    args = parser.parse_args()

    total_rows = 0
    total_saved = 0
    total_removed = Counter()
    unknown = set()
    changed_files = 0
    for directory in args.directories:
        directory = directory.resolve()
        if not directory.is_dir():
            raise RuntimeError(f"catalog directory does not exist: {directory}")
        directory_rows = 0
        directory_saved = 0
        for path in sorted(directory.glob("config_*.json")):
            table, changed, saved, removed, unseen = process(path, args.apply)
            document = json.loads(path.read_text(encoding="utf-8"))
            directory_rows += len(document.get("rows", []))
            directory_saved += saved
            total_removed.update(removed)
            unknown.update(unseen)
            if changed:
                changed_files += 1
                print(f"{path.name}: rows={changed}, removed={sum(removed.values())}, saved={saved} bytes")
        total_rows += directory_rows
        total_saved += directory_saved
        print(f"{directory}: rows={directory_rows}, saved={directory_saved} bytes")
    if unknown:
        print("UNMAPPED TABLES (preserved): " + ", ".join(sorted(unknown)))
    print(
        f"{'APPLIED' if args.apply else 'CHECK'}: files={changed_files}, "
        f"rows={total_rows}, saved={total_saved} bytes, "
        f"removed_fields={sum(total_removed.values())}"
    )
    if total_removed:
        print("removed by field: " + ", ".join(f"{key}={count}" for key, count in total_removed.most_common()))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, RuntimeError) as error:
        raise SystemExit(f"error: {error}")
