#!/usr/bin/env python3
"""Simplified-Chinese localization for first-batch UI and system text DBs."""

from __future__ import annotations

import importlib.util
import json
import re
import shutil
import sqlite3
from pathlib import Path
from typing import Any


TARGET_FIELDS = {
    "config_function_info.db": {"name", "comment", "open_show_name", "description"},
    "config_attribute.db": {"attr_name", "attr_direction", "attr_unit"},
    "config_equip_type.db": {"show_name"},
    "config_access.db": {"name"},
    "config_chapter_type.db": {"top_text"},
    "config_game_limits.db": {"desc"},
    "config_loading_tips.db": {"tips"},
    "config_pushnotice.db": {"text", "notice", "name"},
    "config_evaluate.db": {"description"},
}
JP_KANA_RE = re.compile(r"[ぁ-ゟァ-ヿ]")
TEXT_TOKEN_RE = re.compile(r"[0-9０-９]+|%[sd]|\{[^}]+\}")

MODEL_EXACT = {
    "「基地」が開放されると「風呂場」が使用できます。": "解锁“基地”后即可使用“浴室”。",
    "コラボ図鑑": "联动图鉴", "コラボ": "联动", "ドロップ率UP": "掉落率UP", "グルメミッション": "美食任务",
    "ムーバー": "移动者", "最強艦隊": "最强舰队", "ラボ": "实验室", "深海回廊": "深海回廊",
    "バベル10周年イベント": "Babel十周年活动", "調合": "调合", "氷の元素": "冰元素", "火の元素": "火元素",
    "魚雷ミサイル": "鱼雷导弹", "イベント物語": "活动剧情",
    "潜水艦の水中潜航時間に影響する（潜航時は潜水値を消耗し、浮上時は回復する）": "影响潜艇水下潜航时间（潜航时消耗潜水值，上浮时恢复）",
    "魚雷装填時間": "鱼雷装填时间", "潜水艦の魚雷装填時間に影響する": "影响潜艇鱼雷装填时间",
    "弾薬が底をついたら、艦隊の戦闘力が大幅に低下してしまいます！": "弹药耗尽后，舰队战斗力会大幅下降！",
    "第二戦速から全速に切り替える度に、一時的に加速効果が獲得する事ができます": "每次从第二战速切换至全速时，可暂时获得加速效果",
    "敵精鋭艦隊を壊滅する": "歼灭敌方精锐舰队",
    "<color=#3f5064>左や右に旋回することで向きを変える</color>": "<color=#3f5064>向左或向右转向即可改变方向</color>",
    "<color=#3f5064>ボタンを押して射撃</color>": "<color=#3f5064>按下按钮射击</color>",
    "<color=#3f5064>小さなターゲットを当たるとより高いダメージが与えられる！</color>": "<color=#3f5064>命中小目标可以造成更高伤害！</color>",
    "当海域での戦闘は<color=#3f5064>80秒の昼戦</color>です": "本海域战斗为<color=#3f5064>80秒昼战</color>",
    "当海域での戦闘は<color=#3f5064>60秒の夜戦</color>です": "本海域战斗为<color=#3f5064>60秒夜战</color>",
}

ACCESS_TRANSLATIONS = {
    "イベント 瑞鶴パネル": "活动 瑞鹤面板", "海域2-4をクリアする 暗雲迫る（初回クリア）": "通关海域2-4 暗云逼近（首次通关）",
    "海域3-4 黒雲立ち込める（初回クリア）": "海域3-4 乌云密布（首次通关）", "海域5-4 一刻を争う（初回クリア）": "海域5-4 分秒必争（首次通关）",
    "海域7-4 北方群島（初回クリア）": "海域7-4 北方群岛（首次通关）", "毎日·EXモード": "每日·EX模式",
    "太陽の化身": "太阳化身", "海鶴の宴ー期間限定ガチャ": "海鹤之宴—限时卡池",
    "夜の帳N7-2 自発的に出撃する": "夜幕N7-2 主动出击", "夜の帳N7-3 待ち伏せする": "夜幕N7-3 进行埋伏",
    "戦域の友『アンノウンM編』": "战域之友《未知M篇》", "戦域ショップ": "战域商店", "イベントショップ": "活动商店",
    "指令イベント": "指令活动", "海域6-2 夜のやみ": "海域6-2 夜之暗", "愛宕＆高雄 雨宿り編": "爱宕＆高雄·避雨篇",
    "ニエロの装甲姫ｘ峰昇龍人姫ー出現率ＵＰガチャ": "尼罗的装甲姬×峰昇龙人姬—概率UP卡池",
    "永遠の姉妹ー期間限定出現率ガチャ": "永恒姐妹—限时概率UP卡池", "装備開発一般ガチャ": "装备开发普通卡池",
    "鎖縛のムーバースレイヤー期間限定出現率ガチャ": "枷锁的移动者杀手限时概率UP卡池", "深海の記憶": "深海的记忆",
    "共闘ショップ-ファクター·Γ": "联合作战商店-因子·Γ", "共闘ショップ-ファクター·Ｈ": "联合作战商店-因子·H",
    "シェフの暴走": "厨师暴走", "共闘期間限定ショップ": "联合作战限时商店", "グルメミッション": "美食任务",
    "夏の大運動会": "夏日大运动会", "艦隊特集II": "舰队特辑II", "旅人たちと耳元の囁き": "旅人与耳畔细语",
    "虹色の埋蔵金": "彩虹宝藏", "最強艦隊": "最强舰队", "交換ショップ": "兑换商店",
    "対決！大艦隊作戦元日篇！": "对决！大舰队作战·元旦篇！", "浮生夢の如し": "浮生若梦",
    "デイリ任務": "每日任务", "週間任務": "每周任务", "指令ショップ": "指令商店",
    "【花咲き物語】期間限定ガチャ": "【花开物语】限时卡池", "ガチャ-【初の潜水艦】": "卡池-【首艘潜艇】",
    "【島々の間を吹き抜ける風】期間限定出現ガチャ": "【吹过群岛之间的风】限时概率UP卡池",
    "【旅人たちと耳元の囁き】期間限定出現率ガチャ": "【旅人与耳畔细语】限时概率UP卡池",
    "【旅人たちと耳元の囁き】期間限定出現ガチャ": "【旅人与耳畔细语】限时概率UP卡池",
    "【咲き誇り花のように】期間限定出現ガチャ": "【如盛放之花】限时概率UP卡池", "赤改造ショップ": "红色改造商店",
    "覚ませ!赤い悪夢!": "醒来吧！赤色噩梦！", "世界ショップ": "世界商店", "イベント補給品2": "活动补给品2",
    "イベント-期間中累計21日ログイン": "活动期间累计登录21天",
}


MODEL_TERMS = {
    "基地": "基地", "風呂場": "浴室", "改造": "改造", "前哨基地": "前哨基地", "機能": "功能",
    "ドロップ率UP": "掉落率UP", "コラボ図鑑": "联动图鉴", "コラボ": "联动", "ラボ": "实验室",
    "指令イベント": "指令活动", "スペシャル": "特别", "バベル10周年イベント": "Babel十周年活动",
    "開放": "解锁", "解放": "解锁", "調合": "调合", "指揮官": "指挥官", "レベル": "等级", "頑張って": "请努力",
    "物語": "剧情", "真実とは": "真相", "海域調査": "海域调查", "地区": "地区", "クリア": "通关",
    "オート戦闘": "自动战斗", "シナリオ": "剧情", "海域": "海域", "支援": "支援", "デイリークエスト": "每日任务",
    "早送り": "快进", "突破": "突破", "強化": "强化", "除隊": "退役", "合成": "合成", "建造": "建造",
    "学院": "学院", "戦術": "战术", "訓練所": "训练所", "イベントシナリオ": "活动剧情", "イベント海域": "活动海域",
    "クリア評価訓練所": "通关评价训练所", "AR海戦": "AR海战", "テロップ": "字幕", "挑戦": "挑战",
    "物資争奪戦": "物资争夺战", "味方": "我方", "敵": "敌方", "スキル": "技能", "魚雷発射動画スキップ": "鱼雷发射动画跳过", "発動動画スキップ": "动画跳过",
    "効果動画スキップ": "效果动画跳过", "装備解体": "装备分解", "ムーバー防衛線": "移动者防卫线", "陣形変更": "阵型变更",
    "大艦隊": "大舰队", "着せ替え": "换装", "時間限定作戦": "限时作战", "ハロウィンイベント": "万圣节活动",
    "装備エフェクト": "装备特效", "図鑑": "图鉴", "雑誌": "杂志", "作戦": "作战", "最終決戦": "最终决战",
}


def model_term_translate(value: str) -> str:
    translated = value
    for source, target in sorted(MODEL_TERMS.items(), key=lambda item: -len(item[0])):
        translated = translated.replace(source, target)
    return translated.replace("機能が", "功能").replace("機能", "功能").replace("・", "·")


def has_kana(value: str) -> bool:
    return bool(JP_KANA_RE.search(value))


def load_helpers(repo_root: Path):
    path = repo_root / "tools" / "localize-ship-names.py"
    spec = importlib.util.spec_from_file_location("blueoath_first_batch_helpers", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_cn_reference(reference_root: Path, file_name: str) -> dict[str, Any]:
    path = reference_root / f"{Path(file_name).stem}.json"
    if not path.exists():
        return {}
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
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
        and not has_kana(reference)
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


def translate_internal(value: str, translator: Any, file_name: str, field: str) -> str:
    if value in MODEL_EXACT:
        return MODEL_EXACT[value]
    if file_name == "config_access.db" and value in ACCESS_TRANSLATIONS:
        return ACCESS_TRANSLATIONS[value]
    if file_name == "config_function_info.db":
        if field in {"name", "description"}:
            return model_term_translate(value)
        if field == "open_show_name":
            match = re.fullmatch(r"<color=([^>]+)>(.*?)</color>機能が開放されました！", value)
            if match:
                return f"<color={match.group(1)}>{model_term_translate(match.group(2))}</color>功能已解锁！"
            match = re.fullmatch(r"<color=([^>]+)>(.*?)</color>機能が開放された！", value)
            if match:
                return f"<color={match.group(1)}>{model_term_translate(match.group(2))}</color>功能已解锁！"
            match = re.fullmatch(r"<color=([^>]+)>(.*?)</color>機能始動！", value)
            if match:
                return f"<color={match.group(1)}>{model_term_translate(match.group(2))}</color>功能启动！"
        if field == "comment":
            match = re.fullmatch(r"指揮官レベル([0-9０-９-]+)(?:に達すると|で)「(.*?)」(?:が開放|を解放)(?:されます|できるので)、?頑張って(?:レベルを上げて|レベルアップして)ください(?:ね)?！", value)
            if match:
                return f"达到指挥官等级{match.group(1)}后将解锁“{model_term_translate(match.group(2))}”。请努力提升等级！"
            match = re.fullmatch(r"海域調査<color=([^>]+)>(.*?)</color>をクリアすると開放されます！", value)
            if match:
                return f"通关海域调查<color={match.group(1)}>{match.group(2)}</color>后即可解锁！"
            match = re.fullmatch(r"<color=([^>]+)>(.*?)</color>をクリアすると(.*?)が開放されます！", value)
            if match:
                return f"通关<color={match.group(1)}>{model_term_translate(match.group(2))}</color>后将解锁{model_term_translate(match.group(3))}！"
    if file_name == "config_game_limits.db":
        match = re.fullmatch(r"指揮官レベル([0-9０-９]+)s*([≧≥])([0-9０-９]+)", value)
        if match:
            return f"指挥官等级{match.group(2)}{match.group(3)}"
        match = re.fullmatch(r"朝日ショップ-指揮官レベル：(.+)", value)
        if match:
            return f"朝日商店-指挥官等级：{match.group(1)}"
        match = re.fullmatch(r"累計チャージ≥([0-9０-９]+)福袋コイン消費", value)
        if match:
            return f"累计充值消耗≥{match.group(1)}福袋币"
        if value == "購入上限に達したので、購入できません":
            return "已达到购买上限，无法购买"
        if value == "1体獲得で":
            return "获得1个即可"
        if value == "破滅支援令状を使えるLv3":
            return "可使用毁灭支援令状Lv3"
        if value == "戦隊装備がSR品質以上の九四式46cm三連装砲を4個装備する":
            return "装备4门SR品质以上的九四式46cm三联装炮"
        translated = model_term_translate(value)
        translated = translated.replace("戦艦", "战列舰").replace("巡洋戦艦", "战列巡洋舰")
        translated = translated.replace("軽巡洋艦", "轻巡洋舰").replace("重巡洋艦", "重巡洋舰")
        translated = translated.replace("駆逐艦", "驱逐舰").replace("錬金術士", "炼金术士")
        translated = translated.replace("累計チャージ", "累计充值").replace("福袋コイン", "福袋币")
        translated = translated.replace("消費", "消耗").replace("購入上限に達したので、購入できません", "已达到购买上限，无法购买")
        return translated
    if file_name == "config_evaluate.db":
        exact_evaluate = {
            "当海域のBOSS艦隊に二隻の護衛原種があります": "本关卡的BOSS舰队中有2艘护卫原种",
            "当海域のBOSS艦隊に<color=#3f5064>護衛原種が1隻含まれる</color>": "本关卡的BOSS舰队中包含<color=#3f5064>1艘护卫原种</color>",
            "当海域のBOSS艦隊に<color=#3f5064>護衛原種が3隻含まれる</color>": "本关卡的BOSS舰队中包含<color=#3f5064>3艘护卫原种</color>",
            "敵艦1隻につき<color=#3f5064>二回まで復活する</color>が、<color=#3f5064>炎上や浸水ダメージ</color>により沈められた場合復活しなくなる": "每艘敌舰最多复活2次，但如果因<color=#3f5064>着火或进水伤害</color>沉没，则不会复活",
            "敵艦1隻につき<color=#3f5064>五回まで復活する</color>が、<color=#3f5064>炎上や浸水ダメージ</color>により沈められた場合復活しなくなる": "每艘敌舰最多复活5次，但如果因<color=#3f5064>着火或进水伤害</color>沉没，则不会复活",
            "敵艦1隻につき<color=#3f5064>1回復活する</color>が、<color=#3f5064>炎上や浸水ダメージ</color>により沈められた場合復活できなくなる": "每艘敌舰复活1次，但如果因<color=#3f5064>着火或进水伤害</color>沉没，则无法复活",
        }
        if value in exact_evaluate:
            return exact_evaluate[value]
    translated = model_term_translate(value)
    if translated != value and not has_kana(translated):
        return translated
    translated = getattr(translator, "INTERNAL_DIRECT", {}).get(value)
    if translated is None:
        translated = translator.direct_pattern_translation(value)
    if translated is None:
        translated = translator.heuristic_local_translation(value)
    if translated is None:
        translated = value
    if has_kana(translated):
        cleaned = translator.heuristic_local_translation(translated)
        if cleaned is not None:
            translated = cleaned
    return translated.replace("ー", "").replace("・", "·")


def main() -> int:
    repo_root = Path(__file__).resolve().parents[1]
    client_root = Path(r"E:\BlueOath Rebirth")
    config_root = client_root / "blueoath" / "blueoath_Data" / "StreamingAssets" / "config"
    cn_reference_root = client_root / "国服config"
    work_root = client_root / "第一批功能汉化"
    source_dir = work_root / "source-db"
    decoded_dir = work_root / "decoded-json"
    translated_dir = work_root / "translated-json"
    compiled_dir = work_root / "compiled-db"
    backup_dir = work_root / "backup" / "client-config"
    for directory in (source_dir, decoded_dir, translated_dir, compiled_dir, backup_dir):
        directory.mkdir(parents=True, exist_ok=True)

    helpers = load_helpers(repo_root)
    translator = helpers.load_local_translator(repo_root)
    references = {file_name: load_cn_reference(cn_reference_root, file_name) for file_name in TARGET_FIELDS}
    summary: dict[str, dict[str, int]] = {}
    mappings: list[dict[str, Any]] = []

    for file_name, fields in TARGET_FIELDS.items():
        current_db = config_root / file_name
        source_db = source_dir / file_name
        if not usable_db(source_db):
            source_db = current_db
        if not current_db.exists() or not source_db.exists():
            raise FileNotFoundError(file_name)
        if source_db.resolve() != (source_dir / file_name).resolve():
            shutil.copy2(source_db, source_dir / file_name)
        shutil.copy2(current_db, backup_dir / file_name)
        shutil.copy2(source_db, compiled_dir / file_name)

        con = sqlite3.connect(source_db)
        rows = con.execute("select id,indexid,jsonbytes from DBObject order by id").fetchall()
        con.close()
        decoded: list[dict[str, Any]] = []
        translated_rows: list[dict[str, Any]] = []
        changes: list[tuple[str, bytes]] = []
        stats = {"rows": 0, "valid_json": 0, "text_fields": 0, "jp_before": 0, "changed": 0, "cn_reference": 0, "jp_after": 0}
        file_references = references.get(file_name, {})

        for row_id, index_id, raw in rows:
            row_id = str(row_id)
            index_id = "" if index_id is None else str(index_id)
            stats["rows"] += 1
            payload = helpers.decode_payload(bytes(raw))
            if payload is None:
                decoded.append({"id": row_id, "indexid": index_id, "json": None})
                translated_rows.append({"id": row_id, "indexid": index_id, "json": None})
                continue
            stats["valid_json"] += 1
            new_payload = payload
            if isinstance(payload, dict):
                row_reference = file_references.get(row_id)
                for field in fields:
                    value = payload.get(field)
                    if not isinstance(value, str) or not value.strip():
                        continue
                    stats["text_fields"] += 1
                    stats["jp_before"] += int(has_kana(value))
                    reference = row_reference.get(field) if isinstance(row_reference, dict) else None
                    if usable_cn_reference(reference, value):
                        mapped = reference
                        source = "cn-reference"
                        stats["cn_reference"] += 1
                    else:
                        mapped = translate_internal(value, translator, file_name, field)
                        source = "internal"
                    stats["jp_after"] += int(has_kana(mapped))
                    if mapped != value:
                        if new_payload is payload:
                            new_payload = dict(payload)
                        new_payload[field] = mapped
                        stats["changed"] += 1
                        mappings.append({"file": file_name, "id": row_id, "field": field, "source": source, "original": value, "translated": mapped})
            decoded.append({"id": row_id, "indexid": index_id, "json": payload})
            translated_rows.append({"id": row_id, "indexid": index_id, "json": new_payload})
            if new_payload is not payload:
                changes.append((row_id, helpers.encode_payload(new_payload)))

        helpers.dump_json(decoded_dir / f"{file_name}.json", decoded)
        helpers.dump_json(translated_dir / f"{file_name}.json", translated_rows)
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

    helpers.dump_json(work_root / "translation-map.json", mappings)
    helpers.dump_json(work_root / "run-summary.json", {"files": summary, "field_scope": {k: sorted(v) for k, v in TARGET_FIELDS.items()}, "cn_reference_root": str(cn_reference_root), "network_translation": False, "xor_key": 0x55})
    print(json.dumps({"work_root": str(work_root), "files": summary}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
