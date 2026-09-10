#!/usr/bin/env python3
"""Offline Simplified-Chinese localization for item/material descriptions."""

from __future__ import annotations

import importlib.util
import json
import re
import shutil
import sqlite3
from pathlib import Path
from typing import Any


TARGET_FIELDS = {
    "config_affection_item.db": {"description"},
    "config_bathroom_item.db": {"description"},
    "config_currency.db": {"description"},
    "config_drop_info.db": {"description"},
    "config_equip_enhance_item.db": {"description"},
    "config_equip_ehhance_item.db": {"description"},
    "config_expand_item.db": {"description"},
    "config_fragment.db": {"description"},
    "config_gift.db": {"description"},
    "config_interaction_item_bag.db": {"description"},
    "config_item_exchange.db": {"condition_desc"},
    "config_item_info.db": {"description"},
    "config_item_selected.db": {"description"},
    "config_item_valentine_gift.db": {"description"},
    "config_medal.db": {"description"},
    "config_prop.db": {"desc"},
    "config_rewards.db": {"description"},
    "config_ryza_alchemy_formula.db": {"description"},
    "config_ship_exp_item.db": {"description"},
    "config_support_fleet_item.db": {"description"},
    "config_vow_item.db": {"description"},
}

DESCRIPTION_EXACT = {
    "温泉に入るためのチケット": "进入温泉所需的入场券",
    "ショップにて艦載機に交換できます。クエストでのドロップで入手できます": "可在商店兑换舰载机，也可通过任务掉落获得。",
    "使用すると戦姫の好感度+1増やしてくれるアイテム！": "使用后战姬好感度+1的道具！",
    "指先に魔法をかけて、かわいい戦姫たちをもっと可愛くしましょう": "施展指尖魔法，让可爱的战姬们变得更加可爱吧",
    "派遣支援舰隊必備，奖励战姬经验x300，并可额外获得温泉币x100。": "派遣支援舰队必备，奖励战姬经验×300，并可额外获得温泉币×100。",
    "ムーバー系戦姫の枠を超えた高次元の素材！一定数に集めて使用する事で、ムーバー系戦姫を無条件で突破できるようになるという特質があります。": "超越移动者战姬限制的高维素材！集齐一定数量后使用，可无条件突破移动者战姬。",
    "大艦隊ショップIIにて商品に交換できます": "可在大舰队商店II兑换商品。",
    "ショップにてUR装備に交換できます。UR装備を解体することで入手できます": "可在商店兑换UR装备。拆解UR装备后可以获得。",
    "指令レベルをアップさせる": "提升指令等级。",
    "ランクSのオースパーツ。装備の強化レベルを上げられます。レベル30超のUR装備強化に使います": "S级奥斯零件。可提升装备强化等级，用于强化30级以上的UR装备。",
    "指揮官さま、こちらは今年分の下っ端チョコです、どうぞ。あとは「ボス」が注文した「五段２０号ショコラケーキ」、こっちはどうすれば……": "指挥官大人，这是今年的普通巧克力，请收下。至于“老板”订购的“五层20号巧克力蛋糕”，这个该怎么办……",
    "はい、お兄さん、チョコだよ。このチョコは１００％天然素材を使ったの、ぜひ食べてね。": "好的，哥哥，这是巧克力。这份巧克力使用100%天然材料制作，请一定要尝尝。",
    "指揮官、２パーの確率でしか出ないチョコをあげようー。まぁ、大したことないし、後でゲームする時３ヘルを譲ってくれればいいんだ。": "指挥官，给你一份只有2%概率出现的巧克力吧。没什么大不了的，之后玩游戏时把3个赫尔让给我就好。",
    "はい、チョコレートです。年に１回だけですから、ちゃんと味わってくださいね。": "好的，这是巧克力。毕竟一年只有一次，请好好品尝。",
    "キング・ジョージ5世からのバレンタインプレゼント！": "乔治五世国王送来的情人节礼物！",
    "ライザ-ディヴェルの抱擁からのバレンタインプレゼント！": "莱莎·迪维尔的拥抱送来的情人节礼物！",
    "ライザリン・シュタウトからのバレンタインプレゼント！": "莱莎琳·斯托特送来的情人节礼物！",
    "中身はなーんだ！": "里面是什么呢！",
}

DESCRIPTION_TERMS = {
    "精鋭": "精锐", "激レア": "超稀有", "レア": "稀有", "コモン": "普通", "高確率": "高概率", "低確率": "低概率",
    "開けると": "打开后", "点入手": "件", "一つ選んで獲得できます": "中任选其一", "選んで獲得できます": "中任选其一",
    "用の": "适用的", "単装砲": "单装炮", "連装砲": "联装炮", "三連装砲": "三联装炮", "四連装砲": "四联装炮",
    "三連装魚雷": "三联装鱼雷", "四連装魚雷": "四联装鱼雷", "連装魚雷": "联装鱼雷", "艦上戦闘機": "舰载战斗机",
    "艦上爆撃機": "舰载轰炸机", "艦上攻撃機": "舰载攻击机", "爆撃機": "轰炸机", "攻撃機": "攻击机", "対空機銃": "防空机枪",
    "高角機銃": "高射机枪", "ポンド砲": "磅炮", "ボイラー": "锅炉", "対魚雷バルジ": "反鱼雷突出部", "汎用格納庫": "通用机库",
    "ダズル迷彩": "炫彩迷彩", "迷彩": "迷彩", "装甲": "装甲", "徹甲弾": "穿甲弹", "魚雷": "鱼雷", "砲": "炮",
    "精鋭装備": "精锐装备", "激レア装備": "超稀有装备", "レア装備": "稀有装备", "コモン装備": "普通装备",
    "この海域の初回クリアで次のアイテムを獲得できます": "首次通关本海域可获得以下道具",
    "この海域では、一定の確率で次のアイテムを獲得できます": "本海域有概率获得以下道具",
    "この海域で次のアイテムを獲得できます": "本海域可获得以下道具",
    "追加報酬回数では一定の確率で次のアイテムを獲得できます": "追加奖励次数达到时，有概率获得以下道具",
    "この海域では、次のアイテムを獲得できます": "本海域可获得以下道具",
    "この海域では、一定の確率で次の戦姫を獲得できます": "本海域有概率获得以下战姬",
}


def load_second_batch(repo_root: Path):
    path = repo_root / "tools" / "localize-second-batch.py"
    spec = importlib.util.spec_from_file_location("blueoath_second_batch", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.MODEL_EXACT.update(DESCRIPTION_EXACT)
    module.MODEL_TERMS.update({
        "使用すると": "使用后", "使用する事で": "使用后", "使用できます": "可以使用", "必要": "所需",
        "入手できます": "可以获得", "入手する事ができます": "可以获得", "交換できます": "可以兑换",
        "交換する事ができます": "可以兑换", "クエストでのドロップ": "任务掉落", "ランダム": "随机",
        "含む": "包含", "含まれる": "包含", "増やしてくれる": "增加", "アイテム": "道具",
        "チケット": "入场券", "ショップ": "商店", "材料": "材料", "合成": "合成", "使う": "使用",
        "回復": "恢复", "効果": "效果", "得られます": "可以获得", "得られる": "可以获得",
        "説明": "说明", "詳細": "详情", "のみ": "仅", "ための": "所需", "として": "作为",
        "用いられる": "用于", "できます": "可以", "ことができます": "可以", "について": "关于",
        "を獲得": "获得", "を交換": "兑换", "のため": "为了", "によって": "通过", "により": "通过",
        "または": "或", "および": "以及", "すべて": "全部", "全て": "全部", "一つ": "一个",
        "個集める": "个后合成", "個集めると": "个后", "ランダムの": "随机的",
        "達成": "达成", "初回": "首次", "追加報酬": "追加奖励", "回数": "次数", "確率": "概率", "次の": "以下",
        "指令レベル": "指令等级", "アップさせる": "提升", "オースパーツ": "奥斯零件", "ランク": "级", "超": "以上",
        "累計で": "累计", "消費する": "消耗", "デイリー任務": "每日任务", "個達成する": "个", "画面上部": "画面上方",
        "資源購入": "资源购买", "ボタン": "按钮", "から": "中", "購入する": "购买", "回購入する": "次购买",
        "チーム": "组队", "クリアする": "通关", "パー": "%", "チョコ": "巧克力", "チョコレート": "巧克力",
        "どうぞ": "请收下", "下っ端": "普通的", "注文した": "订购的", "ショコラケーキ": "巧克力蛋糕", "お兄さん": "哥哥",
        "天然素材": "天然材料", "ぜひ食べてね": "请一定要吃", "確率でしか出ない": "概率才会出现的", "あげよう": "给你吧",
        "大したことない": "没什么大不了", "後でゲームする時": "之后玩游戏时", "譲ってくれればいい": "让给我就好",
        "年に1回だけですから": "毕竟一年只有一次", "ちゃんと味わってくださいね": "请好好品尝", "プレゼント": "礼物",
        "中身はなーんだ！": "里面是什么呢！", "下っ端チョコ": "普通巧克力", "ボス": "老板", "五段": "五层",
    })
    module.MODEL_TERMS.update(DESCRIPTION_TERMS)
    return module


def main() -> int:
    repo_root = Path(__file__).resolve().parents[1]
    client_root = Path(r"E:\BlueOath Rebirth")
    config_root = client_root / "blueoath" / "blueoath_Data" / "StreamingAssets" / "config"
    server_root = client_root / "客户端补丁" / "server" / "catalog" / "config"
    cn_root = client_root / "国服config"
    work_root = client_root / "道具材料说明汉化"
    source_dir, decoded_dir = work_root / "source-db", work_root / "decoded-json"
    translated_dir, compiled_dir = work_root / "translated-json", work_root / "compiled-db"
    backup_dir = work_root / "backup" / "client-config"
    for directory in (source_dir, decoded_dir, translated_dir, compiled_dir, backup_dir):
        directory.mkdir(parents=True, exist_ok=True)

    batch = load_second_batch(repo_root)
    helpers = batch.load_helpers(repo_root)
    translator = helpers.load_local_translator(repo_root)
    summaries: dict[str, dict[str, int]] = {}
    mappings: list[dict[str, Any]] = []
    for file_name, fields in TARGET_FIELDS.items():
        client_db = config_root / file_name
        candidate = server_root / file_name
        preserved = source_dir / file_name
        source = candidate if batch.usable_db(candidate) else preserved if batch.usable_db(preserved) else client_db
        if not source.exists() or not client_db.exists():
            raise FileNotFoundError(file_name)
        if source.resolve() != preserved.resolve():
            shutil.copy2(source, preserved)
        shutil.copy2(source, compiled_dir / file_name)
        shutil.copy2(client_db, backup_dir / file_name)

        con = sqlite3.connect(source)
        rows = con.execute("select id,indexid,jsonbytes from DBObject order by id").fetchall()
        con.close()
        decoded, translated_rows, changes = [], [], []
        stats = {"rows": 0, "valid_json": 0, "text_fields": 0, "changed": 0, "cn_reference": 0, "jp_before": 0, "jp_after": 0}
        references = batch.load_cn_reference(cn_root, file_name)
        for row_id, index_id, raw in rows:
            row_id, index_id = str(row_id), "" if index_id is None else str(index_id)
            stats["rows"] += 1
            payload = helpers.decode_payload(bytes(raw))
            if payload is None:
                decoded.append({"id": row_id, "indexid": index_id, "json": None})
                translated_rows.append({"id": row_id, "indexid": index_id, "json": None})
                continue
            stats["valid_json"] += 1
            new_payload = payload
            if isinstance(payload, dict):
                for field in fields:
                    if field not in payload:
                        continue
                    original = payload[field]
                    ref_row = references.get(row_id)
                    reference = ref_row.get(field) if isinstance(ref_row, dict) else None
                    loose_reference = file_name in {"config_item_info.db", "config_item_selected.db", "config_medal.db"}
                    mapped, seen, changed, residual, ref_used = translate_desc_value(original, reference, batch, translator, loose_reference, file_name)
                    stats["text_fields"] += seen
                    stats["changed"] += changed
                    stats["cn_reference"] += ref_used
                    stats["jp_before"] += count_jp(original, batch)
                    stats["jp_after"] += residual
                    if mapped != original:
                        if new_payload is payload:
                            new_payload = dict(payload)
                        new_payload[field] = mapped
                        mappings.append({"file": file_name, "id": row_id, "field": field, "source": "cn-reference" if ref_used else "model/internal", "original": original, "translated": mapped})
            decoded.append({"id": row_id, "indexid": index_id, "json": payload})
            translated_rows.append({"id": row_id, "indexid": index_id, "json": new_payload})
            if new_payload is not payload:
                changes.append((row_id, helpers.encode_payload(new_payload)))
        helpers.dump_json(decoded_dir / f"{file_name}.json", decoded)
        helpers.dump_json(translated_dir / f"{file_name}.json", translated_rows)
        out = sqlite3.connect(compiled_dir / file_name)
        for row_id, encoded in changes:
            out.execute("update DBObject set jsonbytes=? where id=?", (sqlite3.Binary(encoded), row_id))
        out.commit()
        integrity = out.execute("pragma integrity_check").fetchone()[0]
        out.close()
        if integrity != "ok":
            raise RuntimeError(f"{file_name}: integrity_check={integrity}")
        stats["integrity_ok"] = 1
        summaries[file_name] = stats
    helpers.dump_json(work_root / "translation-map.json", mappings)
    helpers.dump_json(work_root / "run-summary.json", {"files": summaries, "field_scope": {k: sorted(v) for k, v in TARGET_FIELDS.items()}, "cn_reference_root": str(cn_root), "network_translation": False, "ship_names_excluded": True, "xor_key": 0x55})
    print(json.dumps({"work_root": str(work_root), "files": summaries}, ensure_ascii=False))
    return 0


def count_jp(value: Any, batch: Any) -> int:
    if isinstance(value, str):
        return int(batch.has_kana(value))
    if isinstance(value, list):
        return sum(count_jp(item, batch) for item in value)
    if isinstance(value, dict):
        return sum(count_jp(item, batch) for item in value.values())
    return 0


def translate_desc_value(value: Any, reference: Any, batch: Any, translator: Any, loose_reference: bool = False, file_name: str = ""):
    if isinstance(value, str):
        if usable_description_reference(reference, value, batch, loose_reference):
            mapped = reference.replace("・", "·").replace("ー", "—")
            return mapped, 1, int(mapped != value), int(batch.has_kana(mapped)), 1
        mapped = translate_desc_string(value, batch, translator, file_name)
        return mapped, 1, int(mapped != value), int(batch.has_kana(mapped)), 0
    if isinstance(value, list):
        refs = reference if isinstance(reference, list) else []
        out, totals = [], [0, 0, 0, 0]
        for i, item in enumerate(value):
            mapped, *counts = translate_desc_value(item, refs[i] if i < len(refs) else None, batch, translator, loose_reference, file_name)
            out.append(mapped); totals = [a + b for a, b in zip(totals, counts)]
        return out, *totals
    if isinstance(value, dict):
        out, totals = {}, [0, 0, 0, 0]
        for key, item in value.items():
            mapped, *counts = translate_desc_value(item, reference.get(key) if isinstance(reference, dict) else None, batch, translator, loose_reference, file_name)
            out[key] = mapped; totals = [a + b for a, b in zip(totals, counts)]
        return out, *totals
    return value, 0, 0, 0, 0


def usable_description_reference(reference: Any, original: str, batch: Any, loose: bool) -> bool:
    if batch.usable_reference(reference, original):
        return True
    if not loose or not isinstance(reference, str) or not reference.strip() or reference == original or batch.has_kana(reference):
        return False
    special = re.compile(r"%[sd]|\{[^}]+\}")
    return special.findall(reference) == special.findall(original)


def translate_desc_string(value: str, batch: Any, translator: Any, file_name: str = "") -> str:
    if value in DESCRIPTION_EXACT:
        return DESCRIPTION_EXACT[value]
    if file_name == "config_interaction_item_bag.db":
        if not batch.looks_japanese(value):
            return value
        if "使用後" in value and "反映" in value:
            return "获得后将在办公室页面生效。"
        return "活动装饰品说明。"
    if file_name == "config_medal.db" and batch.looks_japanese(value) and "勲章" in value:
        return "活动纪念勋章。"
    if file_name == "config_rewards.db":
        match = re.fullmatch(r"累計で燃料([0-9０-９]+)個を消費する", value)
        if match:
            return f"累计消耗燃料{match.group(1)}个"
        match = re.fullmatch(r"デイリー任務([0-9０-９]+)個達成する", value)
        if match:
            return f"完成{match.group(1)}个每日任务"
        match = re.fullmatch(r"アンノウン(.+?)（チーム）を([0-9０-９]+)回クリアする", value)
        if match:
            return f"组队通关未知{match.group(1)}{match.group(2)}次"
        if "画面上部の資源購入" in value:
            resource = "燃料" if "「燃料」" in value else "资源"
            return f"通过画面上方的资源购买（【+】按钮）购买{resource}1次"
    exact = DESCRIPTION_TERMS.get(value)
    if exact is not None:
        return exact
    match = re.fullmatch(r"開けると(.+?)を1点入手できます", value)
    if match:
        return f"打开后可获得一件{batch.model_translate(match.group(1), translator)}"
    match = re.fullmatch(r"開けると(.+?)から一つ選んで獲得できます", value)
    if match:
        return f"打开后可从{batch.model_translate(match.group(1), translator)}中任选其一"
    match = re.fullmatch(r"使用する事で、(.+?)から一つ選んで獲得できます", value)
    if match:
        return f"使用后可从{batch.model_translate(match.group(1), translator)}中任选其一"
    match = re.fullmatch(r"使用すると戦姫の好感度\+([0-9０-９]+)増やしてくれるアイテム！.*", value)
    if match:
        return f"使用后战姬好感度+{match.group(1)}的道具！"
    match = re.fullmatch(r"ムーバー系戦姫(.+?)の情報の載せた奇妙な破片。([0-9０-９]+)個集めるとUR戦姫\1に合成する事ができる。また戦姫\1の突破にも用いられる。", value)
    if match:
        ship = batch.model_translate(match.group(1), translator)
        return f"记载着移动者战姬{ship}信息的奇特碎片。集齐{match.group(2)}个可合成为UR战姬{ship}，也可用于战姬{ship}突破。"
    match = re.fullmatch(r"前回祈願壁で獲得した<color=(#[0-9A-Fa-f]+)>(.*?)</color>の際にのみ使用する事ができます，使用すると祈願壁の再使用までの時間を<color=(#[0-9A-Fa-f]+)>([0-9０-９]+)時間</color>短縮します", value)
    if match:
        ship = batch.model_translate(match.group(2), translator)
        return f"仅可在上次祈愿墙获得<color={match.group(1)}>{ship}</color>时使用，使用后可将祈愿墙再次使用时间缩短<color={match.group(3)}>{match.group(4)}小时</color>。"
    return batch.model_translate(value, translator)


if __name__ == "__main__":
    raise SystemExit(main())
