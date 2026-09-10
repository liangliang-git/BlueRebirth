#!/usr/bin/env python3
"""Offline localization for BlueOath Rebirth shop and recharge content."""

from __future__ import annotations

import importlib.util
import json
import re
import shutil
import sqlite3
from pathlib import Path
from typing import Any


TARGET_FIELDS = {
    "config_shop.db": {"name"},
    "config_shop_goods.db": {"name"},
    "config_recharge.db": {"name", "show_name", "desc", "nodouble_desc", "sdkdesc"},
    "config_recharge_selective.db": {"name"},
}
JP_KANA_RE = re.compile(r"[ぁ-ゟァ-ヿ]")
TEXT_TOKEN_RE = re.compile(r"[0-9０-９]+|%[sd]|\{[^}]+\}")
MODEL_SHOP_EXACT = {
    "赤改造ウィークリーショップ": "红色改造周商店", "アイスクリーム": "冰淇淋", "ラフィーの記念メダル": "拉菲的纪念勋章",
    "かぼちゃのお菓子": "南瓜糖果", "姉妹メダル": "姐妹勋章", "策士メダル": "策士勋章", "ヴェネトの証": "维内托之证",
    "誓いのラムネ": "誓约弹珠汽水", "クリスマスツリーキャンディー": "圣诞树糖果", "クリスマスキャンディー": "圣诞糖果",
    "スイートハート": "甜心", "お化けカボチャ隊の装飾品": "南瓜鬼怪队装饰品", "ハロウィンのかぼちゃ": "万圣节南瓜",
    "クリスマスツリー": "圣诞树", "ネイチャークロス": "自然十字", "パールクリスタル": "珍珠水晶",
    "豆腐ステーキきのこソースかけ": "蘑菇酱豆腐牛排", "プライムステーキ": "精选牛排", "クリスマスキャンディーステッキ": "圣诞糖果手杖",
    "ちゃんと仕事すべき": "应该认真工作", "デートしない？": "要约会吗？", "モーちゃんスタンプ": "小莫印章",
    "追想ファクター（アンノウンΜ）": "追想因子（未知Μ）", "思念ファクター（アンノウンΜ）": "思念因子（未知Μ）",
    "初回チャージ特典戦姫選択箱": "首次充值特典战姬选择箱", "走れオトメ": "奔跑吧，少女！",
}

MODEL_SHOP_TERMS = {
    "ウィークリー": "周", "ショップ": "商店", "アイスクリーム": "冰淇淋", "メダル": "勋章", "かぼちゃ": "南瓜",
    "お菓子": "糖果", "クリスマス": "圣诞", "ツリー": "树", "キャンディー": "糖果", "キャンディ": "糖果",
    "スイートハート": "甜心", "ラムネ": "弹珠汽水", "ファクター": "因子", "証": "之证", "高級": "高级",
    "祈願石": "祈愿石", "ログイン": "登录", "獲得": "获得", "毎日": "每日", "増量": "增加",
}


def model_shop_translation(value: str) -> str | None:
    if value in MODEL_SHOP_EXACT:
        return MODEL_SHOP_EXACT[value]
    translated = value
    for source, target in sorted(MODEL_SHOP_TERMS.items(), key=lambda item: -len(item[0])):
        translated = translated.replace(source, target)
    translated = translated.replace("ー", "—").replace("・", "·")
    return translated if translated != value and not JP_KANA_RE.search(translated) else None
JP_VARIANT_RE = re.compile(r"[弾薬撃機徹専許結鋭銀勲気揮姫鶴鴎風証憶揚勝負隊濃]")
JP_TO_CN = {
    "弾": "弹", "薬": "药", "撃": "击", "機": "机", "徹": "彻", "専": "专", "許": "许",
    "結": "结", "鋭": "锐", "銀": "银", "勲": "勋", "気": "气", "揮": "挥", "姫": "姬",
    "鶴": "鹤", "鴎": "鸥", "風": "风", "証": "证", "憶": "忆", "揚": "扬", "勝": "胜",
    "負": "负", "隊": "队", "濃": "浓", "戦": "战", "艦": "舰", "駆": "驱", "軽": "轻",
    "砲": "炮", "魚": "鱼", "敵": "敌",
}
LOCAL_TERMS = {
    "ショップ": "商店", "購入して即": "购买后立即", "購入": "购买", "価格": "价格", "個": "个",
    "無料": "免费", "優遇セット": "优惠套装", "お手頃": "实惠", "極上": "顶级", "福袋": "福袋",
    "ダイヤ": "钻石", "配当金": "分红", "獲得": "获得", "即": "立即", "月額": "月卡", "累計": "累计",
    "初回": "首次", "期限": "期限", "限定": "限定", "販売": "售卖", "ボーナス": "奖励", "通常": "普通",
    "残り": "剩余", "キャンペーン": "活动", "パック": "礼包", "ギフト": "礼物", "イベント": "活动",
    "戦姫": "战姬", "装備": "装备", "補給": "补给", "交換": "兑换", "コイン": "硬币", "チケット": "兑换券",
    "お得": "特惠", "デイリー": "每日", "レア": "稀有", "激レア": "超稀有", "精鋭": "精英",
    "オース": "奥斯", "ムーバー": "移动者", "アンブラ": "安布拉", "ダイヤ": "钻石", "円": "日元",
}
POST_TERMS = {
    "交換": "兑换", "証": "之证", "初登場": "首次登场", "登場": "登场", "新しい": "新的",
    "太陽": "太阳", "化身": "化身", "月鶴": "月鹤", "支援娘": "支援少女", "朝日の": "朝日的",
    "高級": "高级", "極上": "顶级", "優遇": "优惠", "自動購読": "自动订阅",
}
SIMPLIFIED_MAP = {
    "期間限定": "限时", "期間": "期间", "時間": "时间", "限定品": "限定物品",
    "毎週": "每周", "特別": "特别", "供給": "补给", "物資": "物资", "特集": "特辑",
    "錬金": "炼金", "祈願": "祈愿", "超増": "超增", "新増": "新增",
    "選別": "精选", "厳選": "精选", "共鳴": "共鸣", "購読": "订阅", "自動購読": "自动订阅",
    "毎日": "每日", "任務": "任务", "報酬": "奖励", "回数": "次数", "作戦": "作战",
    "掃討": "扫荡", "編成": "编成", "艦隊": "舰队", "獲得": "获得", "購入": "购买",
    "成功後": "成功后", "即時": "立即", "上級": "高级", "通常": "普通", "増量": "增加",
    "推薦": "推荐", "優遇": "优惠", "福袋コイン": "福袋硬币", "戦姫": "战姬",
    "裝備": "装备", "補給": "补给", "交換": "兑换", "選擇": "选择", "選択": "选择",
    "獎勵": "奖励", "獎品": "奖品", "購買": "购买", "價格": "价格", "免費": "免费",
    "貨幣": "货币", "資源": "资源", "現金": "现金", "點數": "点数", "數量": "数量",
    "發放": "发放", "發售": "发售", "開始": "开始", "結束": "结束", "總計": "总计",
    "個": "个", "長": "长", "門": "门", "強": "强", "後": "后", "將": "将", "達": "达",
    "機": "机", "艦": "舰", "戰": "战", "姫": "姬", "鶴": "鹤", "鴎": "鸥", "選": "选",
    "擇": "择", "獲": "获", "報": "报", "會": "会", "來": "来", "與": "与", "為": "为",
    "這": "这", "們": "们", "國": "国", "體": "体", "實": "实", "務": "务", "還": "还",
    "點": "点", "對": "对", "應": "应", "於": "于", "樂": "乐", "學": "学", "業": "业",
    "書": "书", "車": "车", "東": "东", "兩": "两", "裡": "里", "現": "现", "號": "号",
    "總": "总", "廣": "广", "確": "确", "經": "经", "種": "种", "節": "节", "買": "买",
    "賣": "卖", "屬": "属", "臺": "台", "無": "无", "優": "优", "萬": "万", "進": "进",
    "見": "见", "從": "从", "開": "开", "說": "说", "彈": "弹", "砲": "炮", "驅": "驱",
    "輕": "轻", "敵": "敌", "納": "纳", "際": "际", "邊": "边", "傳": "传", "續": "续",
    "認": "认", "讓": "让", "賦": "赋", "頁": "页", "貨": "货", "價": "价", "貴": "贵",
    "賽": "赛", "贈": "赠", "購": "购", "費": "费", "產": "产", "預": "预", "備": "备",
    "錄": "录", "釋": "释", "獻": "献", "類": "类", "庫": "库", "單": "单", "組": "组",
    "編": "编", "製": "制", "裝": "装", "飾": "饰", "發": "发", "証": "证", "憶": "忆",
    "揚": "扬", "勲": "勋", "薬": "药", "撃": "击", "専": "专", "許": "许", "結": "结",
    "鋭": "锐", "銀": "银", "氣": "气", "勝": "胜", "負": "负", "隊": "队", "護": "护",
    "時": "时", "間": "间", "毎": "每", "週": "周", "増": "增", "錬": "炼", "願": "愿",
    "資": "资", "別": "别", "薦": "荐", "補": "补", "厳": "严", "給": "给", "後": "后",
}


def simplify_text(value: str) -> str:
    for source, target in sorted(SIMPLIFIED_MAP.items(), key=lambda item: -len(item[0])):
        value = value.replace(source, target)
    return value


def translate_shop_pattern(value: str) -> str | None:
    """Deterministic local translations for recurring shop templates."""
    if "<<n" in value:
        parts = value.split("<<n")
        mapped_parts = [translate_shop_pattern(part) or part for part in parts]
        if mapped_parts != parts:
            return "<<n".join(mapped_parts)
    exact = {
        "おすすめ": "推荐",
        "厳選パック": "精选礼包",
        "累計チャージ": "累计充值",
        "クリスマスショップ": "圣诞商店",
        "新しいムーバーの初登場": "新移动者首次登场",
        "雪の玉交換": "雪球兑换",
        "パープル紋章": "紫色纹章",
        "桜の花びら": "樱花花瓣",
        "謎の開発機材料": "神秘开发机材料",
        "異界からの支援娘": "异界支援少女",
        "自動購読": "自动订阅",
        "無料優遇セット": "免费优惠套装",
    }
    if value in exact:
        return exact[value]
    patterns = [
        (r"(\d+)ダイヤ", lambda m: f"{m.group(1)}钻石"),
        (r"(\d+)福袋コイン", lambda m: f"{m.group(1)}福袋硬币"),
        (r"朝日の配当金（(\d+)日）", lambda m: f"朝日分红（{m.group(1)}天）"),
        (r"朝日の配当金（自動購読）", lambda _m: "朝日分红（自动订阅）"),
        (r"購入して即(\d+)ダイヤ獲得", lambda m: f"购买后立即获得{m.group(1)}钻石"),
        (r"購入して即(\d+)福袋コイン獲得", lambda m: f"购买后立即获得{m.group(1)}福袋硬币"),
        (r"購入して即朝日の配当金サービス獲得", lambda _m: "购买后立即获得朝日分红服务"),
        (r"福袋コイン(\d+)個獲得", lambda m: f"获得{m.group(1)}个福袋硬币"),
        (r"初回限定-追加(\d+)個", lambda m: f"首次限定-追加{m.group(1)}个"),
        (r"最大燃料値\+(\d+)増量", lambda m: f"最大燃料上限+{m.group(1)}"),
        (r"毎日任務のボーナス報酬回数\+(\d+)増量", lambda m: f"每日任务奖励次数+{m.group(1)}"),
        (r"物資大作戦の達成報酬\+(\d+)(?:%|％)を増量", lambda m: f"物资大作战达成奖励提高{m.group(1)}%"),
        (r"掃討作戦機能にて、編成できる艦隊が(\d+)つ増えます", lambda m: f"扫荡作战中可编成舰队增加{m.group(1)}支"),
        (r"毎日ログインでダイヤ(\d+)個獲得", lambda m: f"每日登录获得钻石{m.group(1)}个"),
        (r"毎日ログインで燃料(\d+)個獲得", lambda m: f"每日登录获得燃料{m.group(1)}个"),
        (r"毎日ログインで(\d+)個の上級祈願石獲得", lambda m: f"每日登录获得{m.group(1)}个高级祈愿石"),
        (r"毎日ログインで(\d+)個の高級祈願石獲得", lambda m: f"每日登录获得{m.group(1)}个高级祈愿石"),
        (r"購入成功後<color=([^>]+)>、即時に</color>ダイヤ(\d+)個と(\d+)個の通常祈願石獲得", lambda m: f"购买成功后<color={m.group(1)}>立即获得</color>钻石{m.group(2)}个和{m.group(3)}个普通祈愿石"),
        (r"購入成功後<color=([^>]+)>、即時に</color>ダイヤ(\d+)個獲得", lambda m: f"购买成功后<color={m.group(1)}>立即获得</color>钻石{m.group(2)}个"),
        (r"瑞鶴パネルミッションをクリア時に72時間限定‐瑞鶴専属セットを獲得するチャンス！（一回限定）。", lambda _m: "完成瑞鹤面板任务后，限时72小时内有机会获得瑞鹤专属套装！（仅限一次）。"),
    ]
    for pattern, replace in patterns:
        match = re.fullmatch(pattern, value)
        if match:
            return replace(match)
    tagged = value
    tagged = tagged.replace("お手頃優遇セット", "实惠优惠套装")
    tagged = tagged.replace("高級優遇セット", "高级优惠套装")
    tagged = tagged.replace("極上優遇セット", "顶级优惠套装")
    tagged = re.sub(r"価格：福袋コイン(\d+)個", r"价格：福袋硬币\1个", tagged)
    tagged = tagged.replace("朝日の配当金", "朝日分红")
    if tagged != value:
        return tagged
    if "<size=18>ショップ</size>" in value:
        return value.replace("ショップ", "商店")
    return None


def load_helpers(repo_root: Path):
    path = repo_root / "tools" / "localize-ship-names.py"
    spec = importlib.util.spec_from_file_location("blueoath_shop_helpers", path)
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


def translate_text(value: str, translator: Any, force: bool = False) -> str:
    model_translated = model_shop_translation(value)
    if model_translated is not None:
        return model_translated
    if not force and not JP_KANA_RE.search(value) and not JP_VARIANT_RE.search(value):
        return simplify_text(value)
    translated = translate_shop_pattern(value)
    if translated is None:
        translated = getattr(translator, "INTERNAL_DIRECT", {}).get(value)
    if translated is None:
        translated = translator.direct_pattern_translation(value)
    if translated is None:
        translated = translator.heuristic_local_translation(value)
    if translated is None:
        translated = value
    if JP_KANA_RE.search(translated):
        cleaned = translator.heuristic_local_translation(translated)
        if cleaned is not None:
            translated = cleaned
    for source, target in sorted(JP_TO_CN.items(), key=lambda item: -len(item[0])):
        translated = translated.replace(source, target)
    for source, target in sorted(POST_TERMS.items(), key=lambda item: -len(item[0])):
        translated = translated.replace(source, target)
    translated = translated.replace("朝日の配当金", "朝日分红")
    translated = translated.replace("配当金", "分红")
    translated = translated.replace("自動購読", "自动订阅")
    translated = translated.replace("ー", "").replace("・", "·")
    return simplify_text(translated)


def main() -> int:
    repo_root = Path(__file__).resolve().parents[1]
    client_root = Path(r"E:\BlueOath Rebirth")
    config_root = client_root / "blueoath" / "blueoath_Data" / "StreamingAssets" / "config"
    work_root = client_root / "商城汉化"
    source_dir = work_root / "source-db"
    decoded_dir = work_root / "decoded-json"
    translated_dir = work_root / "translated-json"
    compiled_dir = work_root / "compiled-db"
    backup_dir = work_root / "backup" / "client-config"
    cn_reference_root = client_root / "国服config"
    for directory in (source_dir, decoded_dir, translated_dir, compiled_dir, backup_dir):
        directory.mkdir(parents=True, exist_ok=True)

    helpers = load_helpers(repo_root)
    translator = helpers.load_local_translator(repo_root)
    cn_reference = {file_name: load_cn_reference(cn_reference_root, file_name) for file_name in TARGET_FIELDS}
    previous_item_backup = client_root / "装备材料道具汉化" / "backup" / "client-config"
    previous_item_compiled = client_root / "装备材料道具汉化" / "compiled-db"
    server_source_root = client_root / "客户端补丁" / "server" / "catalog" / "config"
    summary: dict[str, dict[str, int]] = {}
    mapping_rows: list[dict[str, Any]] = []

    for file_name, fields in TARGET_FIELDS.items():
        current_db = config_root / file_name
        if file_name == "config_shop_goods.db" and (previous_item_compiled / file_name).exists():
            # Preserve stronger item-name translation from previous pass.
            original_db = previous_item_compiled / file_name
        elif usable_db(server_source_root / file_name):
            original_db = server_source_root / file_name
        elif usable_db(source_dir / file_name):
            original_db = source_dir / file_name
        else:
            original_db = current_db
        if not current_db.exists() or not original_db.exists():
            raise FileNotFoundError(current_db)
        if original_db.resolve() != (source_dir / file_name).resolve():
            shutil.copy2(original_db, source_dir / file_name)
        shutil.copy2(original_db, compiled_dir / file_name)
        shutil.copy2(current_db, backup_dir / file_name)

        con = sqlite3.connect(original_db)
        rows = con.execute("select id,indexid,jsonbytes from DBObject order by id").fetchall()
        con.close()
        decoded: list[dict[str, Any]] = []
        translated: list[dict[str, Any]] = []
        changes: list[tuple[str, bytes]] = []
        stats = {"rows": 0, "valid_json": 0, "text_fields": 0, "jp_before": 0, "changed": 0, "cn_reference": 0, "jp_after": 0}
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
            if isinstance(payload, dict):
                for field in fields:
                    value = payload.get(field)
                    if not isinstance(value, str) or not value:
                        continue
                    stats["text_fields"] += 1
                    stats["jp_before"] += int(bool(JP_KANA_RE.search(value) or JP_VARIANT_RE.search(value)))
                    row_reference = references.get(row_id)
                    reference_value = row_reference.get(field) if isinstance(row_reference, dict) else None
                    if usable_cn_reference(reference_value, value):
                        mapped = reference_value
                        stats["cn_reference"] += 1
                        translation_source = "cn-reference"
                    else:
                        mapped = translate_text(value, translator, force=file_name == "config_recharge.db")
                        translation_source = "internal"
                    stats["jp_after"] += int(bool(JP_KANA_RE.search(mapped)))
                    if mapped != value:
                        if new_payload is payload:
                            new_payload = dict(payload)
                        new_payload[field] = mapped
                        stats["changed"] += 1
                        mapping_rows.append({"file": file_name, "id": row_id, "field": field, "source": translation_source, "original": value, "translated": mapped})
            decoded.append({"id": row_id, "indexid": index_id, "json": payload})
            translated.append({"id": row_id, "indexid": index_id, "json": new_payload})
            if new_payload is not payload:
                changes.append((row_id, helpers.encode_payload(new_payload)))

        helpers.dump_json(decoded_dir / f"{file_name}.json", decoded)
        helpers.dump_json(translated_dir / f"{file_name}.json", translated)
        con = sqlite3.connect(compiled_dir / file_name)
        for row_id, encoded in changes:
            con.execute("update DBObject set jsonbytes=? where id=?", (sqlite3.Binary(encoded), row_id))
        con.commit()
        integrity = con.execute("pragma integrity_check").fetchone()[0]
        con.close()
        if integrity != "ok":
            raise RuntimeError(f"{file_name}: integrity_check={integrity}")
        stats["integrity_ok"] = 1
        summary[file_name] = stats

    helpers.dump_json(work_root / "translation-map.json", mapping_rows)
    helpers.dump_json(work_root / "run-summary.json", {"files": summary, "field_scope": {k: sorted(v) for k, v in TARGET_FIELDS.items()}, "cn_reference_root": str(cn_reference_root), "network_translation": False, "xor_key": 0x55})
    print(json.dumps({"work_root": str(work_root), "files": summary}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
