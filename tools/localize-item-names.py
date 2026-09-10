#!/usr/bin/env python3
"""Offline localization for equipment, material, and item names."""

from __future__ import annotations

import importlib.util
import json
import re
import shutil
import sqlite3
from pathlib import Path
from typing import Any


TARGET_FILES = [
    "config_affection_item.db",
    "config_bathroom_item.db",
    "config_currency.db",
    "config_equip.db",
    "config_equip_enhance_item.db",
    "config_expand_item.db",
    "config_fragment.db",
    "config_gift.db",
    "config_interaction_item_bag.db",
    "config_interaction_item_bag_group.db",
    "config_item_info.db",
    "config_item_selected.db",
    "config_item_valentine_gift.db",
    "config_medal.db",
    "config_ryza_alchemy_formula.db",
    "config_ship_exp_item.db",
    "config_shop_goods.db",
    "config_support_fleet_item.db",
    "config_vow_item.db",
]
FIELD = "name"
JP_KANA_RE = re.compile(r"[ぁ-ゟァ-ヿ]")
TEXT_TOKEN_RE = re.compile(r"[0-9０-９]+|%[sd]|\{[^}]+\}")
JP_TO_CN = {
    "敵": "敌", "黒": "黑", "駆": "驱", "軽": "轻", "戦": "战", "艦": "舰",
    "砲": "炮", "魚": "鱼", "擁": "拥", "抱擁": "拥抱", "戦鬼": "战鬼",
    "確": "确", "記": "记", "飛": "飞", "変": "变", "夢": "梦",
    "駆逐艦": "驱逐舰", "軽巡洋艦": "轻巡洋舰", "戦艦": "战舰",
    "弾": "弹", "薬": "药", "撃": "击", "機": "机", "徹": "彻", "専": "专", "許": "许",
    "結": "结", "鋭": "锐", "銀": "银", "勲": "勋", "気": "气", "揮": "挥",
    "姫": "姬", "鶴": "鹤", "鴎": "鸥", "風": "风", "証": "证", "憶": "忆", "揚": "扬",
    "勝": "胜", "負": "负", "隊": "队", "濃": "浓",
    "日誌": "日志", "実験": "实验", "夜間": "夜间", "爆撃機": "轰炸机", "戦闘機": "战斗机",
    "攻撃機": "攻击机", "雷撃機": "雷击机", "徹甲弾": "穿甲弹", "砲弾": "炮弹",
}
LOCAL_TERMS = {
    "激レア": "超稀有", "精鋭": "精英", "レア": "稀有", "オース": "奥斯", "戦艦": "战舰",
    "装備": "装备", "補給": "补给", "戦姫": "战姬", "ムーバー": "移动者", "アンブラ": "安布拉",
    "スタート": "新手", "パック": "礼包", "デイリー": "每日", "イベント": "活动", "お得": "特惠",
    "アルミ": "铝", "テスト": "测试", "メダル": "勋章", "メカニカル": "机械", "戦術": "战术", "ポイント": "点数",
    "交換": "兑换", "艦載機": "舰载机", "設備": "设备", "温泉": "温泉", "コイン": "硬币",
    "上級": "高级", "豪華": "豪华", "着せ替え": "换装", "御礼券": "答谢券", "訓練": "训练",
    "経験値": "经验值", "誓い": "誓约", "クローバー": "四叶草", "貝合せ": "贝合",
    "アイス": "冰淇淋", "チケット": "兑换券", "拡張": "扩展", "破片": "碎片", "マニキュア": "指甲油",
    "ヘットホン": "耳机", "アロマ": "香薰", "聖誕": "圣诞", "ポスター": "海报", "セット": "套装",
    "記念": "纪念", "限定版": "限定版", "限定": "限定", "バレンタインデー": "情人节",
    "プレゼント": "礼物", "祈願石": "祈愿石", "水銀": "水银", "結晶": "结晶", "専許": "专属", "攻撃": "攻击",
    "爆撃機": "轰炸机", "戦闘機": "战斗机", "雷撃機": "雷击机", "艦上": "舰载", "対空砲": "防空炮",
    "レーダー": "雷达", "ソナー": "声呐", "魚雷": "鱼雷", "主砲": "主炮", "副砲": "副炮",
    "装甲": "装甲", "対魚雷": "反鱼雷", "砲弾": "炮弹", "大型": "大型", "中型": "中型",
    "小型": "小型", "極小型": "极小型", "試作": "试制", "実験": "实验", "夜間": "夜间",
    "コア": "核心", "スペシャル": "特别", "パンダ": "熊猫", "ひなまつり編": "雏祭篇",
    "編": "篇", "ニードルライト": "针光", "コアコード": "核心代码", "紅蓮": "红莲",
    "オバケバルーン": "鬼怪气球", "イビルコーティング": "邪恶涂层", "禁忌眠れる棺": "禁忌沉睡之棺",
    "ぶらり": "漫游", "極ぶら": "极·漫游", "ムライド": "穆莱德", "八硝": "八硝",
    "除隊": "退役", "ソーダ": "苏打", "クッキー": "曲奇", "勝負しよう": "来决胜吧",
}
LOCAL_SUBSTRINGS = {
    "バンカー・ヒル": "邦克山",
    "バンカー·ヒル": "邦克山",
    "ル・マラン": "勒·马兰",
    "ル·マラン": "勒·马兰",
    "ジャーウィス": "贾维斯",
    "ジャウィス": "贾维斯",
    "ミニゲーベン": "迷你戈本",
}

# Model-reviewed overrides for names where mechanical kana conversion loses
# meaning. Ship names intentionally excluded from this table and task scope.
MODEL_NAME_EXACT = {
    "誓いのラムネ": "誓约弹珠汽水",
    "クリスマスツリーキャンディー": "圣诞树糖果",
    "クリスマスキャンディー": "圣诞糖果",
    "対魚雷バルジ(大)": "大型反鱼雷突出部",
    "対魚雷バルジ（大）": "大型反鱼雷突出部",
    "お化けカボチャ隊の装飾品": "南瓜鬼怪队装饰品",
    "ハロウィンのかぼちゃ": "万圣节南瓜",
    "クリスマスツリー": "圣诞树",
    "ネイチャークロス": "自然十字",
    "パールクリスタル": "珍珠水晶",
    "豆腐ステーキきのこソースかけ": "蘑菇酱豆腐牛排",
    "プライムステーキ": "精选牛排",
    "クリスマスキャンディーステッキ": "圣诞糖果手杖",
    "追想ファクター（アンノウンΜ）": "追想因子（未知Μ）",
    "思念ファクター（アンノウンΜ）": "思念因子（未知Μ）",
    "ちゃんと仕事すべき": "应该认真工作",
    "デートしない？": "要约会吗？",
    "モーちゃんスタンプ": "小莫印章",
    "初回チャージ特典戦姫選択箱": "首次充值特典战姬选择箱",
    "走れオトメ": "奔跑吧，少女！",
    "豪華装備交換コイン": "豪华装备兑换硬币",
    "指令ポイント（スペシャル）": "指令点数（特别）",
}


def load_ship_helpers(repo_root: Path):
    path = repo_root / "tools" / "localize-ship-names.py"
    spec = importlib.util.spec_from_file_location("blueoath_ship_helpers", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_cn_reference(reference_root: Path, file_name: str) -> dict[str, Any]:
    json_path = reference_root / f"{Path(file_name).stem}.json"
    if not json_path.exists():
        return {}
    try:
        document = json.loads(json_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    rows = document.get("Rows", document.get("rows")) if isinstance(document, dict) else None
    if not isinstance(rows, list):
        return {}
    result: dict[str, Any] = {}
    for row in rows:
        if not isinstance(row, dict):
            continue
        row_id = row.get("Id", row.get("id"))
        if row_id is not None:
            result[str(row_id)] = row.get("Data", row.get("value"))
    return result


def usable_cn_reference(reference: Any, original: str) -> bool:
    return (
        isinstance(reference, str)
        and bool(reference.strip())
        and reference != original
        and not JP_KANA_RE.search(reference)
        and TEXT_TOKEN_RE.findall(reference) == TEXT_TOKEN_RE.findall(original)
    )


def usable_db(path: Path) -> bool:
    if not path.exists() or path.stat().st_size == 0:
        return False
    try:
        con = sqlite3.connect(path)
        table = con.execute("select 1 from sqlite_master where type='table' and name='DBObject'").fetchone()
        rows = con.execute("select 1 from DBObject limit 1").fetchone() if table else None
        con.close()
        return bool(table and rows)
    except sqlite3.Error:
        return False


def translate_name(value: str, translator: Any) -> str:
    model_translation = MODEL_NAME_EXACT.get(value)
    if model_translation is not None:
        return model_translation
    jp_hint = getattr(translator, "JP_HAN_HINT_RE", None)
    # Only single-glyph Japanese variants trigger conversion. Characters from
    # multi-glyph terms such as 攻撃機 overlap normal Simplified Chinese.
    variant_chars = "".join(key for key in JP_TO_CN if len(key) == 1)
    variant_re = re.compile("[" + re.escape(variant_chars) + "]")
    if not JP_KANA_RE.search(value) and not (jp_hint and jp_hint.search(value)) and not variant_re.search(value):
        return value
    seed = value
    for source, target in sorted(LOCAL_TERMS.items(), key=lambda item: -len(item[0])):
        seed = seed.replace(source, target)
    for source, target in sorted(LOCAL_SUBSTRINGS.items(), key=lambda item: -len(item[0])):
        seed = seed.replace(source, target)
    translated = translator.direct_pattern_translation(seed)
    if translated is None:
        translated = translator.heuristic_local_translation(seed)
    if translated is None:
        translated = seed
    if JP_KANA_RE.search(translated):
        heuristic = translator.heuristic_local_translation(translated)
        if heuristic is not None:
            translated = heuristic
    for source, target in sorted(LOCAL_SUBSTRINGS.items(), key=lambda item: -len(item[0])):
        translated = translated.replace(source, target)
    for source, target in sorted(JP_TO_CN.items(), key=lambda item: -len(item[0])):
        translated = translated.replace(source, target)
    return translated.replace("ー", "").replace("・", "·")


def main() -> int:
    repo_root = Path(__file__).resolve().parents[1]
    client_root = Path(r"E:\BlueOath Rebirth")
    config_root = client_root / "blueoath" / "blueoath_Data" / "StreamingAssets" / "config"
    work_root = client_root / "装备材料道具汉化"
    source_dir = work_root / "source-db"
    decoded_dir = work_root / "decoded-json"
    translated_dir = work_root / "translated-json"
    compiled_dir = work_root / "compiled-db"
    backup_dir = work_root / "backup" / "client-config"
    cn_reference_root = client_root / "国服config"
    for directory in (source_dir, decoded_dir, translated_dir, compiled_dir, backup_dir):
        directory.mkdir(parents=True, exist_ok=True)

    helpers = load_ship_helpers(repo_root)
    translator = helpers.load_local_translator(repo_root)
    cn_reference = {file_name: load_cn_reference(cn_reference_root, file_name) for file_name in TARGET_FILES}
    summary: dict[str, dict[str, int]] = {}
    mapping_rows: list[dict[str, Any]] = []

    for file_name in TARGET_FILES:
        current_db = config_root / file_name
        preserved_source = source_dir / file_name
        candidate_source = client_root / "客户端补丁" / "server" / "catalog" / "config" / file_name
        previous_item_source = client_root / "装备材料道具汉化" / "compiled-db" / file_name
        if file_name == "config_shop_goods.db" and usable_db(previous_item_source):
            source_db = previous_item_source
        elif usable_db(candidate_source):
            source_db = candidate_source
        elif usable_db(preserved_source):
            source_db = preserved_source
        else:
            source_db = current_db
        if not source_db.exists():
            raise FileNotFoundError(source_db)
        if source_db.resolve() != (source_dir / file_name).resolve():
            shutil.copy2(source_db, source_dir / file_name)
        if source_db.resolve() != (compiled_dir / file_name).resolve():
            shutil.copy2(source_db, compiled_dir / file_name)

        source_con = sqlite3.connect(source_db)
        try:
            rows = source_con.execute("select id,indexid,jsonbytes from DBObject order by id").fetchall()
        finally:
            source_con.close()

        decoded: list[dict[str, Any]] = []
        translated: list[dict[str, Any]] = []
        changes: list[tuple[str, bytes]] = []
        stats = {"rows": 0, "valid_json": 0, "names": 0, "jp_before": 0, "changed": 0, "cn_reference": 0, "jp_after": 0}
        references = cn_reference.get(file_name, {})
        for row_id, index_id, raw in rows:
            row_id = str(row_id)
            index_id = "" if index_id is None else str(index_id)
            stats["rows"] += 1
            payload = helpers.decode_payload(bytes(raw))
            if payload is None:
                decoded.append({"id": row_id, "indexid": index_id, "json": None})
                translated.append({"id": row_id, "indexid": index_id, "json": None})
                continue
            stats["valid_json"] += 1
            new_payload = payload
            original_name = None
            translated_name = None
            if isinstance(payload, dict) and isinstance(payload.get(FIELD), str):
                stats["names"] += 1
                original_name = payload[FIELD]
                stats["jp_before"] += int(bool(JP_KANA_RE.search(original_name)))
                row_reference = references.get(row_id)
                reference_name = row_reference.get(FIELD) if isinstance(row_reference, dict) else None
                if usable_cn_reference(reference_name, original_name):
                    translated_name = reference_name
                    stats["cn_reference"] += 1
                    translation_source = "cn-reference"
                else:
                    translated_name = translate_name(original_name, translator)
                    translation_source = "internal"
                if translated_name != original_name:
                    new_payload = dict(payload)
                    new_payload[FIELD] = translated_name
                    stats["changed"] += 1
                stats["jp_after"] += int(bool(JP_KANA_RE.search(translated_name)))
                mapping_rows.append({"file": file_name, "id": row_id, "source": translation_source, "original_name": original_name, "translated_name": translated_name, "changed": translated_name != original_name})
            decoded.append({"id": row_id, "indexid": index_id, "json": payload})
            translated.append({"id": row_id, "indexid": index_id, "json": new_payload})
            if new_payload is not payload:
                changes.append((row_id, helpers.encode_payload(new_payload)))

        helpers.dump_json(decoded_dir / f"{file_name}.json", decoded)
        helpers.dump_json(translated_dir / f"{file_name}.json", translated)

        compiled_con = sqlite3.connect(compiled_dir / file_name)
        try:
            for row_id, encoded in changes:
                compiled_con.execute("update DBObject set jsonbytes=? where id=?", (sqlite3.Binary(encoded), row_id))
            compiled_con.commit()
            integrity = compiled_con.execute("pragma integrity_check").fetchone()[0]
            if integrity != "ok":
                raise RuntimeError(f"{file_name}: integrity_check={integrity}")
        finally:
            compiled_con.close()
        stats["integrity_ok"] = 1
        summary[file_name] = stats

    helpers.dump_json(work_root / "translation-map.json", mapping_rows)
    helpers.dump_json(work_root / "run-summary.json", {"files": summary, "xor_key": 0x55, "field": FIELD, "cn_reference_root": str(cn_reference_root), "network_translation": False})
    print(json.dumps({"work_root": str(work_root), "files": summary}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
