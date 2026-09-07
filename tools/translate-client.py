#!/usr/bin/env python3
"""Translate Blue Oath SQLite config strings and preserve the XOR-encoded format."""

from __future__ import annotations

import argparse
import concurrent.futures
import collections
import html
import json
import os
import re
import shutil
import sqlite3
import sys
import time
import unicodedata
from pathlib import Path
from typing import Any
from urllib.parse import quote

import requests

XOR_KEY = 0x55
JP_KANA_RE = re.compile(r"[\u3040-\u30ff\u31f0-\u31ff]")
JP_HAN_HINT_RE = re.compile(
    r"[々〆〄鶴賀龍榛嵐翔艦戦姫撃敵弾砲駆逐払拠営覧覧鎮鎌薩薇輝邂逅]")
JP_RE = re.compile(r"[\u3040-\u30ff\u31f0-\u31ff]|[々〆〄鶴賀龍榛嵐翔艦戦姫撃敵弾砲駆逐払拠営覧鎮鎌薩薇輝邂逅]")
CN_RE = re.compile(r"[\u3400-\u4dbf\u4e00-\u9fff]")
# Some rows already contain Simplified Chinese from the regional client, but
# retain Japanese hint characters elsewhere in the same configuration set.
CN_SIMPLIFIED_RE = re.compile(
    r"[爱办宝贝边标并参仓层场车达单导灯对队吨东动断发飞费废复该个给关汉号华画回会机继价将节仅进经举据开课块来类离联两灵录吗买卖么门难鸟农盘确让热认杀设审声势适书术数说虽孙态条铁听头图团为卫稳问无线乡响协写兴选压严样药爷业亿应优邮鱼员远愿运杂战证执质专转装总组轻驱级舰题课种际铁气营选敌炮弹击败过归间击区终旧梦广归场妇众别属绪]")
MARKER_RE = re.compile(r"ZXQ(\d{7})")

# Game terminology. Replacing these before machine translation keeps common
# Blue Oath terms stable even when Google's general model chooses a literal
# Japanese gloss.
GLOSSARY = {
    "戦姫": "战姬", "艦隊": "舰队", "出撃": "出击", "編成": "编队",
    "遠征": "远征", "任務": "任务", "指揮官": "指挥官", "装備": "装备",
    "強化": "强化", "改造": "改造", "修理": "维修", "建造": "建造",
    "戦闘": "战斗", "攻撃": "攻击", "防御": "防御", "敵": "敌人",
    "報酬": "奖励", "資源": "资源", "設定": "设置", "確認": "确认",
    "開始": "开始", "終了": "结束", "購入": "购买", "売却": "出售",
    "所持": "持有", "受取": "领取", "図鑑": "图鉴", "補給": "补给",
    "燃料": "燃料", "弾薬": "弹药", "損傷": "损伤", "大破": "大破",
    "中破": "中破", "小破": "小破", "勝利": "胜利", "敗北": "败北",
    "回復": "恢复", "成長": "成长", "レベル": "等级", "スキル": "技能",
    "キャラ": "角色", "イベント": "活动", "ログイン": "登录", "ホーム": "主页",
    "ショップ": "商店", "メール": "邮件", "フレンド": "好友", "ランキング": "排行榜",
    "ガチャ": "抽卡", "ダイヤ": "钻石", "コイン": "金币", "ポイント": "点数",
    "無料": "免费", "有料": "付费", "駆逐艦": "驱逐舰", "軽巡": "轻巡洋舰",
    "重巡": "重巡洋舰", "巡洋艦": "巡洋舰", "戦艦": "战列舰", "空母": "航空母舰",
    "潜水艦": "潜艇", "艦": "舰", "船": "舰船", "海域": "海域",
    "基地": "基地", "艦娘": "舰娘", "少女": "少女", "物資": "物资",
    "経験値": "经验值", "好感度": "好感度", "限界突破": "突破", "受け取る": "领取",
    "キャンセル": "取消", "はい": "是", "いいえ": "否", "戻る": "返回",
}

# Hand-authored translations for recurring short labels. This stays local and
# is used only after the CN client reference mapping, with no remote service.
INTERNAL_DIRECT = {
    "ミニゲーム": "小游戏", "ステージ": "关卡", "イベントシナリオ": "活动剧情",
    "プロローグ": "序章", "チャプター1": "第1章", "チャプター2": "第2章",
    "お散歩": "散步", "スキンシップ": "亲密互动", "思い出話": "回忆往事",
    "先輩の時間": "前辈的时间", "秘密にして？": "替我保密？", "暇つぶし": "消磨时间",
    "宝箱の行方": "宝箱的去向", "雨宿り編": "避雨篇", "蝕む悪夢": "侵蚀的噩梦",
    "赤き海の彼方へ": "前往赤色海洋彼端", "ボトル世界": "瓶中世界", "怒涛を駆ける": "驰骋于怒涛",
    "花咲き物語·前編": "花开物语·前篇", "花咲き物語·後編": "花开物语·后篇",
    "戦場に駆けつける": "奔赴战场", "激戦の昼と夜": "激战的昼与夜", "風に吹かれて": "迎风而行",
    "恩讐の彼方": "恩仇彼端", "未完の日記": "未完的日记", "オマハ級": "奥马哈级",
    "アイス＆スパイ": "冰淇淋与间谍", "挑戦海域": "挑战海域", "格好付け": "装酷",
    "泣き虫のお姉さん": "爱哭的大姐姐", "幻の黒猫": "幻之黑猫", "戦闘のプロ": "战斗专家",
    "夢歩き": "梦中漫步", "若き獅子": "年轻的雄狮", "兼好さ": "随和", "臆病さ": "胆怯",
    "リーダーシップ": "领导力", "奇妙な思想": "奇妙的思想", "正常さ": "正常",
    "次の計画は~": "下一个计划是~", "人を励ます": "鼓励他人", "海の力": "海洋之力",
    "みんなの名前": "大家的名字", "ムードメーカー": "气氛制造者", "スクラップ": "废料",
    "合いの手": "帮腔", "ゆるふわ": "轻松柔和", "シスコン": "姐控", "アイドルマスター": "偶像大师",
    "威嚇さ": "威吓", "ティータイム": "下午茶时间", "オースパワー": "奥斯力量",
    "スピードマスター": "速度大师", "ブレイン": "头脑", "バケツリレー": "水桶接力",
    "カンバン娘": "招牌女郎", "お姉さまが来た？": "姐姐来了？", "チームチャンネル": "队伍频道",
    "ワールドボイス": "世界语音", "チームボイス": "队伍语音", "ちびっこ": "小不点",
    "ボトムレス": "无底", "サイレント": "寂静", "サイレント+": "寂静+", "サイレント++": "寂静++",
    "ひらめき": "灵感", "隠された真実": "隐藏的真相", "名探偵の推理": "名侦探的推理",
    "不穏な気配": "不祥的气息", "罪深き雪": "罪孽之雪", "虹架かる戦場": "彩虹横跨的战场",
    "リセット": "重置", "最終決戦": "最终决战", "邂逅": "邂逅", "陽動作戦": "佯攻作战",
    "潜入作戦": "潜入作战", "救出作戦": "救出作战", "協同作戦": "协同作战", "突破作戦": "突破作战",
    "ただの子供だけ": "只是个孩子", "目標と挑戦？": "目标与挑战？", "「未来」の足音": "“未来”的脚步声",
    "ネコの目・1": "猫之眼·1", "ネコの目・2": "猫之眼·2", "ネコの目・3": "猫之眼·3",
    "私の「間違い」": "我的“错误”", "アンブラへ": "前往安布拉", "あなたの名前は…": "你的名字是……",
    "旅ノ夢": "旅之梦", "暇ノ夢": "闲之梦", "遊惰ノ夢": "游惰之梦", "惑いノ夢": "迷惘之梦",
    "志ノ夢": "志之梦", "寤寐ノ夢": "寤寐之梦", "祭り来たり": "祭典来临", "憂うつ": "忧郁",
    "上客と騒動": "贵客与骚动", "改ざん": "篡改", "情勢はいかに": "局势如何", "「自分」の価値": "“自我”的价值",
    "花が咲く春": "花开的春天", "協力者＆カキ氷": "协力者与刨冰", "いつか、きっと": "总有一天，一定会",
    "波乱の始まり！": "波澜的开始！", "真剣勝負だ！": "认真决胜负！", "勇気と希望": "勇气与希望",
    "ずらかるぞ！": "撤退啦！", "暗流立ち込める": "暗流涌动", "オイゲンの贈り物": "欧根的礼物",
    "マッコールの為に": "为了麦考尔", "悩み相談": "烦恼咨询", "コツ": "诀窍", "似ているところ": "相似之处",
    "打ち明け": "坦白", "いいアイデア": "好主意", "久々の日常": "久违的日常", "迷い込む": "误入",
    "悪夢を祓う": "驱散噩梦", "快晴の兆し": "放晴的征兆", "出会いが縁": "相遇结缘",
    "憂いを祓う言葉": "驱散忧愁的话语", "万事尽きた！": "万事休矣！", "ダメなものはダメ": "不行就是不行",
    "何とかなる！ ": "总会有办法的！ ", "暗流と潮汐": "暗流与潮汐", "誘い": "邀请", "求めるもの": "追求之物",
    "過去の跡": "过去的痕迹", "捨て去られた記録": "被遗弃的记录", "可能性への追求": "追寻可能性",
    "水平面を眺めて": "眺望海平面", "夢の中で": "在梦中", "正義の味方たちよ": "正义的伙伴们",
    "不良の矜持": "不良的矜持", "コンビ結成": "结成搭档", "神秘の導き": "神秘的指引",
    "面接タイム": "面试时间", "クリプレの選び方": "圣诞礼物的选择方法", "歳末の足どり": "年末的脚步",
    "年末の鍋料理": "年末火锅", "大晦日の食卓": "除夕餐桌", "それぞれのHNY": "各自的新年快乐",
    "海上攻防戦": "海上攻防战", "牙を剥く": "露出獠牙", "月下の襲撃": "月下袭击", "戦火が再び": "战火再起",
    "波の両端": "波浪两端", "長い夜": "漫长的夜", "砲火が轟く": "炮火轰鸣", "敌の影": "敌人的影子",
    "不意を撃つ": "出其不意地射击", "眠れぬ夜": "不眠之夜", "かぼちゃの祝福": "南瓜的祝福",
    "雨音のする方に": "前往传来雨声的方向", "アンタッチャブル": "不可触碰", "不意討ち": "偷袭",
    "体験クエスト": "体验任务", "ひきょうもの": "卑鄙小人", "おくびょうもの": "胆小鬼",
    "助っ人登場！": "援军登场！", "拷問の時間？": "拷问时间？", "犬猿のコンビ": "水火不容的搭档",
    "博士さま、登場！": "博士大人登场！", "欺くこと": "欺骗", "テンプレート": "模板", "天下り": "空降任职",
    "ソサエティ": "协会", "インペリアル": "帝国", "リリウム": "百合", "キューリア": "库里亚",
    "アナトリア": "安纳托利亚", "ノースライン": "北方航线", "ムーバー": "搬运者", "メダル": "奖章",
    "ダイヤ": "钻石", "メカニカルメダル": "机械奖章", "福袋コイン": "福袋硬币", "艦隊コインII": "舰队硬币II",
    "残り6日": "剩余6天", "残り5日": "剩余5天", "残り4日": "剩余4天", "残り3日": "剩余3天",
    "残り2日": "剩余2天", "ラスト1日": "最后1天", "初回クリア": "首次通关", "必ず": "必定",
    "パーツ": "零件", "ジェム": "宝石", "ドロップ": "掉落", "必ずドロップ": "必定掉落",
    "確率ドロップ": "概率掉落", "アイテム": "道具", "メイン": "主线", "アンコウ": "鮟鱇",
    "スキル": "技能", "ラボアイテム": "实验室道具", "クリア報酬": "通关奖励", "隼鷹スタンプ": "隼鹰印章",
}

# Keep format/markup tokens out of the translation model. These tokens occur
# in UI strings and plot text and must round-trip byte-for-byte.
TOKEN_RE = re.compile(
    r"(?:<<n|<(?!(?:[^=<>]*[:：]))[^>\r\n]+>|\{[^{}\r\n]+\}|%(?:\d+\$)?[+#0\- ]*\d*(?:\.\d+)?[a-zA-Z]|%%|\\[nrt]|\n|\r)"
)
TIME_MARKER_RE = re.compile(r"<%%[^>\r\n]+>")


def xor_bytes(data: bytes) -> bytes:
    return bytes(b ^ XOR_KEY for b in data)


def protect(text: str) -> tuple[str, dict[str, str]]:
    tokens: dict[str, str] = {}

    def repl(match: re.Match[str]) -> str:
        key = f"QZPH{len(tokens):04d}QZ"
        tokens[key] = match.group(0)
        return key

    return TOKEN_RE.sub(repl, text), tokens


def restore(text: str, tokens: dict[str, str]) -> str:
    for key, value in tokens.items():
        text = text.replace(key, value)
    return text


def walk_strings(value: Any, path: tuple[str, ...] = ()):
    if isinstance(value, str):
        yield path, value
    elif isinstance(value, dict):
        for key, child in value.items():
            yield from walk_strings(child, path + (str(key),))
    elif isinstance(value, list):
        for index, child in enumerate(value):
            yield from walk_strings(child, path + (str(index),))


def value_at_path(value: Any, path: tuple[str, ...]) -> Any:
    for part in path:
        if isinstance(value, dict):
            value = value.get(part)
        elif isinstance(value, list) and part.isdigit() and int(part) < len(value):
            value = value[int(part)]
        else:
            return None
    return value


def object_identity(value: Any) -> tuple[str, str] | None:
    if not isinstance(value, dict):
        return None
    for key in (
        "id", "ship_id", "item_id", "equip_id", "equipment_id", "character_id",
        "skill_id", "task_id", "story_id", "chapter_id", "code", "uid",
    ):
        if key in value and isinstance(value[key], (str, int, float)):
            return key, str(value[key])
    return None


def walk_aligned(source: Any, reference: Any, path: tuple[str, ...] = ()):
    """Walk source JSON while aligning version-skewed object arrays by IDs."""
    if isinstance(source, str):
        reference_value = reference if isinstance(reference, str) else None
        yield path, source, reference_value
        return
    if isinstance(source, dict):
        reference_dict = reference if isinstance(reference, dict) else {}
        for key, child in source.items():
            yield from walk_aligned(child, reference_dict.get(key), path + (str(key),))
        return
    if isinstance(source, list):
        reference_list = reference if isinstance(reference, list) else []
        by_identity: dict[tuple[str, str], Any] = {}
        for item in reference_list:
            identity = object_identity(item)
            if identity is not None and identity not in by_identity:
                by_identity[identity] = item
        for index, child in enumerate(source):
            identity = object_identity(child)
            reference_child = by_identity.get(identity) if identity is not None else None
            if reference_child is None and index < len(reference_list):
                reference_child = reference_list[index]
            yield from walk_aligned(child, reference_child, path + (str(index),))


def normalized_text(value: str) -> str:
    value = unicodedata.normalize("NFKC", value)
    for source, target in {
        "★": "☆", "峯": "峰", "焔": "焰", "劇": "剧", "戦": "战",
        "闘": "斗", "駆": "驱", "軽": "轻", "鋭": "锐", "艦": "舰",
        "姫": "姬", "竜": "龙", "龍": "龙", "鶴": "鹤", "賀": "贺",
        "専": "专", "砲": "炮", "撃": "击", "弾": "弹", "敵": "敌",
        "會": "会", "貝": "贝", "備": "备", "誓": "誓", "樂": "乐", "國": "国",
        "‐": "-", "‑": "-", "‒": "-", "–": "-", "—": "-", "－": "-",
    }.items():
        value = value.replace(source, target)
    return value


def expand_normalized_mapping(mapping: dict[str, str]) -> dict[str, str]:
    expanded = dict(mapping)
    for source, target in mapping.items():
        expanded.setdefault(normalized_text(source), target)
    return expanded


def align_symbols(source: str, translated: str) -> str:
    if "☆" in source and "★" not in source:
        translated = translated.replace("★", "☆")
    elif "★" in source and "☆" not in source:
        translated = translated.replace("☆", "★")
    return translated


def prepare(text: str) -> tuple[str, dict[str, str]]:
    protected, tokens = protect(text)
    for src, dst in sorted(GLOSSARY.items(), key=lambda x: len(x[0]), reverse=True):
        if src in protected:
            key = f"ZXTERM{len(tokens):04d}ZX"
            tokens[key] = dst
            protected = protected.replace(src, key)
    return protected.replace("\r\n", " QZNL0000QZ ").replace("\n", " QZNL0000QZ "), tokens


def parse_mobile_google(body: str) -> str:
    match = re.search(r'<div class="result-container">(.*?)</div>', body, re.DOTALL)
    if not match:
        raise ValueError("Google mobile response missing result container")
    return html.unescape(match.group(1))


def translate_batch(items: list[tuple[int, str]], session: requests.Session) -> dict[int, str]:
    prepared: dict[int, tuple[str, dict[str, str]]] = {i: prepare(s) for i, s in items}
    source = " ".join(f"ZXQ{i:07d} {value[0]}" for i, value in prepared.items())
    url = "https://translate.google.com/m?sl=ja&tl=zh-CN&q=" + quote(source, safe="")
    response = session.get(url, timeout=90)
    response.raise_for_status()
    translated = parse_mobile_google(response.text)
    found = list(MARKER_RE.finditer(translated))
    if len(found) != len(items):
        raise ValueError(f"marker mismatch: expected {len(items)}, got {len(found)}")
    result: dict[int, str] = {}
    for pos, match in enumerate(found):
        # Google occasionally changes a digit inside an alphanumeric marker.
        # It preserves order, so bind by position and only use marker count
        # as the integrity check.
        idx = items[pos][0]
        start = match.end()
        end = found[pos + 1].start() if pos + 1 < len(found) else len(translated)
        value = translated[start:end].strip()
        value = value.replace("QZNL0000QZ", "\n")
        value = restore(value, prepared[idx][1])
        result[idx] = value
    return result


def translate_one(item: tuple[int, str], session: requests.Session) -> tuple[int, str]:
    for attempt in range(4):
        try:
            return next(iter(translate_batch([item], session).items()))
        except Exception:
            if attempt == 3:
                raise
            time.sleep(1.5 * (attempt + 1))
    raise AssertionError("unreachable")


def translate_plain(item: tuple[int, str], session: requests.Session) -> tuple[int, str]:
    """Fallback for strings where Google drops or rewrites marker tokens."""
    index, text = item
    prepared, tokens = prepare(text)
    url = "https://translate.google.com/m?sl=ja&tl=zh-CN&q=" + quote(prepared, safe="")
    response = session.get(url, timeout=90)
    response.raise_for_status()
    translated = parse_mobile_google(response.text)
    translated = translated.replace("QZNL0000QZ", "\n")
    return index, restore(translated, tokens)


def collect_strings(config_root: Path) -> tuple[dict[str, int], dict[int, str]]:
    values: dict[str, int] = {}
    for db_path in sorted(config_root.glob("config_*.db")):
        with sqlite3.connect(f"file:{db_path}?mode=ro", uri=True) as db:
            for _, _, encoded in db.execute("SELECT id,indexid,jsonbytes FROM DBObject WHERE id <> 'nill'"):
                data = json.loads(xor_bytes(encoded).decode("utf-8"))

                for _, value in walk_strings(data):
                    if JP_RE.search(value):
                        values.setdefault(value, 0)
                        values[value] += 1
    indexed = {i: value for i, value in enumerate(values)}
    return values, indexed


def is_existing_chinese(value: str) -> bool:
    """Recognize rows already translated by presence of Simplified-only glyphs."""
    # U+30FB middle dot is common in Chinese ship-name punctuation and is
    # included in the broad Japanese block; do not treat it as kana.
    kana_probe = value.replace("・", "")
    if not (CN_RE.search(value) and not JP_KANA_RE.search(kana_probe)):
        return False
    if CN_SIMPLIFIED_RE.search(value):
        return True
    # A few CN story rows use names such as 薇/鸾/呜, which are also present in
    # the Japanese-hint set but are unambiguous Simplified-Chinese prose here.
    return bool(re.search(r"[薇鸾呜]|最强|前来|讨教|拜访|没事|像素|姐妹|宿舍", value))


def compatible_reference_tokens(source: str, reference: str) -> bool:
    """Allow CN client to omit timing markers, but reject other format drift."""
    source_tokens = TOKEN_RE.findall(source)
    reference_tokens = TOKEN_RE.findall(reference)
    source_non_timing = [token for token in source_tokens if not TIME_MARKER_RE.fullmatch(token)]
    reference_non_timing = [token for token in reference_tokens if not TIME_MARKER_RE.fullmatch(token)]
    if source_non_timing != reference_non_timing:
        return False
    return all(token in source_tokens for token in reference_tokens)


def preserve_source_tokens(source: str, translated: str) -> str:
    """Keep source markup, placeholders, line breaks, and timing markers."""
    source_tokens = TOKEN_RE.findall(source)
    translated_tokens = TOKEN_RE.findall(translated)
    if source_tokens == translated_tokens:
        return translated
    source_plain = TOKEN_RE.sub("", source)
    translated_plain = TOKEN_RE.sub("", translated)
    source_length = max(1, len(source_plain))
    buckets: dict[int, list[str]] = collections.defaultdict(list)
    for marker in TOKEN_RE.finditer(source):
        before = TOKEN_RE.sub("", source[: marker.start()])
        target_position = round(len(before) * len(translated_plain) / source_length)
        buckets[target_position].append(marker.group(0))
    rebuilt: list[str] = []
    for index in range(len(translated_plain) + 1):
        rebuilt.extend(buckets.get(index, ()))
        if index < len(translated_plain):
            rebuilt.append(translated_plain[index])
    return "".join(rebuilt)


SHIP_NAME_DIRECT = {
    "高雄": "高雄", "愛宕": "爱宕", "カレイジャス": "勇敢", "朝日": "朝日",
    "利根": "利根", "ポートランド": "波特兰", "摩耶": "摩耶", "レンジャー": "游骑兵",
    "ワスプ": "胡蜂", "グローウォーム": "萤火虫", "ノーフォーク": "诺福克",
    "アドミラル・シェーア": "舍尔海军上将", "ビスマルク": "俾斯麦", "サンフアン": "圣胡安",
    "インディアナポリス": "印第安纳波利斯", "ラムダ": "拉姆达", "オークランド": "奥克兰",
    "クリーブランド": "克利夫兰", "吹雪": "吹雪", "アリシューザ": "阿瑞图萨",
    "日向": "日向", "時雨": "时雨", "伊勢": "伊势", "夕立": "夕立",
    "ダンケルク": "敦刻尔克", "寧海": "宁海", "ストラスブール": "斯特拉斯堡",
    "ネルソン": "纳尔逊", "祥鳳": "祥凤", "クイーン・エリザベス": "伊丽莎白女王",
    "メリーランド": "马里兰", "ケーニヒスベルク": "柯尼斯堡", "瑞鳳": "瑞凤",
    "ウォースパイト": "厌战", "蒼龍": "苍龙", "ウェストバージニア": "西弗吉尼亚",
    "カールスルーエ": "卡尔斯鲁厄", "ベルファスト": "贝尔法斯特", "クレイヴン": "克雷文",
    "ヤウズ・スルタン・セリム": "雅乌兹·苏丹·塞利姆", "ケルン": "科隆", "マッコール": "麦考尔",
    "アンソン": "安森", "ヘレナ": "海伦娜", "ジョンストン": "约翰斯顿", "天龍": "天龙",
    "バンカーヒル": "邦克山", "サンディエゴ": "圣地亚哥", "ハーミーズ": "竞技神",
    "フューリアス": "暴怒", "ラングレー": "兰利", "ドーセットシャー": "多塞特郡",
    "フッド": "胡德", "タスカルーサ": "塔斯卡卢萨", "プリンツ・オイゲン": "欧根亲王",
    "榛名": "榛名", "イータ": "伊塔", "グラーフ・ツェッペリン": "齐柏林伯爵",
    "ル・テリブル": "可怖", "イラストリアス": "光辉", "アリゾナ": "亚利桑那",
    "ヴォルタ": "伏尔塔", "ブリュッヒャー": "布吕歇尔", "ソーマレス": "萨默斯",
    "エディンバラ": "爱丁堡", "サヴェージ": "萨维奇", "水浴びキャット": "戏水小猫",
    "シャルンホルスト": "沙恩霍斯特", "フランクリン": "富兰克林", "グナイゼナウ": "格奈森瑙",
    "ミュー": "缪", "スパルヴィエロ": "斯帕尔维耶罗", "フォッシュ": "福煦",
    "アルジェリア": "阿尔及利亚", "ジャーヴィス": "贾维斯", "レナウン": "声望",
    "シカゴ": "芝加哥", "アラスカ": "阿拉斯加", "ル・ファンタスク": "空想",
    "加賀": "加贺", "ライザリン・シュタウト": "莱莎琳·斯托特", "モーリー": "莫利",
    "ヴィットリオ・ヴェネト": "维托里奥·维内托", "敵Z16": "敌方Z16", "翔鶴": "翔鹤",
    "サミュエル・Ｂ・ロバーツ": "塞缪尔·B·罗伯茨", "龍驤": "龙骧", "アルティリエーレ": "阿尔蒂列雷",
    "サミュエル・B・ロバーツ": "塞缪尔·B·罗伯茨",
    "ヘルマン": "赫尔曼", "アーク・ロイヤル": "皇家方舟", "ペンシルベニア": "宾夕法尼亚",
    "ラフィー": "拉菲", "ニューメキシコ": "新墨西哥", "アラバマ": "阿拉巴马",
    "サラトガ": "萨拉托加", "ヨークタウン": "约克城", "アドミラル・グラーフ・シュペー": "斯佩伯爵海军上将",
    "ホーネット": "大黄蜂", "サンフランシスコ": "旧金山", "マクシム・ゴーリキー": "马克西姆·高尔基",
    "モロトフ": "莫洛托夫", "ウィチタ": "威奇塔", "ボルチモア": "巴尔的摩",
    "ザラ": "扎拉", "レパルス": "反击", "キング・ジョージ5世": "乔治五世国王",
    "ノースカロライナ": "北卡罗来纳", "プリンス・オブ・ウェールズ": "威尔士亲王",
    "飛龍": "飞龙", "ブルックリン": "布鲁克林", "サウスダコタ": "南达科他",
    "グリッドレイ": "格里德利", "ホーエル": "霍埃尔", "アンドレア・ドーリア": "安德烈亚·多里亚",
    "ゼータ": "泽塔", "長門": "长门", "雪風": "雪风", "デューク・オブ・ヨーク": "约克公爵",
    "鳥海": "鸟海", "妙高": "妙高", "瑞鶴": "瑞鹤", "ミニハウ": "迷你豪",
    "ミニフィウメ": "迷你菲乌梅", "アーク·ロイヤル": "皇家方舟",
    "ライザリン・シュタウト": "莱莎琳·斯托特", "ライザリン·シュタウト": "莱莎琳·斯托特", "ライザ": "莱莎",
    "シグマ": "西格玛", "オメガ": "欧米伽", "リリ": "莉莉", "エセックス": "埃塞克斯",
    "オスカー": "奥斯卡", "羽黒": "羽黑", "比叡": "比叡", "金剛": "金刚", "綾波": "绫波",
    "大井": "大井", "北上": "北上", "フィウメ": "菲乌梅", "キーロフ": "基洛夫", "トレント": "特伦托",
    "ロドニー": "罗德尼", "ヴァンガード": "前卫", "サウスダコタ": "南达科他", "テーテューアス": "特修斯",
    "ムーバーイータ": "搬运者伊塔", "ムーバーゼータ": "搬运者泽塔", "ムーバーミュー": "搬运者缪",
    "ソーマレズ": "萨默斯",
    "赤城": "赤城", "Z39": "Z39", "フランクリン": "富兰克林", "サラトガ": "萨拉托加",
    "アラスカ": "阿拉斯加", "アルジェリア": "阿尔及利亚", "モロトフ": "莫洛托夫", "プリンツ・オイゲン": "欧根亲王",
    "アルジェリー": "阿尔及利亚", "伊168": "伊168",
    "アドミラル・ヒッパー": "希佩尔海军上将",
    "平海": "平海", "ハウ": "豪", "陸奥": "陆奥", "大鳳": "大凤", "隼鷹": "隼鹰",
    "ヒーアマン": "希尔曼", "羽黒": "羽黑", "推薦状": "推荐信", "万象": "万象", "ファクター": "因子",
}


BROKEN_SKIN_RE = re.compile(
    r"戦姫の大破着せ替えシリーズ！【(.+?)】の破損状態を保存した特別品。"
    r"戦闘開始から大破した気持ちで挑めば怖いもの無しだ([!！]+)"
)
COLOR_TEXT_RE = re.compile(r"(<color=[^>]+>)(.*?)(</color>)")


KANA_PHONETIC = {
    "ア": "阿", "イ": "伊", "ウ": "乌", "エ": "埃", "オ": "欧", "カ": "卡", "キ": "基", "ク": "库", "ケ": "克", "コ": "科",
    "サ": "萨", "シ": "希", "ス": "斯", "セ": "塞", "ソ": "索", "タ": "塔", "チ": "奇", "ツ": "茨", "テ": "特", "ト": "托",
    "ナ": "纳", "ニ": "尼", "ヌ": "努", "ネ": "内", "ノ": "诺", "ハ": "哈", "ヒ": "希", "フ": "弗", "ヘ": "赫", "ホ": "霍",
    "マ": "马", "ミ": "米", "ム": "姆", "メ": "梅", "モ": "莫", "ヤ": "亚", "ユ": "尤", "ヨ": "约", "ラ": "拉", "リ": "里",
    "ル": "鲁", "レ": "雷", "ロ": "罗", "ワ": "瓦", "ヲ": "沃", "ン": "恩", "ァ": "阿", "ィ": "伊", "ゥ": "乌", "ェ": "埃", "ォ": "欧",
    "ャ": "亚", "ュ": "尤", "ョ": "约", "ッ": "", "ヮ": "瓦", "・": "·", "ー": "",
}
HIRAGANA_PHONETIC = {
    "あ": "啊", "い": "伊", "う": "呜", "え": "诶", "お": "哦", "か": "卡", "き": "基", "く": "库", "け": "克", "こ": "科",
    "さ": "萨", "し": "希", "す": "斯", "せ": "塞", "そ": "索", "た": "塔", "ち": "奇", "つ": "茨", "て": "特", "と": "托",
    "な": "纳", "に": "尼", "ぬ": "努", "ね": "内", "の": "的", "は": "是", "ひ": "希", "ふ": "弗", "へ": "向", "ほ": "霍",
    "ま": "马", "み": "米", "む": "姆", "め": "梅", "も": "也", "や": "呀", "ゆ": "尤", "よ": "哟", "ら": "拉", "り": "里",
    "る": "鲁", "れ": "雷", "ろ": "罗", "わ": "哇", "を": "将", "ん": "嗯", "ぁ": "啊", "ぃ": "伊", "ぅ": "呜", "ぇ": "诶", "ぉ": "哦",
    "ゃ": "亚", "ゅ": "尤", "ょ": "约", "っ": "", "ゎ": "哇",
}


def heuristic_local_translation(value: str) -> str | None:
    """Last-resort offline conversion for residual Japanese-only rows."""
    if not JP_KANA_RE.search(value) and not JP_HAN_HINT_RE.search(value):
        return None
    result = value
    common_terms = {
        "ありがとうございます": "谢谢", "ありがとう": "谢谢", "おめでとう": "恭喜", "お願いします": "拜托",
        "おねがいします": "拜托", "よろしく": "请多关照", "ごめんなさい": "对不起", "ごめん": "抱歉",
        "どうしましたか": "怎么了", "わかりません": "不明白", "わかりました": "明白了", "わからない": "不知道",
        "おのれ": "可恶", "ファイトー": "加油", "グッド": "很好", "主砲激射": "主炮猛烈射击", "この海域": "本海域", "その海域": "该海域",
        "この": "本", "その": "该", "あの": "那个", "これ": "这个", "それ": "那个", "どの": "哪个",
        "ここ": "这里", "そこ": "那里", "どこ": "哪里", "なに": "什么", "はい": "是", "いいえ": "不是",
        "だめ": "不行", "すごい": "厉害", "ほんと": "真的", "本当に": "真的", "まだ": "还", "もう": "已经",
        "みんな": "大家", "あなた": "你", "わたし": "我", "たち": "们", "こと": "事情", "もの": "东西",
        "ください": "请", "して下さい": "请", "してください": "请", "獲得する事ができます": "可以获得",
        "獲得することができます": "可以获得", "使用する事で": "使用后", "使用することで": "使用后",
        "する事ができます": "可以", "することができます": "可以", "する事ができる": "可以", "することができる": "可以",
        "できません": "不能", "できます": "可以", "できる": "可以", "できない": "不能", "しました": "了",
        "します": "", "する": "", "している": "正在", "して": "进行", "された": "被", "されます": "会被", "される": "会被",
        "しましょう": "吧", "という": "的", "について": "关于", "のように": "一样", "ように": "地", "ために": "为了",
        "ですが": "但是", "けど": "不过", "または": "或", "および": "以及", "そして": "然后", "だから": "所以",
        "から": "从", "まで": "到", "より": "比", "だけ": "仅", "全て": "全部", "敵艦": "敌舰", "味方戦姫": "我方战姬",
        "錬金徹甲弾": "炼金穿甲弹", "練式": "炼式", "支援戦力": "支援战力", "砲刃矢石": "炮刃矢石",
        "戦姫": "战姬", "艦隊": "舰队", "艦": "舰", "戦力": "战力", "対雷撃": "对雷击", "対空": "对空",
        "対": "对", "有効": "有效", "自身": "自身", "消滅": "消灭", "効果": "效果", "下げる": "降低", "ごとに": "每次",
        "空母": "航母", "味方": "我方", "敵": "敌", "共鳴": "共鸣", "祈願壁": "祈愿墙", "チャンス": "机会",
        "戦姫": "战姬", "艦隊": "舰队", "艦": "舰", "装備": "装备", "装飾品": "装饰品", "アイテム": "道具",
        "イベント": "活动", "任務": "任务", "報酬": "奖励", "ショップ": "商店", "ガチャ": "卡池", "交換": "兑换",
        "ミッション": "任务", "ストーリー": "故事", "シーズン": "赛季", "ダメージ": "伤害", "アップ": "提升",
        "ダウン": "降低", "攻撃": "攻击", "命中": "命中", "耐久": "耐久", "戦闘": "战斗", "主砲": "主炮",
        "副砲": "副炮", "魚雷": "鱼雷", "航空": "航空", "火力": "火力", "回避": "回避", "消費": "消耗",
        "購入": "购买", "使用": "使用", "解放": "解锁", "必要": "需要", "可能": "可能", "成功": "成功", "失敗": "失败",
        "限定": "限定", "大型": "大型", "小型": "小型", "特別": "特殊", "記念": "纪念", "獲得": "获得", "一つ": "一个",
        "一体": "一名", "ランダム": "随机", "優先": "优先", "無獲得": "未拥有", "開けると": "打开后", "入手": "获得",
        "できます": "可以", "ござい": "有", "です": "是", "ます": "", "でした": "是", "です。": "。", "ます。": "。",
        "事": "事", "時": "时", "場合": "时", "毎": "每", "日": "天", "回": "次", "個": "个", "名": "名",
    }
    for source_term, target_term in sorted(SHIP_NAME_DIRECT.items(), key=lambda item: -len(item[0])):
        result = result.replace(source_term, target_term)
    for source_term, target_term in sorted(common_terms.items(), key=lambda item: -len(item[0])):
        result = result.replace(source_term, target_term)
    result = re.sub(r"[ぁ-ゖ]", lambda match: HIRAGANA_PHONETIC.get(match.group(), ""), result)
    result = re.sub(r"[ァ-ヺ]", lambda match: KANA_PHONETIC.get(match.group(), ""), result)
    result = result.replace("ー", "").replace("・", "·").replace("ヾ", "").replace("ゝ", "").replace("ゞ", "")
    if JP_KANA_RE.search(result):
        return None
    return result


def direct_pattern_translation(value: str) -> str | None:
    """Translate repeated local templates without any remote service."""
    if TIME_MARKER_RE.search(value):
        plain_value = TIME_MARKER_RE.sub("", value)
        plain_translation = direct_pattern_translation(plain_value)
        if plain_translation is not None:
            return preserve_source_tokens(value, plain_translation)
    exact_high_frequency = {
        "この海域では、次のアイテムを獲得できます": "在此海域可以获得以下道具",
        "ライザの世界に存在する材料です。": "莱莎世界中存在的材料。",
        "バンカー・ヒル": "邦克山",
        "軽巡専属-装備ガチャ": "轻巡专属-装备卡池",
        "轻巡専属-装備ガチャ": "轻巡专属-装备卡池",
        "攻撃ミス時": "攻击未命中时",
        "母港タップ1": "母港点击1",
        "母港タップ2": "母港点击2",
        "母港タップ3": "母港点击3",
        "当海域での戦闘は60秒の昼戦と20秒の夜戦です": "本海域战斗为60秒昼战和20秒夜战",
        "当海域での戦闘は90秒の昼戦です": "本海域战斗为90秒昼战",
        "当海域での戦闘は60秒の昼戦です": "本海域战斗为60秒昼战",
        "当海域での戦闘は120秒の昼戦です": "本海域战斗为120秒昼战",
        "当海域での戦闘は90秒の夜戦です": "本海域战斗为90秒夜战",
        "当海域での戦闘は80秒の夜戦です": "本海域战斗为80秒夜战",
        "謎の開発機に使われる材料です": "用于神秘开发机的材料。",
        "使用する事で、何かを獲得する事ができます。": "使用后可以获得某种物品。",
        "航空戦１": "航空战1",
        "航空戦２": "航空战2",
        "以下の報酬を獲得可能：": "可获得以下奖励：",
        "指令イベント\n（スペシャル）": "指令活动\n（特别）",
        "指令イベント\n(スペシャル)": "指令活动\n(特别)",
        "黒猫のグミ": "黑猫软糖",
        "黒猫のカップケーキ": "黑猫纸杯蛋糕",
        "美食マスターの道": "美食大师之路",
        "幸運はダイスから！": "幸运来自骰子！",
        "ブルースフィアの来訪者": "蓝色星球的访客",
        "フェニーの裏切り者": "菲妮的背叛者",
        "お年玉": "压岁钱",
        "休暇願い": "休假申请",
        "葉探し": "寻找树叶",
        "カウントダウンイベント": "倒计时活动",
        "約束のアイスクリーム": "约定的冰淇淋",
        "島々の間を吹き抜ける風": "穿过群岛之间的风",
        "除夜の鐘は響き渡る": "除夕钟声回荡",
        "夏姫祭--咲き誇り花のように": "夏姬祭——如盛放的花朵",
        "ラムダ/アイス＆スパイ": "拉姆达/冰淇淋与间谍",
        "ヴァムステック": "瓦姆斯泰克",
        "来るもの、去るもの": "来者与去者",
        "悪い子はおしりペンペンの刑": "坏孩子要接受打屁股惩罚",
        "高貴な美徳": "高贵的美德",
        "サニーライトフラワー": "阳光花",
        "おのれ…": "可恶……",
        "うっ…！": "呜……！",
        "おっけ～おっけ～": "好～好～",
        "SS評価：全ての敵艦を撃沈する\nS評価：敵艦の精鋭戦隊を撃沈する": "SS评价：击沉所有敌舰\nS评价：击沉敌舰的精锐舰队",
        "愛宕＆高雄‐姉妹の雨宿り編イベント交換コイン。\n下位のコインを5枚集めるとイベントページの*切替ボタンをタップして、一つ上位のコイン1枚と交換できます。（金＞銀＞銅＞白）": "爱宕＆高雄—姐妹避雨篇活动兑换币。\n收集5枚低级硬币后，点击活动页面的*切换按钮，即可兑换1枚更高级硬币。（金＞银＞铜＞白）",
        "祝100日記念セット【限定版】の一つ、獲得する事で、指揮官室内に装飾する事ができます。\n※倉庫⇒装飾品内の大型装飾品の祝100日記念セット【限定版】からON/OFF設定する事が可能です。": "百日纪念套装【限定版】之一，获得后可装饰在指挥官室内。\n※可在仓库⇒装饰品中的大型装饰品“百日纪念套装【限定版】”处设置开启/关闭。",
        "心血のこもった料理が拒否され続け、悲しみの果てに暴走するポートランドの復讐劇、これにて幕を開く。": "精心准备的料理不断遭到拒绝，悲伤至极而暴走的波特兰复仇剧，就此拉开帷幕。",
        "ライザ-ディヴェルの抱擁": "莱莎-迪维尔的拥抱",
        "エイプリルフールの箱": "愚人节的箱子",
        "SS評価：全敵艦撃沈、且つ味方戦姫大破数≤0隻でクリア\nS評価：全敵艦撃沈、且つ味方戦姫大破数≥1隻でクリア": "SS评价：击沉全部敌舰，且我方战姬大破数≤0艘通关\nS评价：击沉全部敌舰，且我方战姬大破数≥1艘通关",
        "艦首式ボイラー（大）": "舰首式锅炉（大）",
        "艦首式ボイラー（中）": "舰首式锅炉（中）",
        "錬金術師ライザ·シュタウト、長き冒険を経て身に付いた少しオトナの雰囲気をかもし出す衣装。落ち着いていて、淑やかな気質さえを身にまとうが、中身はあの前に進むことしか知らない、破天荒な少女に変わりは無い。": "炼金术士莱莎·斯托特，经过漫长冒险后获得了些许成熟气质的服装。外表沉稳优雅，但内心依旧是那个只知道向前、天马行空的少女。",
        "2022エイプリルフールの箱": "2022愚人节的箱子",
        "旅人たちと耳元の囁き": "旅人与耳畔的低语",
        "愛宕＆高雄\n雨宿り編": "爱宕＆高雄\n避雨篇",
        "シェフの暴走-NORMAL": "厨师暴走-普通",
        "アンノウンM-NORMAL": "未知M-普通",
        "シェフの暴走-HARD": "厨师暴走-困难",
        "アンノウンM-HARD": "未知M-困难",
        "アンノウンΓ-HELL": "未知Γ-地狱",
        "アンノウンH-HELL": "未知H-地狱",
        "シェフの暴走-HELL": "厨师暴走-地狱",
        "アンノウンM-HELL": "未知M-地狱",
        "シェフの暴走-EASY": "厨师暴走-简单",
        "アンノウンM-EASY": "未知M-简单",
        "世界ショップで獲得できる報酬": "可在世界商店获得的奖励",
        "【復刻版・ソロモンの戦い】イベントの報酬": "【复刻版·所罗门之战】活动奖励",
        "少女の思い、オースエネルギーとともに戦場で迸る。": "少女的思念，与奥斯能量一同在战场上迸发。",
        "指定戦姫チケットセット（限定版）": "指定战姬兑换券套装（限定版）",
        "限定イベント中の海域や任務にて、獲得可能な交換アイテム。たくさん集めて“祭り”を盛り上げよう。": "可在限时活动海域或任务中获得的兑换道具。收集更多道具，让“祭典”更加热闹吧。",
        "カタバミと三つ葉のオークに見守られながら": "在酢浆草与三叶橡树的守望下",
        "ニシン鉄砲シーズンⅡ": "鲱鱼炮赛季Ⅱ",
        "極限パルクールシーズンⅡ": "极限跑酷赛季Ⅱ",
        "障害物競走シーズンⅡ": "障碍赛跑赛季Ⅱ",
        "ニシン鉄砲 シーズンⅠ": "鲱鱼炮赛季Ⅰ",
        "極限パルクールシーズンⅠ": "极限跑酷赛季Ⅰ",
        "障害物競走シーズンⅠ": "障碍赛跑赛季Ⅰ",
        "8型ディーゼルサイクル ": "8型柴油循环 ",
        "錬金爆撃機‐コクーン": "炼金轰炸机—茧",
        "ヴィットリオ・ヴェネト・改": "维托里奥·维内托·改",
        "ヴィットリオ・ヴェネト・改二": "维托里奥·维内托·改二",
        "指揮官が厳選した食材達が入っている夢の美食セット！": "装有指挥官精心挑选食材的梦幻美食套装！",
        "開けると、SSR空母からランダムに1体入手する事ができます。（優先して無獲得のSSR空母から入手できます。）": "打开后可从SSR航母中随机获得1名。（优先获得尚未拥有的SSR航母。）",
        "【復刻版・カタバミと三つ葉のオークに見守られながら】イベントの報酬": "【复刻版·在酢浆草与三叶橡树的守望下】活动奖励",
        "復刻版・ブリキの騎士団の戦い-Tin·Can·Orderショップにて獲得できます。": "可在复刻版·锡制骑士团之战-Tin·Can·Order商店中获得。",
        "開けると、SSR巡洋艦からランダムに1体入手する事ができます。（優先して無獲得のSSR巡洋艦から入手できます。）": "打开后可从SSR巡洋舰中随机获得1名。（优先获得尚未拥有的SSR巡洋舰。）",
        "特定のミニキャラが包装された特別な箱、使用する事で、そのミニキャラを獲得する事ができる！": "包装着特定迷你角色的特殊箱子，使用后即可获得该迷你角色！",
        "ヴェネトお嬢様の奮闘伝": "维内托小姐的奋斗记",
        "蕾綻ぶ季節に": "在花蕾绽放的季节",
        "航空ダメージを20%アップさせる": "航空伤害提升20%",
        "魚雷ダメージを20%アップさせる": "鱼雷伤害提升20%",
        "K・G5世級姉妹スタンプ": "乔治五世级姐妹表情",
        "意気揚々": "意气风发",
        "くだらない": "无聊",
        "ビスマルクスタンプ大笑い": "俾斯麦表情·大笑",
        "イータスタンプティータイム": "伊塔表情·下午茶",
        "島風スタンプ微笑む": "岛风表情·微笑",
        "島風スタンプ2飛ぶ": "岛风表情2·飞翔",
        "照れ笑い": "害羞地笑",
        "どうしましたか": "怎么了？",
        "ボっとしてる": "发呆中",
        "ヘリオプロクス": "太阳神石",
        "グリムクォーツ": "格林石英",
        "クーケンスウェット": "库肯运动衫",
        "チェインベスト": "锁链背心",
        "8型ディーゼルサイクル改": "8型柴油循环·改",
        "エナジーペンダント": "能量吊坠",
        "焔雪の耳飾り": "焰雪耳饰",
        "退魔のブローチ": "退魔胸针",
        "グナーデリング ": "格纳德戒指 ",
        "雷嵐の耳飾り": "雷岚耳饰",
        "クォーツネックレス": "石英项链",
    }
    if value in exact_high_frequency:
        return exact_high_frequency[value]
    if "福袋" in value or "パック" in value or "セット" in value or "箱" in value:
        pack_terms = {
            "ムーバー": "搬运者", "ゼータ": "泽塔", "ミュー": "缪", "サンディエゴ": "圣地亚哥",
            "バンカー・ヒル": "邦克山", "鳥海": "鸟海", "摩耶": "摩耶", "時雨": "时雨",
            "鶴鷹": "鹤鹰", "伊勢": "伊势", "日向": "日向", "鳳舞": "凤舞",
            "夜空の隠れ里": "夜空的隐村", "の": "的", "超お得": "超值", "お得": "超值",
            "育成": "培养", "アイテム": "道具", "装備": "装备", "戦姫": "战姬", "歓迎": "欢迎",
            "誓い": "誓约", "誓約": "誓约", "巡洋艦": "巡洋舰", "戦艦": "战列舰",
            "駆逐艦": "驱逐舰", "改造": "改造", "セット": "套装", "ゴールド": "黄金",
            "シルバー": "白银", "ミラクル": "奇迹", "限定": "限定", "年末": "年末",
            "推薦状": "推荐信", "ラッキー": "幸运", "折返し記念": "中途纪念",
            "共鳴素材": "共鸣素材", "共鳴": "共鸣", "専属": "专属", "夏の日": "夏日",
            "新アイテム増量": "新道具增量", "激アツ": "超热血", "最高級": "最高级",
            "夏姫祭": "夏姬祭", "突破": "突破", "記念": "纪念", "福袋": "福袋",
            "パック": "礼包", "セット": "套装", "虹色チップ": "彩虹芯片",
            "ムーバゼータ": "搬运者泽塔", "ラムダ": "拉姆达", "ブルーチップ": "蓝色芯片",
            "聖誕": "圣诞", "ダイヤ": "钻石", "大破着せ替え": "大破换装", "新人向け": "新手",
            "コラボ": "联动", "ディヴェル": "迪维尔", "抱擁": "拥抱", "ガチャ": "抽卡",
            "お助け": "助力", "無料箱": "免费箱", "バレンタイン": "情人节", "推薦": "推荐",
            "ブリキの騎士団": "锡制骑士团", "冬姫祭": "冬姬祭", "超得": "超值",
            "超特注": "超特制", "重版": "再版", "聖誕": "圣诞",
            "ビスマルク": "俾斯麦", "周姫祭": "周姬祭", "姫羅": "姬罗", "バランス": "平衡",
            "装備コイン": "装备硬币", "BT": "BT", "シェフの暴走": "厨师暴走",
            "クリアする": "通关", "回": "次", "限定着せ替え": "限定换装",
            "コラボ記念": "联动纪念", "新人向け激レア装備": "新手超稀有装备", "サクラ": "樱",
            "月中限定": "月中限定", "ラボ": "实验室", "小吉": "小吉", "大吉": "大吉",
            "松": "松", "竹": "竹", "梅": "梅", "大艦隊レイドボス予備": "大舰队Raid Boss备用",
            "万象": "万象", "指揮官様": "指挥官大人", "朝日がきました": "朝日来了",
            "月明け": "月初", "ThreeDay": "三日", "祈願石": "祈愿石", "ぶらり提灯": "闲逛灯笼",
            "ファクター": "因子", "クリスマス": "圣诞节", "ムーバイータ": "搬运者伊塔",
            "許願石": "祈愿石", "上級": "高级", "赤銅": "红铜", "お正月": "新年",
            "戦姫": "战姬", "激レア": "超稀有",
            "指揮官様": "指挥官大人", "朝日がきました": "朝日来了", "ヴェネト": "维内托",
            "ボルチモア": "巴尔的摩", "イータ": "伊塔", "K・G 5世級": "乔治五世级",
            "秋シーズン": "秋季", "シーズン": "赛季", "着せ替え券": "换装券", "豪華": "豪华",
            "得々": "超值", "選択": "选择", "強化": "强化", "ハロウィン": "万圣节",
            "超大吉": "超大吉", "中吉": "中吉",
            "ランダム": "随机",
        }
        replaced_pack = value
        for source_term, target_term in sorted(pack_terms.items(), key=lambda item: -len(item[0])):
            replaced_pack = replaced_pack.replace(source_term, target_term)
        if replaced_pack != value and not JP_KANA_RE.search(replaced_pack):
            return replaced_pack
    label_terms = {
        "デイリ任務": "每日任务", "グルメミッション": "美食任务", "期間限定出現ガチャ": "限时出现卡池",
        "期間限定出現率ガチャ": "限时出现率卡池", "期間限定ガチャ": "限时卡池", "毎日": "每日",
        "戦域ショップ": "战域商店", "夜のやみ": "夜之暗", "雨宿り編": "避雨篇", "交換ショップ": "兑换商店",
        "共闘ショップ": "协同商店", "赤改造ショップ": "红色改造商店", "世界ショップ": "世界商店",
        "海域調査": "海域调查", "クリアする": "通关", "クリア報酬": "通关奖励", "受け取る": "领取",
        "装備": "装备", "改修": "改修", "達成する": "完成", "所有する": "拥有", "強化する": "强化",
        "到達する": "达到", "登録する": "登记", "変更する": "更换", "建造する": "建造",
        "タップする": "点击", "使う": "使用", "購入する": "购买", "獲得する": "获得",
        "好感度": "好感度", "誓いを交わす": "缔结誓约", "安全レベルになる": "达到安全等级",
        "任意の": "任意", "異なる": "不同的", "名": "名", "点": "件", "回": "次", "日": "天",
        "レベル": "等级", "デイリークエスト": "每日任务", "難易度": "难度", "戦友": "好友",
        "秘書艦": "秘书舰", "製油所": "炼油厂", "戦術": "战术", "風呂場": "浴场",
        "美食マスターの道": "美食大师之路", "ファラガット": "法拉格特", "クレムソン": "克莱姆森",
        "マハン": "马汉", "真・": "真·", "ムーバー防衛線": "搬运者防线", "海域": "海域",
        "戦姫": "战姬", "好感度": "好感度", "大艦隊任務": "大舰队任务", "スキル": "技能",
        "報酬": "奖励", "ダイヤ": "钻石", "燃料": "燃料",
    }
    if any(term in value for term in ("ガチャ", "ショップ", "クリアする", "到達する", "所有する", "達成する", "獲得する", "登録する", "建造する")):
        replaced_label = value
        for source_term, target_term in sorted(label_terms.items(), key=lambda item: -len(item[0])):
            replaced_label = replaced_label.replace(source_term, target_term)
        if replaced_label != value and not JP_KANA_RE.search(replaced_label):
            return replaced_label
    if value == "デイリ任務":
        return "每日任务"
    if value == "グルメミッション":
        return "美食任务"
    if value == "浮生夢の如し":
        return "浮生如梦"
    if value == "海鶴の宴ー期間限定ガチャ":
        return "海鹤之宴—限时卡池"
    if value == "ガチャ-【初の潜水艦】":
        return "卡池-【首艘潜艇】"
    if value == "毎日·EXモード":
        return "每日·EX模式"
    if value == "海域6-2 夜のやみ":
        return "海域6-2 夜之暗"
    if value == "愛宕＆高雄 雨宿り編":
        return "爱宕＆高雄 避雨篇"
    if value == "覚ませ!赤い悪夢!":
        return "醒来吧！赤色噩梦！"
    match = re.fullmatch(r"海域調査(\d+)-J地区をクリアする", value)
    if match:
        return f"完成海域调查{match.group(1)}-J地区"
    match = re.fullmatch(r"真・海域(\d+-\d+)をクリアする", value)
    if match:
        return f"通关真·海域{match.group(1)}"
    match = re.fullmatch(r"計(\d+)日ログインする", value)
    if match:
        return f"累计登录{match.group(1)}天"
    match = re.fullmatch(r"海域☆(\d+)クリア報酬を(\d+)回受け取る", value)
    if match:
        return f"领取海域☆{match.group(1)}通关奖励{match.group(2)}次"
    match = re.fullmatch(r"装備([６6]+)点を☆(\d+)まで改修する", value)
    if match:
        return f"将{match.group(1)}件装备改修至☆{match.group(2)}"
    match = re.fullmatch(r"大艦隊任務を(\d+)回達成する", value)
    if match:
        return f"完成大舰队任务{match.group(1)}次"
    match = re.fullmatch(r"ムーバー防衛線(.+)級をクリアする", value)
    if match:
        return f"通关搬运者防线{match.group(1)}级"
    match = re.fullmatch(r"真・海域(\d+-\d+)が安全レベルになる", value)
    if match:
        return f"真·海域{match.group(1)}达到安全等级"
    if value == "この着せ替えを購入する事で、解放する事ができます。":
        return "购买该换装后即可解锁。"
    if value == "ツェッペリン（博士？）":
        return "齐柏林（博士？）"
    if value == "減塩シオフキ":
        return "减盐盐吹"
    if value == "ムーバー·エリート":
        return "搬运者·精英"
    match = re.fullmatch(r"累計で燃料(\d+)を消費する", value)
    if match:
        return f"累计消耗燃料{match.group(1)}"
    match = re.fullmatch(r"累計でダイヤ(\d+)を消費する", value)
    if match:
        return f"累计消耗钻石{match.group(1)}"
    match = re.fullmatch(r"画面上部の資源購入\n（【\+】ボタン）から\n「(燃料|資源)」を(\d+)回購入する", value)
    if match:
        resource = "燃料" if match.group(1) == "燃料" else "资源"
        return f"从画面上方的资源购买\n（【+】按钮）购买{resource}{match.group(2)}次"
    match = re.fullmatch(r"研究課題([A-Z]\d+)がLv(\d+)に到達", value)
    if match:
        return f"研究课题{match.group(1)}达到Lv{match.group(2)}"
    match = re.fullmatch(r"本ラボの課題Lv総数は(\d+)まで到達", value)
    if match:
        return f"本实验室任务等级总数达到{match.group(1)}"
    if value == "海域：8-4をクリア":
        return "通关海域：8-4"
    if value == "共闘海域の戦闘を一回遂行する":
        return "完成一次协同海域战斗"
    if value == "冷たい声":
        return "冰冷的声音"
    if value == "臆病な少女":
        return "胆怯的少女"
    if value == "編成条件に満たない":
        return "不满足编队条件"
    if value == "終末のガーディアン":
        return "终焉守护者"
    match = re.fullmatch(r"砲撃\+(\d+)", value)
    if match:
        return f"炮击+{match.group(1)}"
    match = re.fullmatch(r"クリティカル率\+(\d+(?:\.\d+)?)%", value)
    if match:
        return f"暴击率+{match.group(1)}%"
    match = re.fullmatch(r"昼戦ダメージ\+(\d+(?:\.\d+)?)%", value)
    if match:
        return f"昼战伤害+{match.group(1)}%"
    if value == "翔鹤改":
        return "翔鹤·改"
    if value == "翔鹤改2":
        return "翔鹤·改2"
    if value == "<color=#f14949>ケルベロススウィートハニー？</color>":
        return "<color=#f14949>地狱三头犬甜心？</color>"
    if value == "<color=#f14949>プラチナのオオカミ姫？</color>":
        return "<color=#f14949>白金狼姬？</color>"
    if value == "うん。 ":
        return "嗯。 "
    if value == "アンノウンΜ":
        return "未知Μ"
    if value == "これを受け取って欲しい":
        return "希望你收下这个"
    if value == "指揮官様！朝日がきました！":
        return "指挥官大人！朝日来了！"
    if value == "イベント<size=18>ショップ</size>":
        return "活动<size=18>商店</size>"
    if value == "満耐久値で1回の試合をクリアする":
        return "以满耐久值完成1场战斗"
    match = re.fullmatch(r"合計(\d+)つのミッションをクリア", value)
    if match:
        return f"完成总计{match.group(1)}个任务"
    match = re.fullmatch(r"対艦\+(\d+)", value)
    if match:
        return f"对舰+{match.group(1)}"
    match = re.fullmatch(r"海域：戦艦毎日(\d+)をクリア", value)
    if match:
        return f"通关海域：战列舰每日{match.group(1)}"
    if value == "イベント物語-どこかでお会いしたことがありますでしょうか？":
        return "活动故事-我们是否曾在哪里见过？"
    if value == "イベント物語-ネコの目・2":
        return "活动故事-猫之眼·2"
    if value == "失敗は成功の母":
        return "失败是成功之母"
    match = re.fullmatch(r"(\d+)回ログインをする", value)
    if match:
        return f"登录{match.group(1)}次"
    if value == "1回戦姫探索をする":
        return "进行1次战姬探索"
    match = re.fullmatch(r"累計燃料x(\d+)を消費する", value)
    if match:
        return f"累计消耗燃料x{match.group(1)}"
    match = re.fullmatch(r"累計(\d+)回料理する", value)
    if match:
        return f"累计烹饪{match.group(1)}次"
    match = re.fullmatch(r"グルメ研究者(\d+)", value)
    if match:
        return f"美食研究者{match.group(1)}"
    match = re.fullmatch(r"【每日】(\d+)回リセットする", value)
    if match:
        return f"【每日】重置{match.group(1)}次"
    match = re.fullmatch(r"【每日】デイリー任務を(\d+)個達成する", value)
    if match:
        return f"【每日】完成{match.group(1)}个每日任务"
    match = re.fullmatch(r"【每日】累計で燃料(\d+)個を消費する", value)
    if match:
        return f"【每日】累计消耗燃料{match.group(1)}个"
    match = re.fullmatch(r"最高到達点を(\d+)回クリアする", value)
    if match:
        return f"完成最高到达点{match.group(1)}次"
    match = re.fullmatch(r"オースソーダを(\d+)回使う", value)
    if match:
        return f"使用奥斯苏打{match.group(1)}次"
    match = re.fullmatch(r"戦姫除隊を(\d+)回行う", value)
    if match:
        return f"进行战姬退役{match.group(1)}次"
    match = re.fullmatch(r"SSR「万能オース装備コア」を(\d+)回購入する", value)
    if match:
        return f"购买SSR“万能奥斯装备核心”{match.group(1)}次"
    match = re.fullmatch(r"基地にて高強度再生合金板を(\d+)回受け取る", value)
    if match:
        return f"在基地领取高强度再生合金板{match.group(1)}次"
    match = re.fullmatch(r"基地にてゲート接続維持器を(\d+)回受け取る", value)
    if match:
        return f"在基地领取网关连接维持器{match.group(1)}次"
    if value == "…なんだ。\n僕か？":
        return "……什么。\n我吗？"
    if value == "んじゃこれにしよう！":
        return "那就选这个吧！"
    if value == "だめだ。\nなんでそいつは名前のままで、僕は「異世界の」をつけなければならないんだ？":
        return "不行。\n为什么它可以保留原名，我却必须加上“异世界的”？"
    if value == "面倒くさいやつだな…":
        return "真是麻烦的家伙……"
    if value == "<color=#f14949>子分たち？</color>":
        return "<color=#f14949>小弟们？</color>"
    if value == "瑞鶴さま……":
        return "瑞鹤大人……"
    if value == "あなたは……":
        return "你是……"
    if value == "そして、 翌日——":
        return "然后，第二天——"
    if value == "『……バンカー・ヒル、 この小動物を麻袋に入れなさい。 \n 「ムーバー宛」 って書いてあるラベルを忘れずに。 』":
        return "『……邦克山，把这只小动物装进麻袋。\n别忘了贴上写着“寄给搬运者”的标签。』"
    if value == "お願いだからちょっと待ってください！ ":
        return "求求你，请稍等一下！ "
    if value == "——バン！":
        return "——砰！"
    if value == "じゃあ、私は…えっと。":
        return "那么，我……呃。"
    if value == "なんのご用でしょうか？":
        return "有什么事吗？"
    if value == "おおおおおおおお——！ ":
        return "哦哦哦哦哦哦哦哦——！ "
    if value == "あっ。 ":
        return "啊。 "
    if value == "お願いします。 ":
        return "拜托了。 "
    if value == "ごゆっくり〜":
        return "请慢慢来～"
    if value == "どうしたの、指揮官？":
        return "怎么了，指挥官？"
    if value == "なんだ？\nブルースフィアの。":
        return "什么？\n蓝星的？"
    if value == "…えっ！？":
        return "……诶！？"
    if value == "あら〜？朝日たちは最近大人気だね。\nモテすぎて困るわ〜":
        return "哎呀～？朝日最近很受欢迎呢。\n太受欢迎也让人困扰呢～"
    if value == "助けるって何を？":
        return "你说帮忙，具体帮什么？"
    if value == "あなたたちの推測通り、\n私たちは偵察のためにこっちの世界に派遣された。":
        return "正如你们推测的那样，\n我们是为了侦察而被派遣到这个世界的。"
    if value == "しかし、ゲートを通過してまもなく、私たちは赤い戦鬼…\nおそらくあなたたちとムーバーの諸君が言った「フェニー」に遭遇した。":
        return "但是通过网关后不久，我们就遇到了红色战鬼……\n恐怕就是你们和搬运者诸位所说的“菲妮”。"
    if value == "そっちも私たちの出現に驚いたでしょうね。圧倒的な火力を有しているにも関わらず、\n私たちを撃沈するより、生捕りにしたかったように見えたわ。":
        return "你们那边也被我们的出现吓到了吧。尽管拥有压倒性的火力，\n它看起来似乎比起击沉我们，更想把我们活捉。"
    if value == "だから私たちは散開して逃げることで、\n生き延びるチャンスを掴んだの。":
        return "所以我们分散逃跑，\n抓住了活下来的机会。"
    if value == "正直言うと、「冗談じゃない」と言いたくなるほどの火力だった。\nこちらの世界にはあのような恐ろしい存在があるなんて、予想外だったわ。":
        return "老实说，那火力强到让我想说“开什么玩笑”。\n没想到这个世界竟然存在那样可怕的东西。"
    if value == "その戦場から脱走する行為を嘲笑いたかったが…\n仕方もなかろう。":
        return "本想嘲笑你们从战场逃走的行为……\n但也情有可原。"
    if value == "ムーバーのものであっても、\nフェニーと遭遇して五体満足に生き延びた部隊は、数えるほどしかない。":
        return "即使是搬运者的部队，\n遇到菲妮后还能完好无损活下来的也屈指可数。"
    if value == "おお！慰めてくれてるの？\n意外と思いやりがあるじゃん〜！":
        return "哦！你是在安慰我吗？\n没想到你还挺体贴的嘛～！"
    if value == "はあ？\n…自惚れるな。":
        return "哈？\n……别自恋。"
    if value == "私たちは無事でいられたとは言え、\n補給物資だけでなく、通信設備もやむを得ず捨ててちゃいましてね。":
        return "虽然我们平安无事，\n但不得不把补给物资和通信设备都丢掉了。"
    if value == "今はまさに、遭難状態だわ～":
        return "我们现在正处于失事困境中～"
    if value == "…ちょっと待って、\nそれって…":
        return "……等一下，\n那是……"
    if value == "居候させてください！\nご飯食わしてください！！！":
        return "请让我们借住吧！\n请给我们饭吃！！！"
    if value == "おかわりだな？わかった！！":
        return "要再来一份吧？明白了！！"
    if value == "そうじゃないでしょう！って言うよりそう簡単に話しに乗るな！":
        return "不是这个意思吧！不对，别这么轻易顺着她的话说！"
    if value == "——え？でもどうせ、\n指揮官は困ってる子を追い出すようなことしないだろ？":
        return "——诶？不过反正，\n指挥官不会把遇到困难的孩子赶出去吧？"
    if value == "それはそうだけど——":
        return "话虽如此——"
    if value == "——ボルチモアさん、エセックスさん、それとサンディエゴさん。\nそろそろ、あなたたちがこの世界にいる理由を教えてもらえないかしら？":
        return "——巴尔的摩小姐、埃塞克斯小姐，还有圣地亚哥小姐。\n差不多可以告诉我你们来到这个世界的理由了吗？"
    if value == "いくらなんでも、私たちは「ブルースフィアの裏切り者」に変わらないから、\nそう簡単に彼女たちを受け入るわけないでしょう？…それに、現実的に考えてよ。":
        return "无论如何，我们不会变成“蓝星的叛徒”，\n所以不可能这么轻易接受她们吧？……而且，请现实一点。"
    if value == "もともと食べられる食料品を探すのが大変なのに、居候が3人も増えるなんて、\n資源確保組として、簡単に受け入れられないわよ。":
        return "本来寻找能吃的食物就很困难，一下子增加3个寄居者，\n作为资源保障组，我无法轻易接受。"
    if value == "そういう問題だったら、エリザベスさん、ご安心ください。\n私たちも全力で働きますから！":
        return "如果是这个问题，伊丽莎白小姐请放心。\n我们也会全力工作的！"
    if value == "食料収集でも衛生掃除でも、必ず全力で臨みますので、\nどうかここで働くチャンスをください！":
        return "无论是收集食物还是卫生清扫，我们都会全力以赴，\n请务必给我们在这里工作的机会！"
    if value == "実に素晴らしい意気込み…ってこういう問題じゃないって言ったでしょう！！":
        return "真是了不起的干劲……都说了问题不在这里吧！！"
    if value == "最近のエリザベスは心労が絶えないな。\n肌に悪いぞ？":
        return "伊丽莎白最近一直操心不断。\n这样对皮肤可不好哦？"
    if value == "加賀…！\nあなたもね…":
        return "加贺……！\n你也是……"
    if value == "まあまあ、落ち着いてって〜":
        return "好啦好啦，冷静一点～"
    if value == "——彼女たちを受け入れるリスクを否定しないけど、\n現状を打破するチャンスでもあるんじゃない？":
        return "——我不否认接纳她们存在风险，\n但这不也是打破现状的机会吗？"
    if value == "ムーバーの3人とは近いうちにさようならするだろ？\nそうしたら、あたいたちはまた元の状態に戻ってしまう。":
        return "我们迟早会和搬运者的三个人告别吧？\n那样的话，我们又会回到原来的状态。"
    if value == "まあ、あんな生活も悪くないけど…\nあたいたちもいつまでも、引き篭もったままにはいかないだろう？":
        return "嘛，那种生活也不坏……\n但我们也不能永远闭门不出吧？"
    if value == "…あなたにしては、珍しく真っ当な意見を出したわね。":
        return "……对你来说，还真是难得提出了正经意见。"
    if value == "わかったわ。\nそれじゃ、決断はやはり指揮官に任せましょう。":
        return "知道了。\n那么，决定权还是交给指挥官吧。"
    if value == "——この3人を受け入れるかどうかを。":
        return "——是否接受这三个人。"
    if value == "こっちのルールに従わってもらうけど":
        return "不过你们要遵守这里的规则。"
    if value == "指揮官〜まさか可憐な美少女を見捨てるはずないよね？\n絶対精一杯に働くから〜":
        return "指挥官～你不会丢下可爱的美少女不管吧？\n我一定会拼命工作的～"
    if value == "わかった。ここにいていいぞ":
        return "知道了。你们可以留在这里。"
    if value == "おお！？本当に！？\nやった～！さすがはオークランドの指揮官！":
        return "哦！？真的吗！？\n太好了～不愧是奥克兰的指挥官！"
    if value == "誠にありがとうございます。\nふふ、短い間と思いますが…仲良くしましょうね。":
        return "真的非常感谢您。\n呵呵，虽然相处时间可能不长……请和我们好好相处。"
    if value == "そんな直接に褒められると…なんだか恥ずかしいな":
        return "被这样直接夸奖……总觉得有点害羞。"
    if value == "まさしく善良でありながら、器量と識見が群を抜いているお方ですね！\nエリザベスさんたちがあなたについていく理由、今ならわかったかもしれません。":
        return "您真是一位善良、才干与见识都出类拔萃的人！\n现在我或许明白伊丽莎白小姐她们为何追随您了。"
    if value == "褒めてくれてありがとう":
        return "谢谢你的夸奖"
    if value == "そんな体で、無理しないで！":
        return "都这样了，别勉强自己！"
    if value == "——フッドさん。":
        return "——胡德小姐。"
    if value == "…だからあの時、何も言わなかったのですか。":
        return "……所以你那时才什么都没说吗。"
    if value == "遠慮しないでください！\n良い人間関係は、お互いの長所を高く評価することから築いたものだと思います。":
        return "请不要客气！\n我认为良好的人际关系，是从高度认可彼此的优点建立起来的。"
    if value == "指揮官様も、どんどんわたくしのことを褒めてくださいね。":
        return "指挥官大人也请多多夸奖我哦。"
    if value == "俺は…そんな風に言われてるのか！？":
        return "我……被这样评价的吗！？"
    if value == "…実際、上層部の態度も微妙だし…\n本当にそうとも思えないね。":
        return "……实际上，上层的态度也很微妙……\n我也不觉得真是这样。"
    if value == "なぜ違うと思う？":
        return "为什么觉得不是？"
    if value == "こんなことになるとわかっていたら、いくら資源がなくても、\n朝日に…もっと弾薬を…支給してもらえれば、よかった…":
        return "如果早知道会变成这样，就算资源再短缺，\n也该让朝日……多发一些弹药……"
    if value == "…モーリエに関する手がかりは、まだ何にも…":
        return "……关于莫利的线索，还是一点都没有……"
    if value == "しかし、このまま何もしないわけにはいきません。\n候補場所を一つ一つしらみつぶしに調べるしかないでしょう。":
        return "但是不能这样什么都不做。\n只能逐一彻底调查候选地点了。"
    if value == "ふふ、こんな程度で照れちゃったなんて、だめですよ。":
        return "呵呵，只是这种程度就害羞了，可不行哦。"
    if value == "しばらくの間はこちらにお世話になりますから、\n指揮官様は早く慣れてくださいね。":
        return "接下来一段时间要承蒙这里照顾了，\n指挥官大人也请快点习惯我们哦。"
    if value == "潜入成功なの。":
        return "潜入成功。"
    if value == "ブルースフィアのワルモノたち…\nラフィーがまとめてやっつけるの！":
        return "蓝星的坏蛋们……\n拉菲会把你们一口气打倒！"
    if value == "…大変そうだな。":
        return "……看起来很辛苦啊。"
    if value == "M・A・O":
        return "M·A·O"
    match = re.fullmatch(r"レベル上限\+(\d+)", value)
    if match:
        return f"等级上限+{match.group(1)}"
    match = re.fullmatch(r"主砲ダメージ\+(\d+(?:\.\d+)?)%", value)
    if match:
        return f"主炮伤害+{match.group(1)}%"
    match = re.fullmatch(r"被ダメージ減少\+(\d+(?:\.\d+)?)%", value)
    if match:
        return f"受到伤害减少+{match.group(1)}%"
    match = re.fullmatch(r"ステルスコーティング材を(\d+)を獲得する", value)
    if match:
        return f"获得隐形涂层材料{match.group(1)}"
    if value == "生徒たち":
        return "学生们"
    if value == "ふん。 ":
        return "哼。 "
    if value == "ブルースフィアのオークランド":
        return "蓝星的奥克兰"
    if value == "ムーバーのオークランド":
        return "搬运者的奥克兰"
    if value == "悲しい少女":
        return "悲伤的少女"
    if value == "運得万能オースパック（限定品）":
        return "运得万能奥斯礼包（限定品）"
    if value == "ハロウィン福袋":
        return "万圣节福袋"
    if value == "とにかく、今のところ、\nあなたたちと刃を交えるつもりはないわ。":
        return "总之，至少目前为止，\n我没有和你们刀刃相向的打算。"
    if value == "むしろ——もしできることなら、\n助けてもらいたいとも考えているの。":
        return "倒不如说——如果可以的话，\n我还想请你们帮忙。"
    if value == "あっ——":
        return "啊——"
    if value == "これを受け取って欲しい":
        return "希望你收下这个"
    if value == "魚雷の搭載数+1。特殊能力獲得：正常さ！ Lv.2":
        return "鱼雷搭载数+1。获得特殊能力：正常！Lv.2"
    if value == "魚雷ダメージ+20%。特殊能力獲得：正常さ！ Lv.3":
        return "鱼雷伤害+20%。获得特殊能力：正常！Lv.3"
    skill_names = {
        "人を励ます": "鼓励他人", "泣き虫のお姉さん": "爱哭的大姐姐", "激射！": "猛烈射击！",
        "自由奔放": "自由奔放", "正常さ！": "正常！", "夢歩き": "梦中漫步",
        "ヴィットリオ・ヴェネト！": "维托里奥·维内托！", "幻の黒猫": "幻之黑猫",
        "格好付け": "装酷", "戦闘のプロ": "战斗专家", "みんなの名前": "大家的名字",
    }
    match = re.fullmatch(r"「主砲攻撃」の時、一定の確率で「Cutin」が発動する。特殊能力獲得：(.+) Lv\.(\d+)", value)
    if match:
        name = skill_names.get(match.group(1), match.group(1))
        return f"进行“主炮攻击”时，有一定概率触发“Cutin”。获得特殊能力：{name} Lv.{match.group(2)}"
    match = re.fullmatch(r"副砲ダメージ\+(\d+)%。特殊能力獲得：(.+) Lv\.(\d+)", value)
    if match:
        name = skill_names.get(match.group(2), match.group(2))
        return f"副炮伤害+{match.group(1)}%。获得特殊能力：{name} Lv.{match.group(3)}"
    match = re.fullmatch(r"魚雷ダメージ\+(\d+)%。特殊能力獲得：(.+) Lv\.(\d+)", value)
    if match:
        name = skill_names.get(match.group(2), match.group(2))
        return f"鱼雷伤害+{match.group(1)}%。获得特殊能力：{name} Lv.{match.group(3)}"
    match = re.fullmatch(r"「夜戦」でのダメージ\+(\d+)%。特殊能力獲得：(.+) Lv\.(\d+)", value)
    if match:
        name = skill_names.get(match.group(2), match.group(2))
        return f"“夜战”中的伤害+{match.group(1)}%。获得特殊能力：{name} Lv.{match.group(3)}"
    match = re.fullmatch(r"「昼戦」での主砲ダメージ\+(\d+)%。特殊能力獲得：(.+) Lv\.(\d+)", value)
    if match:
        name = skill_names.get(match.group(2), match.group(2))
        return f"“昼战”中的主炮伤害+{match.group(1)}%。获得特殊能力：{name} Lv.{match.group(3)}"
    match = re.fullmatch(r"「戦闘開始」時に開幕魚雷攻撃ができる。開幕魚雷攻撃のダメージは通常魚雷攻撃の(\d+)%に相当する。魚雷を消費しない。特殊能力獲得：(.+) Lv\.(\d+)", value)
    if match:
        name = skill_names.get(match.group(2), match.group(2))
        return f"战斗开始时可进行开幕鱼雷攻击。开幕鱼雷攻击伤害相当于普通鱼雷攻击的{match.group(1)}%。不消耗鱼雷。获得特殊能力：{name} Lv.{match.group(3)}"
    match = re.fullmatch(r"シェフの暴走-(EASY|NORMAL|HARD)（チーム）を(\d+)回クリアする", value)
    if match:
        return f"完成“厨师暴走”-{match.group(1)}（队伍）{match.group(2)}次"
    if value == "「主砲攻撃」の時、一定の確率で「Cutin」が発動する。特殊能力獲得：泣き虫のお姉さん Lv.2\n<color=#E34174>【黒炎降臨】を習得し、ターゲット周辺区域の敵艦に一定のダメージを与える。（冷却時間45秒）</color>":
        return "进行“主炮攻击”时，有一定概率触发“Cutin”。获得特殊能力：爱哭的大姐姐 Lv.2\n<color=#E34174>习得【黑炎降临】，对目标周边区域的敌舰造成固定伤害。（冷却时间45秒）</color>"
    if value == "「昼戦」での主砲ダメージ+10%。特殊能力獲得：泣き虫のお姉さん Lv.3\n<color=#E34174>【黒炎降臨】が【無限黒炎降臨】に進化し、新たな姿となる。敵艦に与えるダメージが上昇する。</color>":
        return "“昼战”中的主炮伤害+10%。获得特殊能力：爱哭的大姐姐 Lv.3\n<color=#E34174>【黑炎降临】进化为【无限黑炎降临】，呈现全新形态。对敌舰造成的伤害提高。</color>"
    if value == "<color=#E34174>【無限黒炎降臨】の敵艦に与えるダメージが更に上昇し、同時に耐久の最も高い敵艦に【黒炎】効果を付与する。ダメージ終了後、味方全体にダメージ15％上昇の祝福効果を付与する（持続時間10秒）</color>":
        return "<color=#E34174>【无限黑炎降临】对敌舰造成的伤害进一步提高，同时对耐久值最高的敌舰施加【黑炎】效果。伤害结束后，为我方全体施加伤害提升15%的祝福效果（持续10秒）</color>"
    if value == "「夜戦」でのダメージ+20%。\n<color=#be309e>【ダウンバースト】のダメージは時間とともに徐々に上がっていって、最も高い倍率は3になる。【ダウンバースト】が発動後に、付加倍率は0になる</color>特殊能力獲得：みんなの名前 Lv.3":
        return "“夜战”中的伤害+20%。\n<color=#be309e>【下击暴流】的伤害会随时间逐渐提高，最高倍率为3。【下击暴流】发动后，附加倍率归零。</color>获得特殊能力：大家的名字 Lv.3"
    if value == "<color=#be309e>味方は魚雷攻撃を発動する度に、一定の確率で【ダウンバースト】仕様の攻撃を追加できる。その攻撃は時間とともに徐々に上がっていかないし、倍率に影響もない。</color>":
        return "<color=#be309e>我方每次发动鱼雷攻击，都有一定概率追加一次【下击暴流】规格的攻击。该攻击不会随时间逐渐增强，也不受倍率影响。</color>"
    if value == "魚雷射出から命中までの間に主砲による射撃を行った場合、魚雷命中時に「砲雷混合攻撃」効果を発動する：自艦が射出した魚雷が命中するときに、自動的に1回の砲撃を行う。この砲撃は艦砲の射程に影響せず、発射後装填する必要がない、自艦の位置問わずT字有利とみなし、通常砲撃の300%のダメージを与え、且つ必ず混合攻撃の最大ダメージ補正を得る。この効果は1度の戦闘で1回しか発動しない。特殊能力獲得：自由奔放 Lv.4":
        return "在鱼雷发射至命中期间进行主炮射击时，鱼雷命中时发动“炮雷混合攻击”效果：自身发射的鱼雷命中时自动进行1次炮击。该炮击不受舰炮射程影响，发射后无需装填；无论自身位置如何均视为T字有利，造成普通炮击300%的伤害，并必定获得混合攻击的最大伤害修正。该效果单场战斗只能发动1次。获得特殊能力：自由奔放 Lv.4"
    if value == "「昼戦」での主砲ダメージ+10%。\n<color=#be309e>【叫喚地獄】は【灼熱地獄】に進化して：\nホットメルト爆弾は3回投射でき、3回目のダメージが更に高くなる。</color>特殊能力獲得：夢歩き Lv.3":
        return "“昼战”中的主炮伤害+10%。\n<color=#be309e>【叫唤地狱】进化为【灼热地狱】：\n热熔炸弹可投掷3次，第3次伤害进一步提高。</color>获得特殊能力：梦中漫步 Lv.3"
    if value == "<color=#be309e>【灼熱地獄】は【阿鼻地獄】に進化して：\n4回目の最終攻撃を追加し、目標エリアに集中的な全力砲撃を行う。4回目のダメージは前の3回より高い。</color>":
        return "<color=#be309e>【灼热地狱】进化为【阿鼻地狱】：\n追加第4次最终攻击，对目标区域进行集中全力炮击。第4次伤害高于前3次。</color>"
    if value == "爆撃機、魚雷機は敵の対空砲火より先に攻撃を行う。\n<color=#be309e>【突襲行動】を習得し、ムーバーゼータは戦場にいくつかのビーコンを降下させ、ビーコンをトリガーすることで複数の戦闘機を召喚して敵に突襲する。(戦闘内に1回のみ発動可能)</color>特殊能力獲得：戦闘のプロ Lv.2":
        return "轰炸机、鱼雷机先于敌方防空炮火发动攻击。\n<color=#be309e>习得【突袭行动】，搬运者泽塔向战场投放数个信标，触发信标后召唤多架战斗机突袭敌人。（单场战斗只能发动1次）</color>获得特殊能力：战斗专家 Lv.2"
    if value == "「出撃」する時、各タイプの予備機数+50 \n<color=#be309e>【突襲行動】が【戦争指令】に進化する：\nビーコンが落下する以後、ムーバーゼータは暴動輝光戰機を戦場に突っ込み、連続攻撃を行う。</color>特殊能力獲得：戦闘のプロ Lv.3":
        return "进行“出击”时，各类型备用机数量+50\n<color=#be309e>【突袭行动】进化为【战争指令】：\n信标落下后，搬运者泽塔将暴动辉光战机投入战场，进行连续攻击。</color>获得特殊能力：战斗专家 Lv.3"
    if value == "<color=#be309e>【戦争指令】が【殲滅通告】に進化する：\n戦場に投下されるビーコンの数が増える。ビーコンをトリガーすることで、一定確率でバリアと耐久性を回復し続ける効果が得られる。\n暴動輝光戰機は連続攻撃回数が増え、敵艦に与えるダメージがさらに上昇、発射速度がさらに速くなる。</color>":
        return "<color=#be309e>【战争指令】进化为【歼灭通告】：\n投放到战场的信标数量增加。触发信标后，有一定概率获得持续恢复护盾与耐久值的效果。\n暴动辉光战机的连续攻击次数增加，对敌舰造成的伤害进一步提高，发射速度也进一步加快。</color>"
    if value == "「主砲攻撃」の時、一定の確率で「Cutin」が発動する。\n<color=#be309e>スキル【叫喚地獄】を獲得，目標がいるエリアにホットメルト爆弾を投射し、マルチステップダメージを与える。1度の戦闘で2回のみ発動可能、装填時間は4秒、2回目のダメージは1回目より高くなる。</color>特殊能力獲得：夢歩き Lv.2":
        return "进行“主炮攻击”时，有一定概率触发“Cutin”。\n<color=#be309e>获得技能【叫唤地狱】，向目标所在区域投掷热熔炸弹，造成多段伤害。单场战斗只能发动2次，装填时间4秒，第2次伤害高于第1次。</color>获得特殊能力：梦中漫步 Lv.2"
    if value == "水中速力+30%特殊能力獲得：リーダーシップ Lv.2":
        return "水下航速+30%。获得特殊能力：领导力 Lv.2"
    if value == "水中魚雷ダメージ+20%。特殊能力獲得：リーダーシップ Lv.3":
        return "水下鱼雷伤害+20%。获得特殊能力：领导力 Lv.3"
    if value == "爆撃機、魚雷機は敵の対空砲火より先に攻撃を行う。特殊能力獲得：海の力 Lv.2":
        return "轰炸机、鱼雷机先于敌方防空炮火发动攻击。获得特殊能力：海洋之力 Lv.2"
    if value == "「出撃」する時、各タイプの予備機数+50特殊能力獲得：海の力 Lv.3":
        return "进行“出击”时，各类型备用机数量+50。获得特殊能力：海洋之力 Lv.3"
    if value == "「主砲攻撃」の時、一定の確率で「Cutin」が発動する。\n<color=#be309e>【ダウンバースト】を獲得：特殊魚雷攻撃を大量に発動し、敵との距離が近ければ近いほど、命中できる数が多くなる。</color>特殊能力獲得：みんなの名前 Lv.2":
        return "进行“主炮攻击”时，有一定概率触发“Cutin”。\n<color=#be309e>获得【下击暴流】：发动大量特殊鱼雷攻击，距离敌人越近，能够命中的数量越多。</color>获得特殊能力：大家的名字 Lv.2"
    if value.startswith("激射〜☆自由奔放な少女。"):
        return value.replace(
            "激射〜☆自由奔放な少女。とにかく大砲が好きで、「激射」と自分が名付けたバカな弾薬浪費行為を広く布教している。息するように問題発言を繰り返しているが、彼女はそういうわがままに釣り合うほどの実力の持ち主である。日常でも戦場でも人々の注目を集めている。",
            "猛烈射击～☆自由奔放的少女。她非常喜欢大炮，广泛宣传自己命名为“猛烈射击”的愚蠢弹药浪费行为。她像呼吸一样不断说出问题发言，但实力足以配得上这份任性。无论日常还是战场，都吸引着众人的目光。",
        )
    if value == "・【戦闘開始時】、砲火または魚雷を<color=#417AE3><1></color>発発射するごとに、<2>の確率で1発の魚雷が追加される。1つの戦闘で2回までアクティブにすることができる。\n・（駆逐艦、巡洋艦にのみ有効）すべての魚雷が使用済の場合、自身の砲撃ダメージ＋<3>。":
        return "・【战斗开始时】，每发射<color=#417AE3><1></color>发炮弹或鱼雷，就有<2>概率追加1发鱼雷。单场战斗最多激活2次。\n・（仅对驱逐舰、巡洋舰有效）所有鱼雷使用完毕时，自身炮击伤害＋<3>。"
    if value == "敵が魚雷攻撃を受けるダメージ+<1>、戦艦の敵にこの効果更に+<2>、上記の効果は全て重複しない。":
        return "敌人受到鱼雷攻击的伤害+<1>，对战列舰敌人该效果额外+<2>，上述效果均不可叠加。"
    if value == "自身がターゲットに選ばれる比率－<1>、耐久値が80%%以下の場合、一秒ごとに<2>の耐久値を回復、最大10回まで発動できる。敵が攻撃を発動する度に、自身の砲撃値+<3>、最大10回まで発動できる。":
        return "自身被选为目标的概率－<1>，耐久值低于80%%时每秒恢复<2>耐久值，最多触发10次。每当敌人发动攻击，自身炮击值+<3>，最多触发10次。"
    if value == "·【戦闘開始時】、自身の魚雷数+<1>、味方戦姫全員の魚雷ダメージ+<2>":
        return "·【战斗开始时】，自身鱼雷数+<1>，我方全体战姬鱼雷伤害+<2>"
    if value == "・【主砲攻撃後】、クリティカルダメージ+<1>、この効果は最大10回まで重複する":
        return "・【主炮攻击后】，暴击伤害+<1>，该效果最多叠加10次"
    if value == "味方の空母爆撃機によるダメージが<1>アップ、この効果は重複しない。":
        return "我方航空母舰轰炸机造成的伤害提升<1>，该效果不可叠加。"
    if value == "・本海域内で味方の特定キャラクターをチームに編入することで、特殊な支援効果を得ることできます。":
        return "・在本海域内将我方指定角色编入队伍，即可获得特殊支援效果。"
    match = re.fullmatch(r"昼戦被ダメージ減少\+(\d+(?:\.\d+)?)%", value)
    if match:
        return f"昼战受到伤害减少+{match.group(1)}%"
    if value == "戸田めぐみ":
        return "户田惠"
    if value == "·（空母のみ有効）【航空攻撃後】5秒毎に【空母の怒り】効果を発動する、最も耐久値の低い敵艦に【火力】x<1>のダメージを与える。この効果は、自身の【航空攻撃】毎に最大4回まで発動する事ができる。（索敵中の航空攻撃は対象外） ":
        return "·（仅对航空母舰有效）【航空攻击后】每5秒触发一次【航母之怒】效果，对耐久值最低的敌舰造成【火力】x<1>伤害。该效果每次自身【航空攻击】最多触发4次。（索敌中的航空攻击除外） "
    if value == "·【戦闘中】、敵の旗艦が受けるダメージ+<1>、自身のクリティカルダメージ+<2>、上記の効果は重複しない。":
        return "·【战斗中】，敌方旗舰受到的伤害+<1>，自身暴击伤害+<2>，上述效果不可叠加。"
    if value == "· 主砲攻撃は最も耐久値の低い敵艦を優先する。主砲攻撃で敵艦を撃沈した場合、直ちに次の主砲1発を装填する(CD<color=#417AE3><1></color>秒)":
        return "· 主炮攻击优先选择耐久值最低的敌舰。主炮攻击击沉敌舰时，立即装填下一发主炮（CD<color=#417AE3><1></color>秒）"
    if value == "·【戦闘開始時】、自身の魚雷数+2、味方戦姫全員のダメージ+<1>、【夜戦突入時】、味方戦姫全員の魚雷ダメージ+<2>":
        return "·【战斗开始时】，自身鱼雷数+2，我方全体战姬伤害+<1>；【进入夜战时】，我方全体战姬鱼雷伤害+<2>"
    if value == "·【戦闘中】、味方全体のダメージ+<1>、【近距離】に進入する際に、味方駆逐艦全体のダメージ+<2>、上記の効果は重複しない。":
        return "·【战斗中】，我方全体伤害+<1>；进入【近距离】时，我方全体驱逐舰伤害+<2>，上述效果不可叠加。"
    if value.startswith("【赤の焚図】を獲得："):
        return "获得【赤之焚图】：召唤力量之剑对敌人造成斩击，可重复发动。（在一定时间内未发动技能时，下次技能伤害会随时间增加）斩击的形状与性能会因和敌人的距离、我方舰队船速差等因素而不同。战斗开始30秒后，再次发动【赤之焚图】时，会释放强力攻击，对大范围敌人造成伤害。"
    if value == "無気力な女性は、怠け者ではないが陸地では元気が出ず、まともな仕事ができない。でも海水に触れただけで戦意が爆発するし、その威圧感は、敏感な仲間の中には、視界から遠ざかるのではないかと恐れる者もいた。":
        return "无精打采的女性并不是懒惰，只是在陆地上提不起精神，无法正常工作。但只要接触海水，战意就会爆发；她的压迫感甚至让敏感的伙伴担心会被赶出视线。"
    if value == "背伸びしたい少女。難易度の高いことばかり選んだため失敗に終わることが多い。しかし、失敗しても前向きな姿勢を貫き、頑張ることをやめない彼女なので、笑顔でその成長を楽しみにしている年配の方も多い。":
        return "想要逞强的少女。因为总是选择高难度的事情，所以经常以失败告终。但她即使失败也始终保持积极态度，从不停止努力，因此有许多年长者都在笑着期待她的成长。"
    if value == "愉快犯の少女は、人前では優等生のままで、陰では他人の喜怒哀楽を鑑賞の目で楽しむ。情動指数や知能指数が高く、悪をすれば必ず現実と人間関係の二重の災いを引き起こすが、「人間大好き」の彼女が悪役を担当することはない——それをどう解釈するかは状況によるけれど。":
        return "恶作剧少女在人前始终是优等生，私下却以欣赏的目光享受他人的喜怒哀乐。她情感指数和智力指数都很高，作恶必然会引发现实与人际关系的双重灾难，但这个“最喜欢人类”的少女不会担任反派——至于如何解读，要看具体情况。"
    if value == "穏やかで優雅なお嬢様。人と接する態度が完璧である一方、実戦経験はゼロで、戦闘に関する知識もまったく備えていないらしい。オスカーという名の黒猫を飼っている彼女だが、部屋では飼育道具など一切存在せず、そのネコを目撃した人間もごく僅かという。":
        return "温和优雅的大小姐。她待人接物的态度无可挑剔，但似乎完全没有实战经验，也不具备任何战斗知识。她养着一只名叫奥斯卡的黑猫，可房间里完全没有饲养用品，见过那只猫的人也寥寥无几。"
    if value == "しっかりしている女の子は、わがままが少なく、大人の手伝いをしっかりしています。自分の所属しているグループが主人公側とは停戦したことは理解しているが、主人公側のイメージは「悪い人」から「そうでない人」に変わっただけで、特に問題の人物に対しては常に警戒心を持っている。":
        return "稳重的女孩，很少任性，会认真帮助大人。她知道自己所属的团体已经与主角方停战，但对主角方的印象只是从“坏人”变成了“并非坏人”；尤其面对问题人物时，她始终保持警惕。"
    if value == "自信満々なお姉さん。いつも当たり前のように指揮官の仕事について指導したがっているが、ちゃんとそれに見合う実力を持っている。しかし、彼女にはたった一つ致命的な問題がある。それは、自分の部屋に帰ると、本人ですら驚くほどのダメ人間に化することである。":
        return "自信满满的大姐姐。她总是理所当然地想指导指挥官的工作，而且确实拥有与之相称的实力。但她有一个致命问题：一回到自己的房间，就会变成连本人都惊讶的废柴。"
    if value == "警戒心の強い女性。任務関連の事項を繰り返してその確実性を検証する癖がある。とある極地訓練で遭難しかけたため、今のような疑り深い性格になってしまった。独自の情報ルートを持っている。あまり団体活動に参加しない彼女だが、基地内のことならことごとく通暁している。":
        return "警惕心很强的女性。她习惯反复确认任务相关事项的可靠性。由于曾在某次极地训练中险些遇难，才变成如今多疑的性格。她拥有独自的信息渠道。虽然不太参加集体活动，却对基地内的一切都了如指掌。"
    if value == "ムーバーからの支援戦力。もともと繊細な性格のため、慣れない環境に来たことでまだ何かを警戒している模様。 しかしあまり自分の意見を表明しない理由は人見知りではなく、元々個別な問題を除いてほとんど自己主張しないからである。最近幻聴が軽減したようだが、たまに再発する。":
        return "来自搬运者的支援战力。她本来性格纤细，来到陌生环境后似乎仍在警戒什么。但她不太表达意见并不是因为怕生，而是除个人问题外本来就几乎不主张自我。最近幻听似乎减轻了，但偶尔还会复发。"
    if value == "かなり困らせたマイペースな少女で、よくぶらぶらしていて誰も見つからない。基本的には勝手に基地を占拠して——ええと、部屋を「徴用」しましたが、気分次第では難しい戦局に急に出てきて、しかも確かに強いので、みんなもあまり何も言えません。":
        return "相当令人困扰的我行我素少女，经常到处闲逛，谁也找不到她。她基本上擅自占据基地——呃，是“征用”了房间，但有时会随心所欲地突然出现在艰难战局中，而且确实很强，所以大家也不太敢说什么。"
    if value == "異世界から来た少女、錬金術マニアで冒険と材料採取にも夢中。ハムスターみたいに大量な材料を備蓄し、持ち歩く習慣があり、「そのカゴは四次元ポケットか！」とつっこまれたことがある。一番好きな錬金術道具は爆弾の疑いがある。":
        return "来自异世界的少女，沉迷炼金术，也热衷冒险和采集材料。她像仓鼠一样储存并随身携带大量材料，曾被吐槽“那个篮子是四次元口袋吗！”她最喜欢的炼金术道具，疑似是炸弹。"
    if value == "自分こそが家族の長女だと上から目線で人に教えている傲慢なお嬢様だが、実は喧嘩が下手で、へこんだ時はお嬢様のイメージを維持するのに必死で涙をこらえてなるべく黙っている。戦闘中は真面目、実にスマートで頼もしい。":
        return "傲慢的大小姐，总是居高临下地告诉别人自己才是家中的长女，但其实不擅长争吵。受挫时为了维持大小姐形象，会拼命忍住眼泪，尽量保持沉默。战斗中认真、干练而可靠。"
    if value == "明るく活発な少女で、いつもエネルギッシュに動き回っている。仕事を頼まれたら喜んでやりますが、放っておくとすぐに勝手に何かを探してきて、大抵は大きなトラブルを起こして、最後には泣きながら助けを求めに戻ってきます。":
        return "开朗活泼的少女，总是精力充沛地四处活动。有人拜托工作时会高兴地完成，但一放任她就会擅自寻找什么东西，通常引发大麻烦，最后哭着回来求助。"
    if value == "内気な少女。人付き合いは苦手だが、仲間になれるように地道に努力している。特技を聞かれると「少し足が速い」と曖昧に答えるが、他の証言では確かに少しだけ速かったので、あまり注目されなかった。":
        return "内向的的少女。不擅长与人交往，但一直踏实努力，希望成为伙伴。被问到特长时含糊地回答“跑得稍微快一点”，但其他证词也确实证明她只快了一点，所以没受到太多关注。"
    if value == "無愛想な女性が、時に激しく怒り出すことがある。どうしても達成しなければならない目標はあるらしいが、外部には何も言わない。基地では基本的に一人歩きをしており、集団活動には参加しない。":
        return "不苟言笑的女性，有时会突然大发雷霆。她似乎有无论如何都必须达成的目标，却从不对外透露。在基地里基本独自行走，也不参加集体活动。"
    match = re.fullmatch(r"夜戦被ダメージ減少\+(\d+(?:\.\d+)?)%", value)
    if match:
        return f"夜战受到伤害减少+{match.group(1)}%"
    if value == "基地にて高強度再生合金板を100回受け取る":
        return "在基地领取高强度再生合金板100次"
    if value == "別に。 ":
        return "没什么。 "
    if value == "えっ？ ":
        return "诶？ "
    if value == "…そうですね。":
        return "……是啊。"
    if value == "大笑いしている女の子":
        return "放声大笑的女孩"
    if value == "……え？ ":
        return "……诶？ "
    if value == "狂暴化した客":
        return "狂暴化的客人"
    if value == "リットリオ":
        return "利托里奥"
    if value == "優しそうなお姉さん":
        return "看起来很温柔的大姐姐"
    if value == "やあっ！？":
        return "呀！？"
    if value == "どう見ても怪しい人":
        return "怎么看都很可疑的人"
    if value == "ありがとう！":
        return "谢谢！"
    if value == "限定福袋を購入する事で、入手する事ができます。":
        return "购买限定福袋后即可获得。"
    if value == "【復刻版】雪月風花限定イベントにて獲得できるicon":
        return "可在【复刻版】雪月风花限定活动中获得的头像"
    if value == "·艦隊に駆逐艦が2名いる場合、自分の雷装ダメージ+<1>、2回目の共鳴突破をした後、駆逐艦が1隻増えるごとに、雷装ダメージが更に+<2>\n艦隊に軽巡洋艦が2名いる場合、自分の砲撃ダメージ+<3>、2回目の共鳴突破をした後、軽巡洋艦が1隻増えるごとに、砲撃ダメージが更に+<4>":
        return "·舰队中有2名驱逐舰时，自身雷装伤害+<1>；完成第2次共鸣突破后，每增加1艘驱逐舰，雷装伤害额外+<2>\n舰队中有2名轻巡洋舰时，自身炮击伤害+<3>；完成第2次共鸣突破后，每增加1艘轻巡洋舰，炮击伤害额外+<4>"
    if value == "·【遠距離以外で】いる時間が累計15秒に到達した場合、敵が強力な火力支援を要請し、味方に大きなダメージを与える。":
        return "·在【非远距离】状态累计达到15秒时，敌人会请求强力火力支援，对我方造成大量伤害。"
    if value == "・敵が受ける魚雷のダメージを50%ダウンする。\n\n<color=#3A3A3A>おすすめの対応：</color>\nこの抵抗性の影響を受けない攻撃方法を用いるか：\n<color=#2F8DE9>【魚雷戦術の最適化.広域】:\n:ル・テリブル、ジョンストン、サンディエゴ、トレント</color>\n敵全体の魚雷防御を15%下げて，重複できません。":
        return "・敌人受到的鱼雷伤害降低50%。\n\n<color=#3A3A3A>推荐应对方式：</color>\n使用不受该抗性影响的攻击方式，或：\n<color=#2F8DE9>【鱼雷战术优化·广域】：\n：恶毒、约翰斯顿、圣地亚哥、特伦托</color>\n降低全体敌人的鱼雷防御15%，不可叠加。"
    if value == "·艦隊に「ライザ」を編入した場合、海域ではエレメントコアを採取する事ができる。\n·【<color=#DB7B00>エレメントコア・雷</color>を採取した時点に】、敵のBOSS艦に【麻痺】効果を与える、（持続10秒）。この効果の持続期間中に、BOSS艦は主砲攻撃、魚雷攻撃及び航空攻撃する事ができなくなる。":
        return "·舰队编入“莱莎”时，可在海域采集元素核心。\n·【采集<color=#DB7B00>元素核心·雷</color>时】，对敌方BOSS舰施加【麻痹】效果（持续10秒）。效果持续期间，BOSS舰无法进行主炮攻击、鱼雷攻击或航空攻击。"
    match = re.fullmatch(r"祈願壁で祈願石を(\d+)回使う", value)
    if match:
        return f"在祈愿墙使用祈愿石{match.group(1)}次"
    match = re.fullmatch(r"赤い結晶エキスを(\d+)を獲得する", value)
    if match:
        return f"获得红色结晶精华{match.group(1)}"
    match = re.fullmatch(r"高強度再生合金板を(\d+)を獲得する", value)
    if match:
        return f"获得高强度再生合金板{match.group(1)}"
    match = re.fullmatch(r"ゲート接続維持器を(\d+)を獲得する", value)
    if match:
        return f"获得网关连接维持器{match.group(1)}"
    if value == "もうすぐ不運に見舞われる人":
        return "即将遭遇不幸的人"
    if value == "強面のオーナー":
        return "凶相的老板"
    pending_pack = {
        "ムーバゼータ誓いの福袋": "搬运者泽塔誓约福袋",
        "迎撃準備お得福袋（限定品）": "迎击准备超值福袋（限定品）",
        "伊勢＆日向歓迎！お得福袋①（限定品）": "伊势＆日向欢迎！超值福袋①（限定品）",
        "伊勢＆日向歓迎！お得福袋②（限定品）": "伊势＆日向欢迎！超值福袋②（限定品）",
        "伊勢＆日向歓迎！お得福袋③（限定品）": "伊势＆日向欢迎！超值福袋③（限定品）",
        "鶴鷹の舞う海＋誓い（限定品）": "鹤鹰飞舞之海＋誓约（限定品）",
        "鶴鷹の舞う海①（限定品）": "鹤鹰飞舞之海①（限定品）",
        "鶴鷹の舞う海②（限定品）": "鹤鹰飞舞之海②（限定品）",
        "鶴鷹の舞う海③（限定品）": "鹤鹰飞舞之海③（限定品）",
        "改造促進セット（限定品）": "改造促进套装（限定品）",
        "クリスマス虹色チップ福袋": "圣诞节彩虹芯片福袋",
        "改造促進セットG（限定品）": "改造促进套装G（限定品）",
        "ミューゴールド福袋": "缪黄金福袋",
        "ミューシルバー福袋": "缪白银福袋",
        "新人向け経験値福袋①（限定品）": "新手经验福袋①（限定品）",
        "新人向け経験値福袋②（限定品）": "新手经验福袋②（限定品）",
        "新人向け経験値福袋③（限定品）": "新手经验福袋③（限定品）",
        "購入して即朝日の配当金サービス獲得": "购买后立即获得朝日的分红服务",
        "鳳舞誓い福袋（限定品）": "凤舞誓约福袋（限定品）",
        "超お得軍備パック": "超值军备礼包",
        "豪華な推薦状パック": "豪华推荐信礼包",
        "豪華な軍備パック": "豪华军备礼包",
        "ムーバーキャラ実装記念福袋①（限定品）": "搬运者角色实装纪念福袋①（限定品）",
        "ムーバーキャラ実装記念福袋②（限定品）": "搬运者角色实装纪念福袋②（限定品）",
        "ムーバーキャラ実装記念福袋③（限定品）": "搬运者角色实装纪念福袋③（限定品）",
        "サンディエゴ歓迎！お得福袋①（限定品）": "圣地亚哥欢迎！超值福袋①（限定品）",
        "サンディエゴ歓迎！お得福袋②（限定品）": "圣地亚哥欢迎！超值福袋②（限定品）",
        "サンディエゴ歓迎！お得福袋③（限定品）": "圣地亚哥欢迎！超值福袋③（限定品）",
        "戦姫育成お得パック（限定品）": "战姬培养超值礼包（限定品）",
        "得々共鳴福袋": "超值共鸣福袋",
        "お得研究福袋（戦艦）": "超值研究福袋（战列舰）",
        "超お得研究福袋①（戦艦）": "超值研究福袋①（战列舰）",
        "超お得研究福袋②（戦艦）": "超值研究福袋②（战列舰）",
    }
    match = re.fullmatch(r"対雷撃\+(\d+)", value)
    if match:
        return f"对雷击+{match.group(1)}"
    match = re.fullmatch(r"夜戦ダメージ\+(\d+(?:\.\d+)?)%", value)
    if match:
        return f"夜战伤害+{match.group(1)}%"
    match = re.fullmatch(r"海域：(\d+-\d+)をクリア", value)
    if match:
        return f"通关海域：{match.group(1)}"
    direct_pack = {
        "お得研究福袋": "超值研究福袋",
        "超お得研究福袋①": "超值研究福袋①",
        "超お得研究福袋②": "超值研究福袋②",
        "虹色チップラッキー福袋": "彩虹芯片幸运福袋",
        "SSR装備超お得福袋": "SSR装备超值福袋",
        "UR装備超お得福袋": "UR装备超值福袋",
        "戦姫育成お得パック": "战姬培养超值礼包",
        "戦姫突破お得パック": "战姬突破超值礼包",
        "ブルーチップ超得福袋": "蓝色芯片超值福袋",
        "ブルーチップ超得福袋（限定品）": "蓝色芯片超值福袋（限定品）",
        "UR戦姫スキル強化福袋（限定品）": "UR战姬技能强化福袋（限定品）",
        "推薦状ラッキー福袋": "推荐信幸运福袋",
        "新人向け福袋④": "新手福袋④",
        "新人ランダム戦姫福袋（限定品）": "新手随机战姬福袋（限定品）",
        "コラボ記念装備福袋": "联动纪念装备福袋",
    }
    direct_pack.update(pending_pack)
    if value in direct_pack:
        return direct_pack[value]
    match = re.fullmatch(r"超お得推薦状パック([①②③])", value)
    if match:
        return f"超值推荐信礼包{match.group(1)}"
    match = re.fullmatch(r"ムーバーゼータと初めて出会って、福袋を獲得できるチャンスがあります", value)
    if match:
        return "首次与搬运者泽塔相遇，有机会获得福袋"
    match = re.fullmatch(r"魚雷ダメージ\+40%。特殊能力獲得：行動派 Lv\.3", value)
    if match:
        return "鱼雷伤害+40%。获得特殊能力：行动派 Lv.3"
    match = re.fullmatch(r"「夜戦」でのダメージ\+30%。特殊能力獲得：錬金術士 Lv\.3", value)
    if match:
        return "“夜战”中的伤害+30%。获得特殊能力：炼金术士 Lv.3"
    match = re.fullmatch(r"戦闘開始」時、タクティクスレベル（T\.Lv）が３まで上昇する。特殊能力獲得：行動派 Lv\.4", value)
    if match:
        return "战斗开始时，战术等级（T.Lv）提升至3。获得特殊能力：行动派 Lv.4"
    match = re.fullmatch(r"「主砲攻撃」の時、一定の確率で「Cutin」が発動する。特殊能力獲得：錬金術士 Lv\.2", value)
    if match:
        return "进行“主炮攻击”时，有一定概率触发“Cutin”。获得特殊能力：炼金术士 Lv.2"
    if value == "主砲の砲撃にオーバーシュートを使い、撃つたびに次の主砲ダメージ+6%、この効果は1度の戦闘で5回まで重複する。":
        return "主炮射击使用超调，每次射击使下次主炮伤害+6%，该效果单场战斗最多叠加5次。"
    match = BROKEN_SKIN_RE.fullmatch(value)
    if match:
        name = SHIP_NAME_DIRECT.get(match.group(1), match.group(1))
        return (
            f"战姬大破换装系列！保存了【{name}】的破损状态的特别品。"
            f"以战斗开始即大破的心态出击，就无所畏惧了{match.group(2)}"
        )
    if value.endswith("大破"):
        name = value[:-2]
        if name in SHIP_NAME_DIRECT:
            return SHIP_NAME_DIRECT[name] + "大破"
    for suffix in ("·改二", "·改2", "·改", "改二", "改2", "改"):
        if value.endswith(suffix):
            name = value[: -len(suffix)]
            if name in SHIP_NAME_DIRECT:
                return SHIP_NAME_DIRECT[name] + suffix
    if value in SHIP_NAME_DIRECT:
        return SHIP_NAME_DIRECT[value]
    mini_box = re.fullmatch(r"ミニ(.+)確定箱", value)
    if mini_box:
        name = SHIP_NAME_DIRECT.get(mini_box.group(1), mini_box.group(1))
        return f"迷你{name}确切箱"
    chat_stamp = re.fullmatch(r"(.+?)のスタンプ、チャットで利用可能", value)
    if chat_stamp:
        name = SHIP_NAME_DIRECT.get(chat_stamp.group(1), chat_stamp.group(1))
        return f"{name}的印章，可在聊天中使用"
    stamp = re.fullmatch(r"(.+?)のスタンプ", value)
    if stamp:
        name = SHIP_NAME_DIRECT.get(stamp.group(1), stamp.group(1))
        return f"{name}的印章"
    rainbow_box = re.fullmatch(r"使用するとランダムに(\d+)~(\d+)個の虹色チップがもらえます", value)
    if rainbow_box:
        return f"使用后可随机获得{rainbow_box.group(1)}～{rainbow_box.group(2)}个彩虹芯片"
    deep_memory = re.fullmatch(r"深海の記憶（(.+)）", value)
    if deep_memory:
        name = SHIP_NAME_DIRECT.get(deep_memory.group(1), deep_memory.group(1))
        return f"深海的记忆（{name}）"
    exact = {
        "魚雷ミサイル": "鱼雷导弹",
        "チャック報酬": "查克奖励",
        "花咲き物語限定ガチャ": "花开物语限定抽卡",
        "10回-召喚する": "召唤10次",
        "1回-召喚する": "召唤1次",
        "敵精鋭艦隊を壊滅する": "歼灭敌方精锐舰队",
        "歓声アンコール": "欢呼·安可",
        "艦隊箱": "舰队箱",
        "ビスマルク": "俾斯麦",
        "ビスマルク·改二": "俾斯麦·改二",
        "ラムダ": "拉姆达",
        "ラムダ·改": "拉姆达·改",
        "ラムダ·改二": "拉姆达·改二",
        "アンソン": "安森",
        "アンソン·改二": "安森·改二",
        "バンカーヒル": "邦克山",
        "サンディエゴ": "圣地亚哥",
        "ラボ": "实验室",
        "ドロップ率UP": "掉落率UP",
        "コラボ図鑑": "联动图鉴",
        "ミニハウ確定箱": "迷你豪确切箱",
        "ミニフィウメ確定箱": "迷你菲乌梅确切箱",
        "戦姫の特徴ある表情。アイコンとして使用可能。": "战姬的特色表情。可用作头像。",
        "戦姫未獲得": "未获得战姬",
        "砲撃+": "炮击+",
        "対艦強化+": "对舰强化+",
        "突破パック①（限定版）": "突破礼包①（限定版）",
        "突破パック②（限定版）": "突破礼包②（限定版）",
        "突破パック③（限定版）": "突破礼包③（限定版）",
        "レアアイコンボックス": "稀有头像箱",
        "イータアイコンセット": "伊塔头像套装",
        "歓声アンコールスタンプ": "欢呼·安可印章",
        "イータ大破スタンプ": "伊塔大破印章",
        "GWイベントの報酬": "GW活动奖励",
        "赤い結晶エキス": "红色结晶精华",
        "さんまの塩焼き": "盐烤秋刀鱼",
        "松茸のホイル焼き": "锡纸烤松茸",
        "玄米ご飯": "糙米饭",
        "オースのエネルギーを応用して戦姫の作戦時の出力を高める特殊装置。一般的な汎用型などのモデルよりも、ブルースターやムーバーの従来のモードとは異なるエネルギー運行に適応することができる。": "应用奥斯能量提升战姬作战时输出的特殊装置。相比普通通用型等型号，它能适应不同于蓝星与搬运者传统模式的能量运行方式。",
        "他のエネルギーを使うはずの装置が、エネルギーの中和剤として、オースエネルギーまたはウンターオースエネルギーを受け取ることができるようにする。通常のオースエネルギー中和剤よりも製造技術が複雑だ。": "让原本应使用其他能量的装置能够接收奥斯能量或温特奥斯能量作为能量中和剂。制造技术比普通奥斯能量中和剂更加复杂。",
        "大型の艦船に使われる通常型よりも軽く、艤装の製造にも使える。技術や製造コストが高いにもかかわらず、あるユーザーから嫌われていた。": "比大型舰船使用的普通型号更轻，也可用于制造舰装。尽管技术与制造成本都很高，却曾受到某位用户的厌恶。",
        "豆腐にえびやしめじなどの食材をのせて、お手軽でヘルシーのグラタン。コクのある仕上がりの秘訣はみその入った和風ホワイトソース！": "在豆腐上放虾、蟹味菇等食材制作的简便健康焗烤。浓郁口感的秘诀，是加入味噌的日式白酱！",
        "牛バラ肉と焼き豆腐、そして長ねぎを加えて作った一品。お肉も豆腐もしっとりしていて美味しい。まさに家庭料理の定番！": "加入牛五花肉、烤豆腐和大葱制作的一道菜。肉和豆腐都鲜嫩多汁，美味可口。正是家常菜的经典！",
        "ぷりっとしたえびにうま味たっぷりの牛挽き肉、そしてキャベツの代わりにみじん切りのしめじをたくさん使った具だくさん餃子！がっつり食べられて栄養も満足感もたっぷり！": "Q弹的虾仁搭配鲜味十足的牛肉馅，再用大量蟹味菇碎代替卷心菜，做成馅料丰富的饺子！吃得过瘾，营养与满足感都满满！",
        "なんと豆腐入りのシュウマイ！牛肉と豆腐、えびと豆腐の二種類がある。柔らかい豆腐と噛みごたえのあるえび、牛肉、食感のハーモニーがクセになる！": "竟然是加入豆腐的烧卖！有牛肉豆腐和虾仁豆腐两种。柔软的豆腐与有嚼劲的虾仁、牛肉形成口感交响，让人欲罢不能！",
        "ゲートの通路を安定的に維持できる装置。単一作戦部隊に割り当てられたこのモデルは、比較的エネルギーの消耗は少ないが、使用には高度な技術が要求される。": "可稳定维持网关通道的装置。分配给单一作战部队的该型号能耗相对较低，但使用时需要高超技术。",
        "喚起企画を完成して「赤のコイン」を獲得でき、赤改造ショップでアイテムを交換することができます。": "完成唤起企划后可获得“红色硬币”，并在红色改造商店兑换道具。",
        "鉄板で焼いたサイコロステーキとえびのマリネ焼き。シンプルな調理法こそ引き出せる食材本来の旨み！": "铁板煎骰子牛排与腌烤虾。正是简单的烹饪方式，才能激发食材本来的鲜味！",
        "にんにくを効かせた、シンプルだけど美味しいお手軽料理！ご飯も進むし、お酒にも合うらしいよ！": "蒜香十足、简单却美味的快手料理！下饭又适合配酒，据说非常不错！",
        "豆腐に衣をまとわせて揚げ、だし汁で味を付けた料理。外はカリッと、中はふわふわ！シンプルだけとやさしい味が心に染みる！": "豆腐裹上面衣油炸，再用高汤调味的料理。外酥里嫩！简单却温和的味道沁人心脾！",
        "「カタバミと三つ葉のオークに見守られながら」イベントにて、限定ショップから交換で入手する事ができます。「敵艦、どこ～？」": "可在“在酢浆草与三叶橡树的守望下”活动中，通过限定商店兑换获得。“敌舰，你在哪里～？”",
        "【第四期】最強艦隊イベントにて入手可能な限定スタンプ。「天下布運！」": "【第四期】最强舰队活动中可获得的限定印章。“天下布运！”",
        "第5期Commnd Sennkiイベントにて獲得できるグローウォームの着せ替え-歓声アンコールスタンプ！チャットで利用可能。": "第5期Command 战姬活动中可获得的萤火虫换装——欢呼·安可印章！可在聊天中使用。",
        "【雷神の申し子出現率UPガチャ】の40連確定報酬として獲得できる記念スタンプ！「寝ても覚めてもあなたを待ってるわ、ふふ…」": "作为【雷神之子出现率UP抽卡】40连保底奖励可获得的纪念印章！“无论睡着还是醒着，我都会等着你，呵呵……”",
        "ブルースフィアの来訪者イベントの限定のスタンプ。イベントショップにて抽選で獲得可能。「激射～！」": "蓝星访客活动限定印章。可在活动商店抽选获得。“猛烈射击～！”",
        "【ハロウィン限定】最強艦隊イベントの限定のスタンプ。大艦隊PT報酬で獲得可能。「うっ…」": "【万圣节限定】最强舰队活动限定印章。可通过大舰队PT奖励获得。“呜……”",
        "ビギナー精鋭パック [Lv10]": "新手精英礼包 [Lv10]",
        "ビギナー精鋭パック [Lv15]": "新手精英礼包 [Lv15]",
        "ビギナー精鋭パック [Lv20]": "新手精英礼包 [Lv20]",
        "ビギナー精鋭パック [Lv25]": "新手精英礼包 [Lv25]",
        "特殊オースエネルギー駆動装置:Ⅰ型": "特殊奥斯能量驱动装置：Ⅰ型",
        "特殊エネルギー中和剤": "特殊能量中和剂",
        "軽量タイプの高強度再生合金板": "轻量型高强度再生合金板",
        "豆腐としめじと海老のグラタン": "豆腐、蟹味菇与虾焗烤",
        "具だくさん焼き餃子": "什锦煎饺",
        "豆腐シュウマイ": "豆腐烧卖",
        "戦術型ゲート接続維持器": "战术型网关连接维持器",
        "赤のコイン": "红色硬币",
        "サイコロステーキとエビの鉄板焼": "骰子牛排与铁板烤虾",
        "しめじのにんにくしょうゆ炒め": "蒜香酱油炒蟹味菇",
        "揚げ出し豆腐": "炸出汁豆腐",
        "騎士団のミニ戦姫箱": "骑士团迷你战姬箱",
        "深海の記憶選択箱": "深海记忆选择箱",
        "戦姫アイコン箱": "战姬头像箱",
        "ムーバー戦姫欠片選択箱": "搬运者战姬碎片选择箱",
        "超レアSSR装備選択箱": "超稀有SSR装备选择箱",
        "大破着せ替え選択箱+": "大破换装选择箱+",
        "精選SSR戦姫選択箱（限定版）": "精选SSR战姬选择箱（限定版）",
        "精選SSR戦姫選択箱（限定版①）": "精选SSR战姬选择箱（限定版①）",
        "装飾品ボックス1": "饰品箱1",
        "装飾品ボックス2": "饰品箱2",
        "指定戦姫ボックス": "指定战姬箱",
        "指定戦姫選択箱（ガチャ限定版）": "指定战姬选择箱（抽卡限定版）",
        "UR装備選択箱（ガチャ限定版）": "UR装备选择箱（抽卡限定版）",
        "UR装備選択箱（ガチャ限定）": "UR装备选择箱（抽卡限定）",
        "共鳴素材選択パック": "共鸣素材选择礼包",
        "指定戦姫チケット（限定版）": "指定战姬兑换券（限定版）",
        "ムーバーイータ・ムーバーミュー・ムーバーゼータかけら選択箱": "搬运者伊塔·搬运者缪·搬运者泽塔碎片选择箱",
        "ムーバーイータ・ムーバーゼータ・ムーバーミューかけら選択箱": "搬运者伊塔·搬运者泽塔·搬运者缪碎片选择箱",
        "2022てんびん座選択箱": "2022天秤座选择箱",
        "てんびん座選択箱": "天秤座选择箱",
        "いて座選択箱": "射手座选择箱",
        "やぎ座選択箱": "摩羯座选择箱",
        "みずがめ座選択箱": "水瓶座选择箱",
        "うお座選択箱": "双鱼座选择箱",
        "上級祈願石＆万象ファクター①": "高级祈愿石＆万象因子①",
        "上級祈願石＆万象ファクター②": "高级祈愿石＆万象因子②",
        "上級祈願石＆万象ファクター③": "高级祈愿石＆万象因子③",
        "初回チャージ特典戦姫選択箱": "首充特典战姬选择箱",
        "強化アイテム選択箱": "强化道具选择箱",
        "専属ガチャ育成アイテム選択箱": "专属抽卡培养道具选择箱",
        "推薦状or虹色チップ指定箱": "推荐信或彩虹芯片指定箱",
        "着せ替え選択箱（ハロウィン限定）": "换装选择箱（万圣节限定）",
        "戦姫育成パック（ガチャ限定）": "战姬培养礼包（抽卡限定）",
        "ムーバーイータ・ムーバーゼータかけら選択箱": "搬运者伊塔·搬运者泽塔碎片选择箱",
        "特選改造素材箱セット①": "精选改造素材箱套装①",
        "特選改造素材箱セット②": "精选改造素材箱套装②",
        "特選改造素材箱セット③": "精选改造素材箱套装③",
        "新人向けSSR戦姫選択箱": "新人SSR战姬选择箱",
        "サクラの戦姫選択箱（限定品）": "樱花战姬选择箱（限定品）",
        "2022おうし座選択箱": "2022金牛座选择箱",
        "2022いて座選択箱": "2022射手座选择箱",
        "2022やぎ座選択箱": "2022摩羯座选择箱",
        "2023みずがめ座選択箱": "2023水瓶座选择箱",
        "特別艦種選択パック": "特殊舰种选择礼包",
        "月明け祈願特注石選択箱（松）": "月初祈愿特制石选择箱（松）",
        "月明け祈願特注石選択箱（竹）": "月初祈愿特制石选择箱（竹）",
        "月明け祈願特注石選択箱（梅）": "月初祈愿特制石选择箱（梅）",
        "ピックアップSSR戦姫選択箱": "精选SSR战姬选择箱",
        "空っぽ…素材集めしよう！": "空空如也……去收集素材吧！",
        "空っぽ…装備集めしよう！": "空空如也……去收集装备吧！",
        "メールが存在しません": "邮件不存在",
        "すべてのチーム": "所有队伍",
        "チームのタイプを選んでください": "请选择队伍类型",
        "チームリスト": "队伍列表",
        "招待状を送る": "发送邀请函",
        "%sはチームに参加しました。": "%s加入了队伍。",
        "チームは満員です。": "队伍已满。",
        "戦艦限定ランダム箱": "战列舰限定随机箱",
        "駆逐艦限定ランダム箱": "驱逐舰限定随机箱",
        "駆逐艦装備選択箱（ガチャ限定）": "驱逐舰装备选择箱（抽卡限定）",
        "軽巡洋艦装備選択箱（ガチャ限定）": "轻巡洋舰装备选择箱（抽卡限定）",
        "重巡洋艦装備選択箱（ガチャ限定）": "重巡洋舰装备选择箱（抽卡限定）",
    }
    if value in exact:
        return exact[value]
    compact_value = re.sub(r"\s+", "", normalized_text(value))
    common_event = {
        "神通を迎えに行く": "去迎接神通",
        "価格：出席ポイント": "价格：出席点数",
        "料理台の食材は既に選択済みです。これ以上追加する事はできません。": "料理台的食材已经选定，无法继续添加。",
        "プレイヤー": "玩家",
        "ランキング報酬": "排名奖励",
        "イベント期間中、個人PTが1以上の隊員を大艦隊から追放する事ができません": "活动期间无法将个人PT达到1以上的成员踢出大舰队",
        "同じ品質のミッションを重複受取る事ができません": "无法重复领取相同品质的任务",
        "期間限定中にドロップ率が大幅UP！": "限时期间掉落率大幅提升！",
        "レイドボスを撃沈しました！": "已击沉Raid Boss！",
        "以下の報酬を獲得": "获得以下奖励",
        "%sに1回出撃する事で、報酬を獲得する事ができます。": "向%s出击1次即可获得奖励。",
        "このイベントは終了しました": "该活动已结束",
        "レイドボスが撃沈されました！「撃沈報酬」を獲得できます": "Raid Boss已被击沉！可获得“击沉奖励”",
        "大艦隊に入隊する事で、「大艦隊レイドボス」に挑戦する事ができます": "加入大舰队后即可挑战“大舰队Raid Boss”",
        "※「大艦隊レイドボス」イベントの開催後に大艦隊に入隊した際は、次回のイベントから挑戦する事ができます。": "※在“大舰队Raid Boss”活动开始后加入大舰队，只能从下次活动开始挑战。",
        "出撃不可な戦姫が編成されています。出撃できません": "编入了无法出击的战姬，无法出击。",
        "残り挑戦回数が不足しています。出撃できません": "剩余挑战次数不足，无法出击。",
        "総合ダメージ：": "总伤害：",
        "獲得したレイドボスPT：": "获得的Raid Boss PT：",
        "比例ダメージ：": "比例伤害：",
        "必要以上な素材が選択されました。必要分の素材だけご選択ください。": "选择的素材超过所需数量，请只选择需要的素材。",
        "この艦隊にはムーバー系戦姫の数が既に上限に到達しました。": "该舰队中的搬运者系战姬数量已达到上限。",
        "この海域では、ムーバー系戦姫が出撃する事ができません。": "搬运者系战姬无法在本海域出击。",
        "この海域では、ムーバー系戦姫が最大%s体まで出撃可能。": "本海域最多可出击%s名搬运者系战姬。",
        "ドックの容量が足りません": "船坞容量不足",
        "レイドボスPT": "Raid Boss PT",
        "敵BOSSは既に他のメンバーに撃沈されました": "敌方BOSS已被其他成员击沉",
        "ただいま未開放です。しばらくお待ち下さい。": "当前尚未开放，请稍候。",
        "大艦隊レイドボスイベントの開催期間に、大艦隊を解散する事ができません。": "大舰队Raid Boss活动期间无法解散大舰队。",
        "敵BOSSを撃沈する事で、隊員全員が豪華報酬を獲得する事ができます。": "击沉敌方BOSS后，全体成员都可获得丰厚奖励。",
        "ダウンバースト": "下击暴流",
        "貢献ランキング": "贡献排名",
        "今回ダメージUP": "本次伤害提升",
        "次回ダメージUP": "下次伤害提升",
        "%d%%ダメージUP": "伤害提升%d%%",
        "スキン": "换装",
        "%d回-入荷する": "进货%d次",
        "改修素材が不足しているため、改修する事ができません。": "改修素材不足，无法进行改修。",
        "リセットする事で、抽選する事ができます。": "重置后即可进行抽选。",
        "在庫補充する事で、抽選する事ができます。": "补充库存后即可进行抽选。",
        "リセットしますか？": "要重置吗？",
        "在庫補充しますか？": "要补充库存吗？",
        "累計おみくじ数：": "累计抽签次数：",
        "1.最大120連以内に、必ず対象装備を全て獲得する事が可能です。\n2.全対象装備が【獲得済み】でない状態は、最大30連以内イベントガチャを引くごとに、*ランダムに必ず【未獲得状態】の対象装備を獲得できます（獲得した対象装備は獲得済み状態になります）\n3.全対象装備を獲得した以降は、最大30連以内イベントガチャを引くごとに、*ランダムに必ず対象装備を獲得できます。\n\n※全対象装備を獲得したら、続けてガチャを引くことをオススメいたしません。\n\n※全対象装備を獲得するまでの、【確定獲得】の確率一覧（獲得済みとなった対象装備は【確定獲得対象装備】から外されます）\n彗星の確率：1/81\n二十五粍高角機銃の確率：10/81\n50口径三年式140mm砲の確率：30/81\nバラクーダの確率：40/81\n\n※全対象装備を獲得した以降の、【確定獲得】の確率一覧\n彗星の確率：18%\n二十五粍高角機銃の確率：18%\n50口径三年式140mm砲の確率：32%\nバラクーダの確率：32%": "1.最多120连内必定获得全部目标装备。\n2.在全部目标装备均未【获得】时，每次抽取活动卡池，最多30连内必定随机获得处于【未获得】状态的目标装备（获得的目标装备将变为已获得状态）。\n3.获得全部目标装备后，每次抽取活动卡池，最多30连内必定随机获得目标装备。\n\n※获得全部目标装备后，不建议继续抽卡。\n\n※获得全部目标装备前的【必定获得】概率列表（已获得的目标装备会从【必定获得目标装备】中移除）\n彗星概率：1/81\n25毫米高射机枪概率：10/81\n50口径三年式140毫米炮概率：30/81\n梭鱼概率：40/81\n\n※获得全部目标装备后的【必定获得】概率列表\n彗星概率：18%\n25毫米高射机枪概率：18%\n50口径三年式140毫米炮概率：32%\n梭鱼概率：32%",
        "報酬はイベント終了後にメールにてお送りいたします。\n個人PTを<color=#3679f5>1000</color>に上げることでランキング報酬を入手できます。": "奖励将在活动结束后通过邮件发送。\n个人PT达到<color=#3679f5>1000</color>即可获得排名奖励。",
        "この隊員が今回の大艦隊レイドボスイベントでレイドボスPTを獲得したことがありますので、大艦隊から追放する事ができません。": "该成员在本次大舰队Raid Boss活动中获得过Raid Boss PT，无法将其踢出大舰队。",
        "このガチャの大当たり賞品は全部在庫切れになりました。リセットしますか？": "该卡池的全部大奖物品已售罄。要重置吗？",
        "このガチャの大当たり賞品は全部在庫切れになりました。在庫補充しますか？": "该卡池的全部大奖物品已售罄。要补充库存吗？",
        "制服の袖通す…新学期シーズン到来": "穿上制服……新学期季节到来",
        "ストーリー": "剧情",
        "サーバーが全目標を達成するスピードに応じて、全服プレイヤーは下記の報酬を獲得できます。": "根据服务器完成全部目标的速度，全服玩家可获得以下奖励。",
        "前節のあらすじを先にクリアしてください": "请先完成上一节剧情。",
        "終了カウントダウン:%s": "结束倒计时：%s",
        "%sに開放予定": "预计于%s开放",
        "残りのガチャ回数不足": "剩余抽卡次数不足",
        "これ以上、このガチャを探索する事はできません": "无法继续探索该卡池",
        "QRコード": "二维码",
        "・敵艦に与えた魚雷ダメージ＋100%%\n·戦姫イータ、ゼータ、ミュー、ラムダ、赤が敵艦に与えるダメージ+25%%": "・对敌舰造成的鱼雷伤害+100%%\n·战姬伊塔、泽塔、缪、拉姆达、赤对敌舰造成的伤害+25%%",
        "・敵艦に与えた副砲ダメージ＋1000%%\n·戦姫イータ、ゼータ、ミュー、ラムダ、赤が敵艦に与えるダメージ+25%%": "・对敌舰造成的副炮伤害+1000%%\n·战姬伊塔、泽塔、缪、拉姆达、赤对敌舰造成的伤害+25%%",
        "・敵艦に与えたダメージ－50%%（航空攻撃除く）\n·戦姫イータ、ゼータ、ミュー、ラムダ、赤が敵艦に与えるダメージ+25%%": "・对敌舰造成的伤害-50%%（不含航空攻击）\n·战姬伊塔、泽塔、缪、拉姆达、赤对敌舰造成的伤害+25%%",
        "・敵艦に与えたダメージ－50%%（主砲攻撃除く）\n·戦姫イータ、ゼータ、ミュー、ラムダ、赤が敵艦に与えるダメージ+25%%": "・对敌舰造成的伤害-50%%（不含主炮攻击）\n·战姬伊塔、泽塔、缪、拉姆达、赤对敌舰造成的伤害+25%%",
        "・味方全体の攻撃ダメージ－50%%、クリティカル率－50%%\n·戦姫イータ、ゼータ、ミュー、ラムダ、赤が敵艦に与えるダメージ+25%%": "・我方全体攻击伤害-50%%、暴击率-50%%\n·战姬伊塔、泽塔、缪、拉姆达、赤对敌舰造成的伤害+25%%",
        "ご注意": "请注意",
        "アカウント削除": "删除账号",
        "食材が不足してます。このレシピの料理はできません": "食材不足，无法制作这道料理。",
        "1.期間中、美食マスターの道イベントで様々な食材を獲得し、レシピを使って料理する事ができます。\n2.料理を成功させると料理報酬を入手する事ができます。各料理毎に異なる料理報酬が獲得可能です。\n3.一定回数以上同じ料理を行い、全料理報酬を獲得すると、次回からリセット報酬に変更となります。\n4.レシピには、必要な食材ヒントがあり、レシピに沿って料理を行うと効果的です。\n5.特殊な条件で解放される特殊レシビがあり、料理した際の報酬も通常より豪華報酬として獲得する事ができます。": "1.活动期间，可在美食大师之路活动中获得各种食材，并使用配方制作料理。\n2.成功制作料理后可获得料理奖励。每种料理都有不同的料理奖励。\n3.制作相同料理达到一定次数并获得全部料理奖励后，之后将改为获得重置奖励。\n4.配方中有必要食材提示，按照配方制作料理效果更好。\n5.部分特殊配方需要满足特殊条件才能解锁，制作后可获得比普通奖励更加丰厚的奖励。",
        "%sが不足しています": "%s不足",
        "残りの報酬獲得回数：": "剩余奖励领取次数：",
        "前回レシピ": "上次配方",
        "レシピ一覧": "配方列表",
        "ディリー": "每日",
        "ミッション": "任务",
        "完成させる": "完成",
        "<color=#BE1F4D>【戦姫祈願】</color>は戦力確保の要、くれぐれも忘れないようにね♪": "<color=#BE1F4D>【战姬祈愿】</color>是获取战力的关键，可千万别忘了♪",
        "何をするかしていいか分からないとき、あたしにコメントでもしてれば…なんて冗談よ～": "不知道该做什么时，就来问问我吧……开玩笑的啦～",
        "レア度<color=#BE1F4D>【R】</color>以上の戦姫を<color=#BE1F4D>【除隊】</color>させることで<color=#BE1F4D>【建造素材】</color>を入手できるわ！": "将稀有度<color=#BE1F4D>【R】</color>以上的战姬<color=#BE1F4D>【退役】</color>后，就能获得<color=#BE1F4D>【建造素材】</color>！",
        "序盤は大人しく<color=#BE1F4D>【デイリー任務】</color>でもやって、<color=#BE1F4D>【温泉コイン】</color>を貯めなさい": "前期先乖乖完成<color=#BE1F4D>【每日任务】</color>，积攒<color=#BE1F4D>【温泉硬币】</color>吧。",
        "海域第三章は新米指揮官にとって難関かもしれないが、艦隊の練度を確実にあげた指揮官にとっては簡単なはずよ!!": "海域第三章对新手指挥官来说可能很难，但对稳步提升舰队熟练度的指挥官来说应该不成问题!!",
        "宝蔵は受け取り済みです": "宝藏已领取",
        "やあ、指揮官さま！あたし、瑞鶴、翔鶴型航空母艦の瑞鶴っていうの、よろしくね♪": "嗨，指挥官大人！我是瑞鹤，翔鹤型航空母舰的瑞鹤，请多关照♪",
        "見掛けによらずやるわね！見直してやるわ～": "没想到你挺能干嘛！我对你刮目相看啦～",
        "引き継ぎキャラは現在のデバイスの役割\nを置き換えます。": "继承角色将替换当前设备中的角色。",
        "設定の中でコードスキャンに必要なカメラの権限を開いてください。": "请在设置中开启扫码所需的相机权限。",
        "カイシュウ": "改修",
        "ステップ%s": "步骤%s",
        "作戦成功！": "作战成功！",
        "この戦闘で%sのダメージを与えました。": "本场战斗造成了%s伤害。",
        "ステップ：": "步骤：",
        "本日累計して%s%d個以上の消費する。挑戦回数+%d": "今日累计消费%s%d个以上。挑战次数+%d",
        "大艦隊の評価が高いほど「評価報酬」が豪華になります。": "大舰队评价越高，“评价奖励”越丰厚。",
        "レイドボス": "Raid Boss",
        "このイベント終了まで残り：": "距离活动结束还剩：",
        "レイドランキング": "Raid排名",
        "個人ランキング": "个人排名",
        "レイド情報": "Raid信息",
        "%sで開放 -": "达到%s后开放-",
        "アカウント情報取得に失敗しました。\n通信状態の良い環境で、もう一度やり直してください。": "获取账号信息失败。\n请在网络状况良好的环境中重试。",
        "ID退出いたしました。『ID入力』をタップして、ログインしたいプレイ方法を選択できます。（Twitter/Google/引き継ぎコード）\nまたゲストプレイの際は、直接*出撃をタップし、ログインする事で、端末上のアカウントをプレイいただけます。（サーバーをお間違えないように、ご確認ください）": "已退出ID。点击『输入ID』即可选择登录方式。（Twitter/Google/继承码）\n使用游客模式时，直接点击*出击并登录，即可游玩设备上的账号。（请确认服务器选择正确）",
        "現在使用中のアカウント状態を退出します。※退出すると未連携状態となりますので、ゲストプレイの方は引き継ぎコードの発行の確認をお願いいたします。": "将退出当前使用中的账号。※退出后账号会变为未绑定状态，游客玩家请确认已生成继承码。",
        "すべての雪の玉を寄贈しますか？": "要捐赠全部雪球吗？",
        "リストの報酬は基本報酬で、実際の報酬は報酬係数をかける必要です（現在の係数：%s）": "列表中的奖励为基础奖励，实际奖励需要乘以奖励系数（当前系数：%s）",
        "ただいま掃討中です。帰投してから出撃する事ができます。": "当前正在扫荡，返航后才可出击。",
        "各レシピを完成させるには正しい食材を調合する事で料理できます": "每个配方都需要调合正确的食材才能制作料理。",
        "まだ料理されていません": "尚未制作料理",
        "調査海域2－Aをクリアする事で、【夏の大運動会】に参加する事ができます": "完成调查海域2-A后即可参加【夏日大运动会】",
        "このシーズンは終了しました。次のシーズンの開放時間はx月x日になります": "本赛季已结束。下一赛季将于x月x日开放",
        "ミッション詳細": "任务详情",
        "ミッションを受け取っていません": "尚未领取任务",
        "終了まで残り：": "距离结束还剩：",
        "報酬はイベント終了後にメールにてお送りいたします。": "奖励将在活动结束后通过邮件发送。",
        "受取る回数：": "领取次数：",
        "ミッション進度：": "任务进度：",
        "このミッションをキャンセルしますか？（消費された受取る回数が返還しませんのでご注意ください）": "要取消该任务吗？（已消耗的领取次数不会返还，请注意）",
        "個人PTを<color=#3679f5>%s</color>に上げることで入手できます。 ": "个人PT达到<color=#3679f5>%s</color>即可获得。",
        "イベントがまもなく終了します。個人PTをご確認ください。": "活动即将结束，请确认个人PT。",
        "回数がなくなりましたので、リセットする事ができません": "次数已用尽，无法重置",
        "このミッションを他隊員が注文しましたので、他のミッションをご選択ください": "其他成员已接取该任务，请选择其他任务",
        "大艦隊に加入する事で、イベントに参加する事ができます": "加入大舰队后即可参加活动",
        "既に受取済みです": "已经领取",
        "潜る": "潜水",
        "潜水ステータス": "潜水状态",
        "補修艦支援突撃": "维修舰支援突击",
        "旗艦スキル：": "旗舰技能：",
        "リセット報酬": "重置奖励",
        "1、イベントが開始以降に大艦隊に加入した隊員は、次回のイベントから参加する事ができます。\n2、イベント旗艦中に隊員が大艦隊から脱退する場合は、脱退した時点からイベントに参加する事ができません。\n3、個人ランキングにランクインするには、個人PTが1000以上に到達する必要があります。\n4、イベント期間中、ミッションの受取る回数の購入制限は、毎日0時にリセットされます。\n5、イベント期間中、艦長は、個人PTが1以上の隊員を大艦隊から追放する事ができません。\n6、全てのイベントミッションは、受け取ってから有効となります。受け取らないままにミッション内容をクリアしても、ミッションクリア・報酬取得する事ができませんので、ご注意ください。\n7、ミッションを受け取った後、指定された時間制限以内にミッションをクリアしなければなりません。\n8、毎日に、ミッションの受取回数を1回無料に購入する事ができます。\n9、ミッションがリセットされます。一つのミッションが受注されたら、もう一つ（同様レベル）のミッションが出現し、受取可能になります。\n10、イベント中に別の大艦隊へ移籍する場合は、元の大艦隊の報酬は獲得する事ができますが、新たな大艦隊にて、次からイベントに参加する事ができます。": "1、活动开始后加入大舰队的成员，只能从下次活动开始参加。\n2、活动期间成员退出大舰队后，从退出时起无法继续参加活动。\n3、进入个人排名需要个人PT达到1000以上。\n4、活动期间，任务领取次数的购买限制每天0点重置。\n5、活动期间，舰长无法将个人PT达到1以上的成员踢出大舰队。\n6、所有活动任务在领取后才会生效。未领取任务时即使完成任务内容，也无法完成任务或领取奖励，请注意。\n7、领取任务后，必须在指定时间限制内完成任务。\n8、每天可以免费购买1次任务领取次数。\n9、任务会重置。一个任务被接取后，会出现另一个同等级任务并可领取。\n10、活动期间转移到其他大舰队时，可以获得原大舰队的奖励，但只能从下一次活动开始参加新大舰队的活动。",
        "ポイントが足りません": "点数不足",
        "受け取れる報酬なし": "没有可领取的奖励",
        "ニシン鉄砲": "鲱鱼炮",
        "極限パルクール": "极限跑酷",
        "第二戦速から全速に切り替える度に、一時的に加速効果が獲得する事ができます": "每次从第二战速切换到全速时，可暂时获得加速效果",
        "中秋の名月を迎える日に胸を飾るべき勲章（2020年10月1日限定版）": "在中秋明月之日装点胸前的勋章（2020年10月1日限定版）",
        "SⅡ射撃金メダル": "SⅡ射击金牌", "SⅡ射撃銀メダル": "SⅡ射击银牌", "SⅡ射撃銅メダル": "SⅡ射击铜牌",
        "SⅢ射撃金メダル": "SⅢ射击金牌", "SⅢ射撃銀メダル": "SⅢ射击银牌", "SⅢ射撃銅メダル": "SⅢ射击铜牌",
        "SⅠ追撃金メダル": "SⅠ追击金牌", "SⅠ追撃銀メダル": "SⅠ追击银牌", "SⅠ追撃銅メダル": "SⅠ追击铜牌",
        "SⅡ追撃金メダル": "SⅡ追击金牌", "SⅡ追撃銀メダル": "SⅡ追击银牌", "SⅡ追撃銅メダル": "SⅡ追击铜牌",
        "SⅢ追撃金メダル": "SⅢ追击金牌", "SⅢ追撃銀メダル": "SⅢ追击银牌", "SⅢ追撃銅メダル": "SⅢ追击铜牌",
        "SⅠ陸上金メダル": "SⅠ田径金牌", "SⅠ陸上銀メダル": "SⅠ田径银牌", "SⅠ陸上銅メダル": "SⅠ田径铜牌",
        "SⅡ陸上金メダル": "SⅡ田径金牌", "SⅡ陸上銀メダル": "SⅡ田径银牌", "SⅡ陸上銅メダル": "SⅡ田径铜牌",
        "SⅢ陸上金メダル": "SⅢ田径金牌", "SⅢ陸上銀メダル": "SⅢ田径银牌", "SⅢ陸上銅メダル": "SⅢ田径铜牌",
        "SⅠ射撃金メダル": "SⅠ射击金牌", "SⅠ射撃銀メダル": "SⅠ射击银牌", "SⅠ射撃銅メダル": "SⅠ射击铜牌",
        "例外もあったけど、やはり料理は見た目も味も大事だ！": "虽然也有例外，但料理果然外观和味道都很重要！",
        "美味しい料理とは": "什么是美味的料理",
        "その名前自体が誉れである。ヴェネトの記念勲章": "仅凭这个名字就足以自豪。维内托的纪念勋章",
        "2022年バレンタインの証": "2022年情人节的证明",
        "イベント実施中にて、獲得可能！": "活动期间可获得！",
        "「カタバミと三つ葉のオークに見守られながら」イベントにて、限定海域の進度宝箱から獲得する事ができます。「黒猫は黒だが、足だけが白いんだ～」": "可在“在酢浆草与三叶橡树的守望下”活动中，通过限定海域进度宝箱获得。“黑猫虽然是黑色的，但只有脚是白色的～”",
        "Command Sennkiバレンタインスペシャルイベントにて、獲得できる記念勲章！ピンクの矢で心を射抜かれちゃう～": "Command 战姬情人节特别活动可获得的纪念勋章！小心被粉色箭矢射中心脏哦～",
        "戦功をたたえる勲章というよりは、装飾品の美しさだけを追求しているようなものだった。襟につけるのが正しいそうです。": "与其说是表彰战功的勋章，不如说是只追求装饰美感的饰品。据说正确戴法是别在衣领上。",
        "愛宕＆高雄 雨宿り編イベントにて獲得できる限定勲章！愛宕と高雄に摩耶が加わった証し！": "可在“爱宕＆高雄·避雨篇”活动中获得的限定勋章！证明摩耶加入了爱宕与高雄！",
        "剣と月と誓いの勲章": "剑、月与誓约的勋章",
        "”蒼藍アンコウ革命”にて、獲得可能！": "可在“苍蓝鮟鱇革命”中获得！",
        "蒼藍アンコウ革命（復刻版）イベント開催を記念して作られたアンコウ鍋風の勲章！味同様のクセのあるデザインに批判殺到である！": "为纪念苍蓝鮟鱇革命（复刻版）活动举办而制作的鮟鱇锅风格勋章！其如同味道般独特的设计引发了大量吐槽！",
        "イベント中-蕾綻ぶ季節に-にて獲得": "可在“花蕾绽放的季节”活动中获得",
        "蕾綻ぶ季節にの報酬勲章": "“花蕾绽放的季节”的奖励勋章",
        "「風雲が立つ」の証": "“风云突起”的证明",
        "示された憧れ": "展现的憧憬",
        "特殊な形の勲章。人は雲が移動するのを見て初めてそこに風があることを知るが、雲が移動する前に必ずかぜ風が起きている。": "形状特殊的勋章。人们看到云移动后才知道那里有风，但云移动之前，风早已吹起。",
        "夜空に佇む道標を模した、すこし抽象的な勲章。それが果たして灯台か、星明りか、それとも蝋燭の光か、決めるのは所有者自身である。": "仿照伫立于夜空中的路标制作的略显抽象的勋章。它究竟是灯塔、星光，还是烛光，由持有者自己决定。",
        "潜水艦・伊168ガチャ300連の報酬勲章": "潜艇·伊168卡池300连奖励勋章",
        "抽象的な外観のメダルを身につけることは、未来への道は常に一つではないし、一つでなくてもよいという理念を認めることでもある。": "佩戴外观抽象的奖牌，也代表认可通往未来的道路不一定只有一条，也不必只有一条。",
        "指令イベント！夏姫祭スペシャルのイベントにて、獲得できる証": "可在指令活动！夏日战姬祭特别活动中获得的证明",
        "Command Sennkiの証（夏姫祭スペシャル）": "Command 战姬的证明（夏日战姬祭特别活动）",
        "たまにお酒を飲む権利は誰にでもあるし、少女のお気に入りのノンアルコールにも癒し効果がある": "偶尔喝酒是每个人都有的权利，而少女喜欢的无酒精饮料也有治愈效果。",
        "セクシーミッドナイト": "性感午夜",
        "コラボイベント実施中にて、獲得可能！": "联动活动期间可获得！",
        "【花咲き物語】の報酬": "【花开物语】的奖励",
        "迎春祭り記念章": "迎春祭纪念章",
        "イベント「覚ませ!赤い悪夢!」の報酬": "活动“醒来吧！赤色噩梦！”的奖励",
        "大事な仕事を任されたからといって、冷たい飲み物ばかり思ってはいけない": "即使被托付了重要工作，也不能只想着冷饮。",
        "周姫祭‐【周姫祭特別イベント‐ケーキ、作ってくれるかな？】にて獲得できる勲章。ケーキ作りの完成度をアップさせる事で、限定報酬を獲得することができます！": "可在战姬祭—【战姬祭特别活动—能帮我做蛋糕吗？】中获得的勋章。提升蛋糕制作完成度即可获得限定奖励！",
        "Command Sennkiの証（二周年）2nd Anniversaryスペシャルイベントにて、獲得できる記念勲章！Happy Birthday！": "Command 战姬的证明（二周年）可在2nd Anniversary特别活动中获得的纪念勋章！生日快乐！",
        "アンブラ世界事件！指令イベントにて、獲得できる記念勲章。": "可在安布拉世界事件！指令活动中获得的纪念勋章。",
        "Command Sennkiの証（アンブラ世界事件）": "Command 战姬的证明（安布拉世界事件）",
        "Command Sennkiの証（アンブラ世界事件 シーズン②）、アンブラ世界事件！指令イベント シーズン②にて、獲得できる記念勲章。": "Command 战姬的证明（安布拉世界事件·赛季②），可在安布拉世界事件！指令活动·赛季②中获得的纪念勋章。",
        "Command Sennkiの証（アンブラ世界事件 シーズン②）": "Command 战姬的证明（安布拉世界事件·赛季②）",
        "『ライザのアトリエ‐シリーズ』とのコラボイベントにて、獲得できる限定勲章！錬金技術とブルースフィアの素材を組み合わせた絆の証し～": "可在与『莱莎的炼金工房·系列』的联动活动中获得的限定勋章！结合炼金技术与蓝星素材的羁绊证明～",
        "『ときめきパールベイ学園』イベント内のex海域をクリア時に獲得できる記念勲章！": "可在【心动珍珠湾学院】活动中通关EX海域时获得的纪念勋章！",
        "ときめきS′BⅡ勲章": "心动S′BⅡ勋章",
        "夢に形があれば、蝶のように軽やかで、雲のように決して手に取ることはできない。": "若梦想有形，它会像蝴蝶般轻盈，如云朵般永远无法触及。",
        "夢の如し": "恍若梦境",
        "【悪霊退散!ハロウィ大冒険】で三つの試練を乗り越えた勇者に与えられる名誉勲章。": "授予在【驱散恶灵！万圣大冒险】中通过三项试炼的勇者的荣誉勋章。",
        "ハロウイン勇者": "万圣勇者",
        "懐かしい夢を見た。【復刻版・カタバミと三つ葉のオークに見守られながら】の報酬": "做了一个怀念的梦。【复刻版·在酢浆草与三叶橡树的守望下】的奖励",
        "第2期Command Sennkiイベントの”朝日の指令”の指令レベル45にて、獲得できる記念勲章！その眩く光る栄光の星は、指揮官の誇り！": "第2期Command 战姬活动“朝日的指令”指令等级45可获得的纪念勋章！这颗耀眼的荣誉之星，是指挥官的骄傲！",
        "新学期イベント実施中にて、獲得可能！": "新学期活动期间可获得！",
        "武装巡りイベント中に、獲得する事ができます": "可在武装巡游活动中获得",
        "武装巡りイベントに参加した証の記念バッチ（期間限定交換可能）": "参加武装巡游活动的证明纪念徽章（限时可兑换）",
        "お祭りバッチ": "祭典徽章",
        "EXモードにて、獲得可能！": "可在EX模式中获得！",
        "EXモードを挑戦したての駆け出し記念勲章！強く更に強くなれるぞ！": "刚开始挑战EX模式的新手纪念勋章！还能变得更强、更强！",
        "駆け出し": "新手",
        "EXモードを挑戦し続けるベテラン記念勲章！最強艦隊まで、もう一息だ！": "持续挑战EX模式的老手纪念勋章！距离最强舰队只差一步！",
        "ベテラン": "老手",
        "愛宕＆高雄イベント実施中にて、獲得可能！": "爱宕＆高雄活动期间可获得！",
        "愛宕＆高雄の初登場イベントを記念して作られた勲章、イベントでのみ獲得する事が可能です。": "为纪念爱宕＆高雄首次登场活动而制作的勋章，仅可在活动中获得。",
        "雨宿り勲章": "避雨勋章",
        "夏姫祭ログイン報酬内で獲得できる記念勲章！カモメ風リボンに、繋ぎ目の巻貝ワンポイント、本体には舵とハイビスカスがベストマッチング！": "可在夏日战姬祭登录奖励中获得的纪念勋章！海鸥风格的丝带搭配连接处的海螺点缀，主体上的船舵与木槿花相得益彰！",
        "夏姫祭ログイン報酬内で獲得できる記念勲章！トンボ風リボンに、繋ぎ目のカブト、クワガタの真夏ポイント、本体には万辺に散りばめたひまわり縁の宝石達が煌びやか！": "可在夏日战姬祭登录奖励中获得的纪念勋章！蜻蜓风格的丝带搭配连接处的独角仙与锹形虫，主体上点缀的向日葵边饰宝石闪耀夺目！",
        "夏姫祭の勲章（トンボ風）": "夏日战姬祭勋章（蜻蜓风格）",
        "御礼ログインイベント実施中にて、獲得可能！": "感谢登录活动期间可获得！",
        "赤オークランドリリースを記念して表彰された勲章！彼女の存在はこうして形になり、後世に引き継がれる…": "为纪念赤·奥克兰发布而表彰的勋章！她的存在就这样化为实体，传承给后世……",
        "短いながらも楽しいひとときを、愛すべき「命の恩人」たちの写真が見守っている。": "虽然短暂，但这段快乐时光被可爱的“救命恩人”们的照片守护着。",
        "素敵な思い出": "美好的回忆",
        "頭にサイコロをつけたある戦姫は、スマホゲームにはまってから、ほとんどの時間を基地内での有料アルバイトに費やしています。": "某位头上顶着骰子的战姬沉迷手机游戏后，把大部分时间都花在基地内的有偿兼职上。",
        "収支が均衡": "收支平衡",
        "子供達の過ごすすべての楽しいクリスマス、背後にサンタクロースの勤勉な努力を欠かすことができません。": "孩子们度过的每个快乐圣诞节，都离不开圣诞老人的辛勤努力。",
        "サンタクロースの証": "圣诞老人的证明",
        "2023年新年限定勲章です。今年もよろしくお願いします": "2023年新年限定勋章。新的一年也请多关照",
        "SBWBシーズン6イベント実施中にて、獲得可能！": "SBWB赛季6活动期间可获得！",
        "ゴールド大艦隊-6": "黄金大舰队-6",
        "シルバー大艦隊-6": "白银大舰队-6",
        "ブロンズ大艦隊-6": "青铜大舰队-6",
        "SBWBシーズン7イベント実施中にて、獲得可能！": "SBWB赛季7活动期间可获得！",
        "ゴールド大艦隊-7": "黄金大舰队-7", "シルバー大艦隊-7": "白银大舰队-7", "ブロンズ大艦隊-7": "青铜大舰队-7",
        "翔鹤大破照": "翔鹤大破照",
        "指令イベント（スペシャル）の報酬を再配布する": "重新发放指令活动（特别）奖励",
        "指令イベント（スペシャル）では未受取の報酬をゲーム内メールに配布させていただきました。ご確認のほどよろしくお願いいたします。": "指令活动（特别）中未领取的奖励已通过游戏内邮件发放，请查收。",
        "敬愛なる指揮官。初購入キャンペーンが再開するため、前回キャンペーンの報酬を補填させていただきます。": "尊敬的指挥官：由于首购活动重新开启，现补发上次活动的奖励。",
        "成就タスク不具合へのお詫び": "成就任务故障致歉",
        "こりゃ臨時内容のじゃ！": "这是临时内容啦！",
        "「赤红唤醒」イベントが終了しましたため、イベントアイテムを回収致します。以下の報酬を補填させていただきます。": "“赤红唤醒”活动已结束，将回收活动道具。现补发以下奖励。",
        "レイドボス報酬": "Raid Boss奖励",
        "お詫び補填": "致歉补偿",
        "【対決！大艦隊作戦夏の篇！】イベントが終了したので、指揮官の大艦隊ランキング報酬を配布させていただきます。": "【决战！大舰队作战·夏之篇！】活动已结束，现发放指挥官的大舰队排名奖励。",
        "尊敬する指揮官たちへ！これは受け取らなかった「対決!大艦隊作戦夏の篇」イベントのミッション報酬となり、イベントがすでに終了しましたので、再配布いたします。": "致尊敬的指挥官们！这是“决战！大舰队作战·夏之篇”活动中未领取的任务奖励。由于活动已经结束，现重新发放。",
        "【大艦隊レイドボス】イベントが終了したので、指揮官のランキング報酬を配布させていただきます。": "【大舰队Raid Boss】活动已结束，现发放指挥官的排名奖励。",
        "指揮官，今回の「世界事件」イベントが終了したため、コインを回収し相当の以下報酬をお送りいたします。": "指挥官，本次“世界事件”活动已结束，我们将回收硬币并发送相应的以下奖励。",
        "コラボ特別ガチャの未使用補填": "联动特别卡池未使用道具补偿",
        "コラボイベントの未使用補填": "联动活动未使用道具补偿",
        "尊敬する指揮官様：\n今回は「世界事件——隠密進軍」イベントが終了しました。\nご所属サーバーが全目標を達成するスピードに応じて、イベントに参加しましたプレイヤーは下記の報酬を配布いたします。\nご確認ください。": "尊敬的指挥官：\n“世界事件——隐秘进军”活动已经结束。\n根据您所在服务器完成全部目标的速度，将向参加活动的玩家发放以下奖励。\n请查收。",
        "尊敬する指揮官様：\n今回は「世界事件——共同改造」イベントが終了しました。\nご所属サーバーが全目標を達成するスピードに応じて、イベントに参加しましたプレイヤーは下記の報酬を配布いたします。\nご確認ください。": "尊敬的指挥官：\n“世界事件——共同改造”活动已经结束。\n根据您所在服务器完成全部目标的速度，将向参加活动的玩家发放以下奖励。\n请查收。",
        "親愛なる指揮官へ、この度はこちらの不備により、大変ご迷惑をおかけしてしまい、誠に申し訳ございませんでした。装備入荷ガチャの確率説明を追加させていただきました上で、メンテナンス時点から遡って、使用回数を調査させていただき、下記の特別艦種選択パックをお詫び補填させていただきました。今後とも弊社アプリをご愛顧くださいますようお願い申し上げます。": "致亲爱的指挥官：本次因我们的疏漏给您带来诸多不便，深表歉意。我们已补充装备进货卡池的概率说明，并追溯调查维护时的使用次数，现发放以下特殊舰种选择礼包作为致歉补偿。今后也请继续支持我们的应用。",
        "「異世界の錬金術士-コラボ特別ガチャ」イベントが終了いたしましたので、未使用の「魔法の呼び鈴」は下記アイテムと交換になり、補填させていただきます。ご確認をお願いいたします。": "“异世界炼金术士－联动特别卡池”活动已结束，未使用的“魔法铃铛”将兑换为以下道具并作为补偿发放，请查收。",
        "コラボイベントが終了しましたため、未使用の調合材料とジェムは下記アイテムと交換になり、補填させていただきます。ご確認をお願いいたします。": "联动活动已结束，未使用的调合材料与宝石将兑换为以下道具并作为补偿发放，请查收。",
        "第6期Command Sennkiイベントにて獲得する事ができる大成の証し、ハロウィン雰囲気たっぷりのフレーム！": "第6期Command 战姬活动可获得的成功证明，充满万圣节气氛的头像框！",
        "ハロウィン指令フレーム": "万圣节指令头像框",
        "「カタバミと三つ葉のオークに見守られながら」イベントにて、限定海域クリアで獲得する事ができるフレーム。「光栄と平和が永遠に続くように」": "可在“在酢浆草与三叶橡树的守望下”活动中通关限定海域获得的头像框。“愿荣光与和平永远延续”",
        "カタバミと三つ葉フレーム": "酢浆草与三叶头像框",
        "『ときめきパールベイ学園』イベントショップにて、交換して獲得できる記念フレーム！": "可在【心动珍珠湾学院】活动商店中兑换获得的纪念头像框！",
        "ときめきS′BⅡフレーム": "心动S′BⅡ头像框",
        "人生は夢のようだ。夢にも価値はあるのかもしれない。": "人生如梦。或许梦也有其价值。",
        "深海回路IIフレーム": "深海回路II头像框",
        "第11期Command Sennkiイベントにて、獲得できるフレーム！": "第11期Command 战姬活动可获得的头像框！",
        "ゲーム大好きのフレーム": "游戏狂热头像框",
        "ブリキの騎士団「ティン・カン・オーダー」のメンバーに授与したフレーム。これさえあれば、あなたもティン・カン・オーダーの一員！": "授予铁皮骑士团“Tin Can Order”成员的头像框。有了它，你也是Tin Can Order的一员！",
        "騎士団「ティン・カン・オーダー」のフレーム": "骑士团“Tin Can Order”的头像框",
        "SBWB8にて、輝かしきNO.1に輝いた指揮官に送られる最強の証しフレーム！皆の手本となり、これからも最強艦隊で世界を守ってくれ！": "SBWB8中授予荣登耀眼NO.1的指挥官的最强证明头像框！请成为大家的榜样，今后也用最强舰队守护世界！",
        "SBWB8にて、頂きを競うNUMVBERSに輝いた指揮官に送られる精鋭の証しフレーム！切磋琢磨を繰り返し、競い合い頂点を狙うは君だ！": "SBWB8中授予荣登顶峰竞争者NUMVBERS的指挥官的精英证明头像框！不断切磋竞争、瞄准顶峰的人就是你！",
        "SBWB8にて、惜しくもAPPROVARに輝いた指揮官に送られる強者の証しフレーム！上にはNUMVBERSに、そしてNO.1が犇めく大海域に挑め！": "SBWB8中授予荣登APPROVAR的指挥官的强者证明头像框！向上挑战NUMVBERS以及NO.1云集的广阔海域吧！",
        "2023年新年限定フレームです。今年もよろしくお願いします!": "2023年新年限定头像框。新的一年也请多关照！",
        "2023年新年限定フレーム": "2023年新年限定头像框",
        "【第三期】美食マスターの道イベントにて、獲得できる限定フレーム！": "可在【第三期】美食大师之路活动中获得的限定头像框！",
        "【ハロウィン限定】最強艦隊イベントの限定のイータ大破を体現したフレーム。個人PT報酬で獲得可能。": "【万圣节限定】最强舰队活动限定、体现伊塔大破状态的头像框。可通过个人PT奖励获得。",
        "イータフレーム": "伊塔头像框",
        "ログインイベント（霜月の宝物）にて獲得できます。ビスマルクを体現した限定フレーム！": "可在登录活动（霜月的宝物）中获得。体现俾斯麦形象的限定头像框！",
        "ビスマルクフレーム": "俾斯麦头像框",
        "【強運少女期間限定ガチャ】の40連確定報酬として獲得できる限定フレーム！サイコロとラッキーボール、時雨らしい一品だ！": "作为【幸运少女限时卡池】40连保底奖励可获得的限定头像框！骰子与幸运球，十分符合时雨的风格！",
        "時雨フレーム": "时雨头像框",
        "第7期Command Sennkiイベントにて獲得する事ができる大成の証し、冬の雰囲気たっぷりのフレーム！": "第7期Command 战姬活动可获得的成功证明，充满冬日气氛的头像框！",
        "冬祭り指令フレーム": "冬祭指令头像框",
        "2021年クリスマスイベントの報酬として入手できる期間限定フレーム！クリスマスを満喫しましょう！": "作为2021年圣诞活动奖励获得的限时头像框！一起享受圣诞节吧！",
        "クリスマスフレーム(2021限定）": "圣诞头像框（2021限定）",
        "【第五期】最強艦隊イベントにて入手可能な限定フレーム。": "可在【第五期】最强舰队活动中获得的限定头像框。",
        "ゼータフレーム": "泽塔头像框",
        "【復刻版】白薔薇の金姫x黒真珠の銀姫期間限定ガチャにて、獲得できる限定フレーム！冬の雪花と小さな光の要素が散りばめられているデザイン。日向ファンには堪らない一品となっている。": "可在【复刻版】白蔷薇金姬×黑珍珠银姬限时卡池中获得的限定头像框！设计中点缀着冬日雪花与微光元素，是日向粉丝不容错过的珍品。",
        "日向の復刻版記念フレーム": "日向复刻版纪念头像框",
        "SBWB-6にて、輝かしきNO.1に輝いた指揮官に送られる最強の証しフレーム！皆の手本となり、これからも最強艦隊で世界を守ってくれ！": "SBWB-6中授予荣登耀眼NO.1的指挥官的最强证明头像框！请成为大家的榜样，今后也用最强舰队守护世界！",
        "ゴールドフレーム-6": "黄金头像框-6",
        "SBWB-6にて、頂きを競うNUMVBERSに輝いた指揮官に送られる精鋭の証しフレーム！切磋琢磨を繰り返し、競い合い頂点を狙うは君だ！": "SBWB-6中授予荣登顶峰竞争者NUMVBERS的指挥官的精英证明头像框！不断切磋竞争、瞄准顶峰的人就是你！",
        "シルバーフレーム-6": "白银头像框-6",
        "SBWB-6にて、惜しくもAPPROVARに輝いた指揮官に送られる強者の証しフレーム！上にはNUMVBERSに、そしてNO.1が犇めく大海域に挑め！": "SBWB-6中授予荣登APPROVAR的指挥官的强者证明头像框！向上挑战NUMVBERS以及NO.1云集的广阔海域吧！",
        "ブロンズフレーム-6": "青铜头像框-6",
        "2022乙女座のフレーム": "2022处女座头像框",
        "2022いて座のフレーム": "2022射手座头像框",
        "2022やぎ座のフレーム": "2022摩羯座头像框",
        "2023みずがめ座のフレーム": "2023水瓶座头像框",
        "Command Sennki（二周年）2nd Anniversaryスペシャルイベントにて、獲得できる大成の証し！これからもよろしくね！": "可在Command 战姬（二周年）2nd Anniversary特别活动中获得的成功证明！今后也请多关照！",
        "2nd Anniversaryのフレーム": "2nd Anniversary头像框",
        "黒猫のフレーム": "黑猫头像框",
        "まわりに合わせてペースを落とすのは、人のためだけでなく、自分のためだけではありません。": "配合周围的人放慢脚步，不只是为了别人，也是为了自己。",
        "「風と共に」フレーム": "“与风同行”头像框",
        "迷彩ディスコのフレーム": "迷彩迪斯科头像框",
        "夜空に輝く代償を、星たちも自分以外の者に知られたくないでしょう。": "在夜空中闪耀的代价，星星们也不想让别人知晓吧。",
        "輝く星": "闪耀之星",
        "アンブラ世界事件！指令イベントにて、獲得できるフレーム。": "可在安布拉世界事件！指令活动中获得的头像框。",
        "アンブラ世界事件のフレーム": "安布拉世界事件头像框",
        "忘れ難い人": "难忘之人",
        "アンブラ世界事件！指令イベント シーズン②にて、獲得できるフレーム。": "可在安布拉世界事件！指令活动·赛季②中获得的头像框。",
        "アンブラ世界事件のフレーム シーズン②": "安布拉世界事件头像框·赛季②",
        "かつては不器用だった少女も、今や一人前になった。【復刻版・カタバミと三つ葉のオークに見守られながら】の報酬": "曾经笨手笨脚的少女如今也已独当一面。【复刻版·在酢浆草与三叶橡树的守望下】的奖励",
        "夜中に咲く華": "深夜绽放的花",
        "ハロウィンは誰もが楽しい思い出を作るべきだ。ハロウィンおめでとう!": "万圣节就该让每个人都留下快乐回忆。万圣节快乐！",
        "ハロウイン大冒険": "万圣大冒险",
        "第5期Command Sennkiイベントにて獲得する事ができる大成の証し、グローウォームの着せ替えを体現したフレーム。": "第5期Command 战姬活动可获得的成功证明，体现萤火虫换装的头像框。",
        "歓声アンコールフレーム": "欢呼·安可头像框",
        "サービス開始500日記念フレーム！これからも【蒼藍の誓い-ブルーオース】をよろしくお願いいたします。": "上线500日纪念头像框！今后也请继续支持【苍蓝誓约-蓝色奥斯】。",
        "ログインイベント（長月の宝物）にて、獲得できる限定フレーム！金色の基調に紅葉、一足お先に秋を満喫！": "可在登录活动（长月的宝物）中获得的限定头像框！以金色为基调搭配红叶，提前享受秋日！",
        "黄金の秋フレーム": "黄金之秋头像框",
        "呼，被辱骂着……这种心灵的痛楚，多么令人心·情・舒・畅♪": "呼，被辱骂着……这种心灵的痛楚，多么令人心情舒畅♪",
        "所以这些只有我知道公主的好・地・方，果然还是应该展现给大家看才对嘛。": "所以这些只有我知道的公主优点，果然还是应该展现给大家看看才对嘛。",
        "（“应该没什么问题吧(<ゝω·)☆ ”……）": "（“应该没什么问题吧(<ゝω·)☆”……）",
        "うっわ～今日は凄い天気だね、波もやや高いわ。": "哇～今天的天气真不错，海浪也有点高呢。",
        "<color=#f14949>今の段階</color>では、<color=#f14949>マイクロ装置を排除する手段がない</color>ため、どうか、心して行動してくれ。": "目前<color=#f14949>没有排除微型装置的手段</color>，所以请做好心理准备后行动。",
        "ちなみに<color=#f14949>双方とも、耐久値が四分の一以下になると</color>、<color=#f14949>大破</color>状態になるわ。": "顺带一提，<color=#f14949>双方耐久值降至四分之一以下时</color>，都会进入<color=#f14949>大破</color>状态。",
        "这边的是用来在正月期间装饰屋子的装饰品。有放在玄关前面的门松、挂在门上的传统玉飾り、还有漂亮的輪飾り……": "这是正月期间用来装饰房屋的装饰品。有摆在玄关前的门松、挂在门上的传统玉饰，还有漂亮的轮饰……",
        "抱紧～（キュン～）": "抱紧～（心动～）",
        "あらら、もうしばらくすると<color=#f14949>夜</color>だね……これじゃあ、基地航空隊による<color=#f14949>支援</color>が<color=#f14949>難しくなる</color>わ……": "哎呀，再过一会儿就是<color=#f14949>夜晚</color>了……这样一来，基地航空队的<color=#f14949>支援</color>会变得<color=#f14949>困难</color>……",
        "<color=#f14949>夜</color>になったら、<color=#f14949>飛行機</color>は<color=#f14949>離着陸</color>するのはとても<color=#f14949>危険</color>、だから<color=#f14949>空母は、夜戦中、攻撃できない</color>のよ。": "进入<color=#f14949>夜晚</color>后，<color=#f14949>飞机</color>起降非常<color=#f14949>危险</color>，所以<color=#f14949>航空母舰在夜战中无法攻击</color>。",
        "ふんっ！【切黑幕】": "哼！【切黑幕】",
        "……くっ！【切黑幕】": "……可恶！【切黑幕】",
        "真是蠢到让我哑口无言了。（馬鹿すぎて呆れた）": "真是蠢到让我哑口无言了。",
        "まったく、勝手だね、あの子……【切黑幕】": "真是的，那孩子太任性了……【切黑幕】",
        "指揮官くん～今回の状況はちょっときびしくなっちゃったわ～": "指挥官～这次的情况变得有点棘手了～",
        "呼呼～真是蠢到让我哑口无言了啊！ふふ、馬鹿すぎて呆れたってね": "呼呼～真是蠢到让我哑口无言了啊！呵呵，就是蠢得让我无语。",
        "——翔鹤姐妹的宿舍": "——翔鹤姐妹的宿舍",
        "翔鹤？": "翔鹤？",
        "衣服！翔鹤小姐！你的衣服——": "衣服！翔鹤小姐！你的衣服——",
        "【特效：翔鹤大破】": "【特效：翔鹤大破】",
        "戦鬼だ。你以前没有见过它们，对吧？": "是战鬼。你以前没有见过它们，对吧？",
        "小莫里弄脏了居酒屋，还弄坏了翔鹤和宁海的衣服，就罚做一星期的苦力——": "小莫里弄脏了居酒屋，还弄坏了翔鹤和宁海的衣服，就罚做一星期的苦力——",
        "【翔鹤往左移出屏幕】": "【翔鹤向左移出屏幕】",
        "是、是的，翔鹤姐姐……大人。": "是、是的，翔鹤姐姐……大人。",
        "この前と同じ、先にチームに配置された二人がAチームで、三番、四番はＢチームで、最後の二人がＣチームという感じにチームわけするよ。": "和之前一样，先配置到队伍的两人是A队，第三、第四人是B队，最后两人是C队，就这样分队。",
        "でもね、今回の敵は戦艦～、前回の空母よりちょっとしぶといかもしれないわ。": "不过，这次的敌人是战列舰～可能比上次的航空母舰更难对付。",
        "那也不是人家的错吧？！人家只是突然看到久违的温柔体贴而且胸部和大腿全是极品中的极品的翔鹤大人想稍微肌肤相亲一下而已！": "那也不是人家的错吧？！人家只是突然看到久违的温柔体贴、胸部和大腿都是极品中的极品的翔鹤大人，想稍微亲近一下而已！",
        "え、本気なの？わあ〜…素敵です！ありがとう指揮官さん。スージーも嬉しそうですね、えへへ。": "诶，真的吗？哇～……太棒了！谢谢你，指挥官。苏西看起来也很开心呢，嘿嘿。",
        "……指揮官さまでしたか。<指揮官：この指輪をあなたに>": "……原来是指挥官大人。<指挥官：这枚戒指送给你>",
        "あらら、これをあたいにくれるの?ふふ、ありがとね〜。": "哎呀，要把这个送给我吗？呵呵，谢谢你～。",
        "指揮官、これを私に？ありがと〜！じゃ……私もお返しにプレゼントしよっかな。よいしょっと。はい、左手と右手。どっちだと思う？": "指挥官，这个给我？谢谢～！那么……我也送你一份回礼吧。嘿咻。来，左手和右手，你猜是哪只？",
        "それじゃあ、このプレゼントはもらっておくね。ありがとう〜。": "那么，这份礼物我就收下啦。谢谢～。",
        "え、え、ええええ？！指揮官、本当？これを、わ、私に？": "诶、诶、诶诶诶？！指挥官，真的吗？这个、送给我？",
        "も〜、し〜き〜か〜ん〜さ〜ん〜？またそんなことをして、サボろうとして～でもまぁ、今回は見逃してあげます。なんせ私のためにしてくれたことなんですから。": "真是的～指～挥～官～？又做这种事，想偷懒吗～不过这次就放过你吧。毕竟这是为我做的嘛。",
        "えへへ、指揮官さんの気持ち、ちゃんと受け取りました。でも、一回だけですよ、指揮官さんがこれ以上サボらないよう、これからず〜っと、おそばにいますね！": "嘿嘿，我好好收到了指挥官的心意。不过只能这一次哦。为了不让指挥官继续偷懒，我今后会一直陪在你身边！",
        "えっ？！これをラフィーに？": "诶？！这个要送给拉菲？",
        "えっ、なになに？わぁ〜、綺麗な指輪、ありがとう。": "诶，什么什么？哇～好漂亮的戒指，谢谢。",
        "いやいや、正しくは君が俺の下に来たんだけど…": "不不，准确来说是你来到我手下才对……",
        "よしよし、いい子ね。頑張り屋で人の話もちゃんと聞いて、しかも甘え上手。\n私の下に来ることができて本当に幸運だわ。": "乖乖，真是个好孩子。努力又听话，而且很会撒娇。\n能来到我手下，真是你的幸运。",
        "こちらこそ、優しくて綺麗な先輩に出会えて、この上なく幸いに存じます！": "我才是，能遇到温柔漂亮的前辈，真是无比幸运！",
        "おのれ…": "可恶……",
        "覚えてろよ！！！": "你给我记住！！！",
        "毎度ありがとうございました〜": "感谢您每次光临～",
        "…あんな淫らなものためにこれほど必死になるとは、本当に見苦しいです。\n外に知られたら、きっと世間の笑い草になるでしょう。": "……为了那种淫秽之物如此拼命，真是难看。\n要是被外界知道，肯定会成为世人的笑柄。",
        "こんにちは、姫様。": "您好，公主殿下。",
        "赤城でいいですよ。エディンバラ殿、この度加賀の購入した新刊を全て処理できたのは、\n貴殿の助力あってのことです。誠にありがとうございます。": "叫我赤城就好。爱丁堡阁下，这次能处理完加贺购买的全部新刊，\n全靠您的帮助。非常感谢。",
        "ふふ、遠慮しないで下さい。基地内には純粋な子供がたくさんおりますから、\nこういうアダルトな印刷物の流通を阻止することは、善い行いになりますよね。": "呵呵，请别客气。基地里有很多纯真的孩子，\n阻止这类成人印刷物流通，也算是件好事，对吧。",
        "それに、加賀様はきちんと「罪を償った」ことでしょう。\nもしよろしければ、彼女への処罰をなるべく軽くしてはいただけませんか？": "而且，加贺大人应该已经好好“赎罪”了。\n如果可以的话，能否尽量减轻对她的处罚？",
        "…神様もお金次第っていうから、善処はします。": "……都说神明也看钱办事，我会尽量处理。",
        "…ご意見、感謝します。\nこの頭痛が収まったら慎重に検討しておきます。": "……感谢您的意见。\n等这阵头痛缓解后，我会慎重考虑。",
        "はい。それでは、今回の件はこれでおしまいです。お先に失礼致します。\nもしかすると、別の迷える子羊が「罪を償いたい」と考えているかもしれませんし。": "好的。那么这次的事情就到此为止。先告辞了。\n说不定还有其他迷途羔羊正想着“赎罪”呢。",
        "貧しい生活は確かに嘆かわしいことですが、\nただ、どうして会うやいなや…": "贫困的生活确实令人叹息，\n不过，为什么才一见面就……",
        "ああ、あなたも「贖宥」をしたいのですね。": "啊，您也想要“赎罪”吧。",
        "げっ！！！申し訳ございません！\n本当に手持ちがなくて、どうか呪わないでください！": "啊！！！对不起！\n我真的身无分文，请千万不要诅咒我！",
        "ふふ、ご安心ください。\n呪いなんてこの世界に存在しませんよ。": "呵呵，请放心。\n诅咒这种东西并不存在于这个世界。",
        "はい。そういうわけですから、どうかご安心ください。\n良識のある方は、普通に暮らしていけばいいのですから。": "好的。正因为如此，请放心。\n有常识的人只要正常生活就好。",
        "では、あなたが平穏な日々を送れることを祈っております。": "那么，祝愿您能过上平静的日子。",
        "噂よりずっとマシな人だったけど、\nやっぱり少し厳しい性格のようですね。": "虽然比传闻中好得多，但性格果然还是有点严厉呢。",
        "今日の収穫は、\nあまり芳しくないですね…": "今天的收获，\n似乎不太理想……",
        "ははは！\n罪を背負う人が少ないってことは、素晴らしいことではないか？": "哈哈哈！\n背负罪孽的人少，不正是件好事吗？",
        "あらまあ…すごく、ドス黒いお方ですね。": "哎呀……您真是位非常腹黑的人呢。",
        "一応信仰を持つ者同士だから、そう警戒しないでくれ。\nまあ、その対象は違うけど——": "我们好歹都是有信仰的人，不必如此戒备。\n不过，我们信仰的对象不同就是了——",
        "それでは邪魔するぞ。ところで、あなたはいつも質素な物にしか手を出していない\nようだけど、それは修行の一環、なのかな？": "那我就打扰了。话说回来，你似乎总是只买朴素的东西，\n这是修行的一环吗？",
        "いいえ、単に安いからです。": "不，只是因为便宜。",
        "あははは！驚くほど真っ直ぐな言葉だな。天使たちに捧げるプレゼントに\n多少は影響を及ぼすが、この機会に、ここの名物をご馳走させてもらおう。": "啊哈哈哈！真是直白得令人惊讶。虽然多少会影响献给天使们的礼物，\n但就趁这个机会，让我请你尝尝这里的特产吧。",
        "あっ、わたくしの言葉が誤解させてしまったようですね。\nごめんなさい、ただ安価な物で十分ってことですよ。": "啊，看来我的话让您误会了。\n抱歉，我只是说便宜的东西就足够了。",
        "…わかってくれるならそれでいい。": "……你能理解就好。",
        "うむ、やはり我が友には人を見る目があると言うべきか。\n「君は彼女とそりが合わないだろうから、距離を取った方が両方のためだ」と彼が言ってたな。": "嗯，看来应该说我的朋友确实有识人之明。\n他曾说：“你和她性格不合，保持距离对双方都好。”",
        "指揮官様の言葉は、時々極端すぎます。\nヒッパー様でも「罪を償いたい」時があるでしょうに。": "指挥官大人的话有时太极端了。\n就算是希佩尔大人，也会有想要“赎罪”的时候吧。",
        "無料か？": "免费吗？",
        "それは無理です。罪とは、他人に損をさせてしまったものです。\nですから、お金を払わずにそれを手放したいというのは、理にかなっていません。": "那是不可能的。罪就是让他人蒙受损失。\n所以，不付钱就想摆脱罪责，是没有道理的。",
        "それは無理です。罪とは、他人に損をさせてしまったものです。\nですから、お金を払わずにそれを手放したいというのは、理にかないません。": "那是不可能的。罪就是让他人蒙受损失。\n所以，不付钱就想摆脱罪责，是没有道理的。",
        "……\n（ドン——）": "……\n（咚——）",
        "あっ、いいえ、そういうことじゃないの。\nこちらに来る前から同じようなものだから、多分何か心理的な要因かな？": "啊，不，不是那个意思。\n在来到这里之前就差不多是这样了，可能是某种心理因素吧？",
        "…彼女と接してわかったけど、それは別に妄想癖とか深刻な問題があるわけでもなく、\nただただ——": "……和她接触后我明白了，这并不是妄想癖或什么严重问题，\n只是单纯地——",
        "後輩くん、あなただって仕事が好きってわけじゃないでしょう？\n好きでもないのにそこまで一生懸命だなんて、もしかしてMなの？": "后辈君，你也不是喜欢工作吧？\n明明不喜欢却这么努力，难道你是M？",
        "世の中の勤勉に働く人間全員に謝れ！\nあと、そのすぐに脱線する考え方をなんとかして仕事に取り掛かれ！": "快向世上所有勤奋工作的人道歉！\n还有，赶紧纠正你这动不动就跑题的想法，开始工作！",
        "それでは、あとはお願いします。緊急な用件ではありませんが、指揮官が帰ってきたらそれを使いますので、可能な限り、早急に完成してもらえると助かります。": "那么，接下来就拜托了。虽然不是紧急事项，但指挥官回来后要用到它，如果能尽快完成就帮大忙了。",
        "わかった。安心して任せて。": "知道了，放心交给我吧。",
        "まあ〜クールビューティーって本当にいいね。背も高くて仕事もできて、あたしもあんな風になりたいな。\nまあ、どちらかというとやはり平海ちゃんの方がああなる可能性は高いけど——": "哇～冷酷美人真好啊。个子高又能干，我也想变成那样。\n不过要说可能性，果然还是平海更有机会变成那样吧——",
        "もう、だめ！\n思い返すだけで…": "不行了！\n光是回想起来就……",
        "ケダモノ様！\n直ちにあなたのそばに参りますわ！": "野兽大人！\n我马上就到您身边！",
        "…行っちゃった。": "……走掉了。",
        "変なことばかり言ってるし…あんな変態にしつこく絡まれる指揮官もご愁傷さまだね。\nまあ、変態の考えなんて理解しようとしても意味がないんだけど。": "他净说些奇怪的话……被那种变态纠缠的指挥官也真够倒霉的。\n不过，试图理解变态的想法也没有意义。",
        "何があったの！？": "发生什么事了！？",
        "む、虫が！\n窓を開けたら飛んできて——": "虫、虫子！\n打开窗户后它就飞进来了——",
        "やあああ！！": "呀啊啊啊！！",
        "…それ、普通の蛾じゃない？\nゴキブリならまだしも、ただの蛾にびびるなんて。": "……那不是普通的飞蛾吗？\n蟑螂也就算了，居然被一只飞蛾吓到。",
        "ゴキブリは顔にぶつかってこないでしょう！\nいくらキモくても——": "蟑螂又不会撞到脸上！\n再恶心也不能——",
        "やああ！！": "呀啊啊！！",
        "ゴキブリにも飛行できる種類がいるって言いたいんだけど…まあいいよ。": "我只是想说，也有能飞的蟑螂……算了。",
        "この書類はもう用済みかな？使わせてもらうよ。": "这份文件已经没用了吧？那我就拿来用了。",
        "へっ！（叩く）": "嘿！（敲击）",
        "ついでに後始末も…": "顺便把后续收拾工作也……",
        "面倒事を押し付けおいて涼しい顔をするつもり？！": "把麻烦事推给别人后，还想装作若无其事？！",
        "さあさあ、考えすぎもよくないよ、とにかく、一緒に激射を叫ぶことから始めよ！\nせーの——": "好啦好啦，想太多也不好。总之，先一起喊出激射开始吧！\n预备——",
        "はい、問題解決だよ。": "好了，问题解决。",
        "「どうせ今日の仕事はもう終わったから」というあまりに関係のない考えを抱きながら、\n俺はボルチモアの部屋を一通り片付けた。": "我抱着“反正今天的工作已经结束了”这种完全无关的想法，\n把巴尔的摩的房间彻底收拾了一遍。",
        "ありがとうございます、指揮官。\nおかげで助かりました。": "谢谢您，指挥官。\n多亏了您帮了大忙。",
        "疲れ果てた社畜が帰宅した時と同じ状態か…": "和精疲力竭的社畜回家时一样的状态吗……",
        "それに、こんなことを堂々というのも恥ずかしいから、\n指揮官に助けを求めるのが最適解なの。": "而且，堂堂说出这种事也很丢脸，\n所以向指挥官求助才是最佳选择。",
        "それって洗濯ができないでしょ？\n女将、ウイスキーを頂戴。": "那样不是没法洗衣服了吗？\n老板娘，给我来杯威士忌。",
        "いやん～♡もうダメ～\n可愛すぎて我慢できない！抱っこしてあげるね！": "讨厌～♡不行了～\n太可爱了，忍不住了！来，让我抱抱！",
        "最後まで付き合ったことに馬鹿らしさを覚えたけど、\nこんなサービスを受ることができて俺はすごく満足している。": "虽然觉得陪到最后实在有点傻，\n但能享受到这样的服务，我非常满足。",
        "あっ、一応これ全部君の仕事だからな。\nつべこべ言わずにささっと終わらせてくれ。": "啊，姑且说一句，这些全都是你的工作。\n别嘀嘀咕咕了，赶快做完。",
        "ええっ～～？うまい汁だけ吸って逃げるの？\n後輩くんって極悪非道だね。": "诶～～？只想占便宜然后逃跑吗？\n后辈君真是恶劣至极。",
        "このダンケルクは、基地に赴任するやいなや先輩と名乗り始めた変人\n（その服装も含めて）だ。": "这个敦刻尔克是个一到基地就自称前辈的怪人，\n（包括她那身服装也是如此）。",
        "お久しぶり、サンディエゴ姉さん。": "好久不见，圣地亚哥姐姐。",
        "本当に久しぶりだね。配属が決まる前にいきなり音信不通になったから、何事かと思ったよ。\n機密任務に就いたかと思ったら、結局ただの補習とはな～": "真是好久不见。你在确定分配前突然失去联系，我还以为出了什么事。\n本以为你接到了机密任务，结果竟然只是补习啊～",
        "あ…そういうことになっているのか…\nどうせ私がバカだから、試験で赤点を取っても何の不自然も無いってこと？あはははは…": "啊……原来是这么回事……\n反正我很笨，所以考试不及格也没什么奇怪的，对吧？啊哈哈哈哈……",
        "まあ…そういうことにしとこっか。それより～！\nせっかく火砲の話に付き合ってくれる人がいるんだから、気が済むまで話そうよ。": "嗯……就当是这么回事吧。先不说这个～！\n难得有人愿意陪我聊火炮，我们聊到尽兴吧。",
        "おや？\n強者揃いの基地だって聞いたけど、話し相手がいないの？": "咦？\n听说这是强者云集的基地，难道没有聊天对象吗？",
        "それに、たとえ私がこれらの春画を全て灰燼に帰そうとしましても、\nあくまで加賀の私物ですから、できるのはせいぜい差し押さえることまでです。": "而且，就算我想把这些春画全部烧成灰烬，\n它们毕竟是加贺的私人物品，我最多也只能扣押。",
        "もし主殿がもっと公正であれば、衆人の目の前でいかがわしい品の数々を処分することで、\nきっと加賀とその一味をを大いにおびえさせることができるでしょう。もっと公正であれば。": "如果主上更加公正，公开处理这些可疑物品，\n一定能让加贺及其同党大为畏惧。要是能更公正一些的话。",
        "どうしてですか？": "为什么？",
        "しかも4連装10基だって。\n別に彼女の悪口を言うつもりはないけど、この基地はそういう変人が超多いの。": "而且还是10座四联装炮。\n我不是想说她坏话，但这个基地里这种怪人真的特别多。",
        "…オークランドよ、結局、世が乱れるのを見て、\nあなたはなぜ激射の道を広げようとしないの？": "……奥克兰，看到世道混乱至此，\n你为什么还是不想推广激射之道？",
        "だって、私は単に火砲が好きなだけだもん！\n姉さんみたいに人に布教するのは恥ずかしすぎるよ！": "因为我只是单纯喜欢火炮而已！\n像姐姐那样向别人传教，实在太羞耻了！",
        "何よ！\n火砲の数を揃えて、パパパッと連射さえできたらいいじゃない？": "怎么了！\n只要备齐火炮，啪啪啪地连续射击不就好了吗？",
        "はいはい、別に悪いとは言ってないでしょ。\nそうだ、再会のお祝いとして、一緒にぶっ飛ばしに行かない？": "好了好了，我又没说这样不好。\n对了，为庆祝重逢，要不要一起去轰个痛快？",
        "OK。\nここの訓練施設は一流だから、夜までガンガン行こう。": "OK。\n这里的训练设施是一流的，尽管练到晚上吧。",
        "しまった…": "糟了……",
        "結局出禁まで食らちゃった…\nごめん、来たばかりなのに。": "结果竟然被禁止入内了……\n抱歉，明明才刚来。",
        "大丈夫大丈夫。\n激射の心をちゃんと持てば、どんなことも何とかなるよ。": "没事没事。\n只要怀有激射之心，什么事都总能解决。",
        "…そうだね。\n実を言うと出禁を食らったのは初めてじゃないし、きっと何とかなるよね。": "……说得对。\n说实话，被禁止入内也不是第一次了，总会有办法的。",
        "リトライ？": "重试？",
        "遠路お越しいただきましてありがとうございます。はじめまして、\n私はオークランドの同僚であり、友人でもあるクイーン・エリザベスと申します——": "感谢您远道而来。初次见面，\n我是奥克兰的同事兼朋友，名叫伊丽莎白女王——",
        "ちなみにエリザベスはクイーンとか女王とか呼ばれるのが嫌いだから、\nその辺は要注意ね。": "顺带一提，伊丽莎白不喜欢被称作Queen或女王，\n这点要特别注意。",
        "…補足ありがとうございます。\nオークランド「さん」。": "……感谢补充说明。\n奥克兰“小姐”。",
        "なんでいきなり怒るの？！": "为什么突然生气？！",
        "怒るに決まってるじゃない！いいからそこでビスケットでも食べてなさい。\n自分でサンディエゴさんと話すから。": "当然会生气啊！你就在那里吃点饼干吧。\n我自己去和圣地亚哥小姐谈。",
        "（ああ、空が青いわ…）": "（啊，天空真蓝……）",
        "えっ？\nいや…部外者である私がそんなことしたら、オークランドに悪いでしょう。": "诶？\n不……身为外人的我那样做的话，奥克兰会很为难吧。",
        "んー？別にいいよ。激射なんてわかりにくいし、私はパパパッと砲弾を撃つだけでいいよ。\n（もぐもぐ）": "嗯？没关系啦。激射什么的太难懂了，我只要啪啪啪地发射炮弹就好。\n（嚼嚼）",
        "実のところ、オークランドは自由派なんだよね。\n激射の一種の形としてはありだけど、その全てを受け止められるかどうかは疑問だな。": "实际上，奥克兰属于自由派。\n作为激射的一种形式倒也可以，但能否接受它的全部内容就不好说了。",
        "だから、私の衣鉢を継ぐ後継者には、あなたが一番相応しいよ。\nこれから精進していけば、きっと激射一門の未来を担う人間になれる！": "所以，最适合继承我衣钵的人就是你。\n今后继续精进，你一定能成为肩负激射一门未来的人！",
        "私はいいから！": "我就不用了！",
        "ウェルカム！": "欢迎！",
        "あらら、さっきはこっちに合わせてくれたのに、どうしてやめちゃったの？\n後輩くん、中途半端は一番よくないことだよ。": "哎呀，刚才还配合我们，为什么突然停下来了？\n后辈君，半途而废是最不好的行为哦。",
        "…あなたね…とっくに言ってやりたかったんだけど、普通にしゃべることができるなら、\nもう指揮官をからかうのをやめてあげたら？あの人本気で怖がっているよ。": "……真是的……我早就想说了，既然你能正常说话，\n就别再捉弄指挥官了吧？那个人是真的害怕了。",
        "はいはいわかった。\n説教するつもりだったけど、さらに興奮しそうだからもうやめる。": "好好，知道了。\n本来想训你一顿，但感觉会越来越兴奋，还是算了。",
        "…こんなこと言っては失礼かもしれませんが、寧海様、\nあなたではわたくしを興奮させるのは無理だと思います。": "……这么说可能有些失礼，但宁海大人，\n我觉得您无法让我兴奋起来。",
        "あたしの背が低いから舐めてるわけ！？\n上等！模擬戦でもやる！？": "你是因为我个子矮就小看我吗！？\n很好！要不要来场模拟战！？",
        "そんな乱暴にならないでください。別に身長は関係ありません。Sとは、天より授かった尊い特質なので、\nにらめっこしながら大声出す程度でごまかせるものではありません。": "请不要这么粗暴。和身高没有关系。S是上天赐予的珍贵特质，\n不是一边瞪眼一边大声喊叫就能糊弄过去的。",
        "うちの艦隊でそんな天分に恵まれているのは、せいぜいシェーア様しかいません。\nしかし彼女でも、人の内臓を自在に弄ぶようなケダモノ様の覇気に比べたら…はあ、はあ…": "我们舰队中拥有这种天赋的，最多也只有舍尔大人。\n但即使是她，与能随意玩弄他人内脏的野兽大人的霸气相比……哈、哈……",
        "もう死んだから、安心して。": "它已经死了，放心吧。",
        "ごめんなさい。\n生き物をあんなふうに叩くなんて考えると…なんかキモいですので。": "对不起。\n一想到要那样拍打活物……就觉得有点恶心。",
        "いかにも面倒くさい小娘のリアクション！": "真是麻烦小姑娘才会有的反应！",
        "しばらく後——": "过了一会儿——",
        "（もぐもぐ）本当？\n蛾を叩いただけだから、そんなにもらっちゃっても悪いし、やっぱいいよ。": "（嚼嚼）真的吗？\n我只是拍死了一只飞蛾，收这么多不太好，还是算了。",
        "遠慮は無用です——その代わり、\n今日のことは、どうか秘密にしてください。": "不必客气——不过作为交换，\n今天的事情请务必保密。",
        "（うむ。このあたりで手を引くか。\nこれ以上探ったら怒られるかもしれないし）": "（嗯，就调查到这里吧。\n再继续探究下去，说不定会惹人生气）",
        "まあまあ、これでも一応ご馳走になったから、\nもし何か困ったことがあったら、遠慮なく言って。": "好啦好啦，毕竟我也受到了款待，\n以后遇到什么困难尽管告诉我。",
        "お気遣いありがとうございます。": "感谢您的关心。",
        "遠慮しなくていいって。\n妹がもう一人増えた感じだし、大したことないよ。": "都说不用客气了。\n就像多了一个妹妹一样，没什么大不了的。",
        "（あ〜お姉さんの身長じゃないって突っ込んでこないのね。いい子いい子）": "（啊～你没有吐槽姐姐的身高嘛。真乖真乖）",
        "それじゃ、そろそろ仕事に戻るから、ご馳走さま〜": "那么，我差不多该回去工作了，多谢款待～",
        "自分のこれまでの人生を振り返ってみると、\nなんだか波瀾万丈だったと思う。": "回顾自己至今的人生，\n总觉得真是波澜壮阔。",
        "誰もが経験したことがあるようなありふれた子供時代。士官学校に入学して、\n襲撃に遭って九死に一生を得る。そしてたくさんの可愛い女の子たちの上官に…": "每个人都经历过的平凡童年。进入军官学校，\n遭遇袭击后九死一生。然后成为许多可爱女孩子的上司……",
        "うむ、戦闘要素のあるラブコメに見られる主人公のテンプレだな。": "嗯，这就是带有战斗要素的恋爱喜剧中常见的主角模板。",
        "今女性の部屋でレースのパンツを持っている状態もいかにも主人公っぽく見える。": "现在在女性房间里拿着蕾丝内裤的样子，也很像恋爱喜剧的主角。",
        "でも、こんな様子がリアルな世界で見られたら、\nきっと下着泥棒だと言われ、社会的に終わるだろう。だからこれからは決定的な選択を——": "不过，如果现实世界里出现这种场面，\n肯定会被当成内衣小偷，社会性死亡吧。所以接下来要做出决定性的选择——",
        "あっ…報酬が欲しいなら、少し高いけど…": "啊……如果你想要奖励，虽然有点贵……",
        "まあいいわ…": "算了……",
        "ふっ…": "哼……",
        "ごゆっくりいってらっしゃいませ。": "请慢走。",
        "一時はどうなることかと思いましたけど…\n事なきを得て何よりです。": "我一度还担心会变成什么样……\n能平安解决真是太好了。",
        "でも最後のところはやはり危なかったです。彼女はこういう話題でよく他人と口論になるっていう情報を\n事前に入手できなければ、なにか失礼な返答でもしたら…": "不过最后还是很危险。如果事先不知道她经常因为这种话题与人争论，\n万一回答得失礼一些……",
        "…しても、大したことにはならないでしょう。\n彼女はジョージ5世姉さんと同じ、心が広い人間ですから。": "……就算那样，也不会有什么大事。\n她和乔治五世姐姐一样，是个心胸宽广的人。",
        "よし、これでプリンス・オブ・ウェールズを働かせることができた。\n働かざる者食うべからずっていうから、引きこもってもいいけど、仕事はやってもらう。": "好，这样就能让威尔士亲王工作了。\n俗话说不劳动者不得食，虽然可以窝在家里，但工作还是得做。",
        "…って、一人で女子寮に入るのは流石に落ち着かないな。\n早く出よう。": "……不过，一个人进女生宿舍果然还是让人坐立不安。\n赶紧出去吧。",
        "ふふ、あなたはそれで満足なのかもしれないけど、冷遇されたって思う人もいるわよ。\nまあ、今後のことは後で考えるようにするわ。": "呵呵，你可能对此很满足，但也有人会觉得自己受到了冷落。\n至于今后的事，之后再考虑吧。",
        "それでは、上官閣下。あなたの悩みを解決したお礼として、\n私が今直面している危機について、話を聞いてくれる時間ぐらいはあるでしょう？": "那么，上官阁下。作为解决您烦恼的回礼，\n您总该有时间听听我目前面临的危机吧？",
        "俺のできる範囲ならな…": "只要在我能力范围内……",
        "慎重ね。まあ、だからこそあなたに頼んでも大丈夫だって思ってるの。\nさあ、続きは私の部屋で。": "真谨慎啊。不过正因为如此，我才觉得拜托你没问题。\n来，后续去我的房间谈。",
        "こういう時には、「よっしゃ！女の子の部屋に行けるぜ！」って反応するのが普通なんだと思うけど、\n大人の女性ボルチモアのことだから、本当に何か問題が発生したとか？": "这种时候，正常反应应该是“太好了！能去女孩子的房间了！”吧，\n但考虑到成熟女性巴尔的摩，难道真的发生了什么问题？",
        "部屋の中が散らかってるから、先に片付けておくわ。\nもし変な音が聞こえたら、すぐに入ってきてね。": "房间里有点乱，我先收拾一下。\n如果听到奇怪的声音，就马上进来。",
        "？？\nわかった…": "？？\n知道了……",
        "力を入れて頂戴。\n遠慮する必要はないわ。": "用力一点。\n不必客气。",
        "そうして、よくわからない要望を伝えたボルチモアは自分の部屋に入っていった。": "就这样，提出了莫名要求的巴尔的摩走进了自己的房间。",
        "…これまでの経験から、「もし」という言葉以上に危険なものはない。\n今回も例外じゃないだろう。": "……根据以往经验，没有什么比“如果”这个词更危险。\n这次应该也不例外。",
        "ドン——": "咚——",
        "（棒読み）そうだな。色っぽくて可愛くて大人の魅力がマンサイ。": "（毫无感情）是啊。性感、可爱，充满成熟魅力。",
        "あはは、\n超いい加減～": "啊哈哈，\n真是太敷衍了～",
        "…よく考えてみたら、\n確かにこのようなやり方は効率に欠ける。": "……仔细想想，\n这种做法确实缺乏效率。",
        "彼女が真剣に仕事をやってくれたとしても、大した助けにはならないから、\nこんなことしたってただの時間の無駄使いだ。": "即使她认真工作，也帮不上什么大忙，\n这样做只是在浪费时间。",
        "うむ……\n人間の思考ってやっぱ複雑だな。": "嗯……\n人类的思维果然很复杂。",
        "人の太ももを枕にしながら、\n真剣に何をぶつぶつ言ってるの？": "你枕着别人的大腿，\n却在认真嘀咕什么呢？",
        "おっと、これは失礼。\nいいかと思ってつい…": "哎呀，失礼了。\n我以为可以，就不小心……",
        "そう？ならこのままでも構わないわ。\nやめる理由もないし。": "是吗？那就这样也没关系。\n反正也没有停止的理由。",
        "うむ、その通り——": "嗯，正是如此——",
        "…やっぱりだ。それじゃ、お邪魔——": "……果然如此。那么，打扰了——",
        "げ？！\nドア重すぎんだろ！？さっきの「力を入れる」って、このこと？！": "咦？！\n门也太重了吧！？刚才说的“用力”，就是指这个？！",
        "何かに阻まれているドアを全力で開けたら、\n中には悲惨とも言える光景が広がっていた。": "我使尽全力打开被什么东西堵住的门，\n里面展现出一幅可以称为惨烈的景象。",
        "…「散らかしてる」ってレベルじゃねえよこれ。\n泥棒が入ってもこんなふうにはならないだろう。": "……这已经不是“有点乱”的程度了。\n就算进了小偷，也不会弄成这样吧。",
        "落ち着いてるわね…": "真冷静啊……",
        "くくく、クレイヴンだな。何か用か？": "呵呵，是克雷文啊。有什么事吗？",
        "うん。\nクレイヴンはダンケルクお姉ちゃんに用があるの。": "嗯。\n克雷文找敦刻尔克姐姐有事。",
        "でも、指揮官はどうしていきなり立ったの？\n横にならなくていいの？": "可是，指挥官为什么突然站起来？\n不用躺着吗？",
        "いや……\n教育に悪いというかなんというか…": "不……\n怎么说呢，这对教育不太好……",
        "そうだ。そろそろ起きあがろうと思っただけだよ！\n君はダンケルクに用があるんだろ？細かいことは気にしないでくれ。": "对了，我只是觉得差不多该起身了！\n你不是有事找敦刻尔克吗？别在意这些细节。",
        "うん！\nしきかんの言う通りだね。": "嗯！\n就像指挥官说的那样。",
        "ダンケルクお姉ちゃん、この前植えた種が芽生えたよ。\n時間があったらクレイヴンと一緒に見に行かない？": "敦刻尔克姐姐，之前种下的种子发芽了。\n有时间的话，要不要和克雷文一起去看看？",
        "でも残念、\n後輩くんが私の足を枕にしたから、まだ痺れてて動けないのよ。": "不过很遗憾，\n后辈君把我的腿当枕头，现在还麻着，动不了。",
        "（目くばせ：俺のせい！？）": "（使眼色：是我的错！？）",
        "（目くばせ：えへん～★）": "（使眼色：哼哼～★）",
        "痛いの？": "很痛吗？",
        "心配しなくてもいいよ。しばらく歩くのが大変なだけよ。": "不用担心，只是暂时走路不方便而已。",
        "…正直言えば、後輩くんが責任を持って、私を目的地までおんぶしてくれたら、問題解決じゃない？": "……说实话，如果后辈君负起责任把我背到目的地，不就解决问题了吗？",
        "（目くばせ：謀ったなダンケルク！？）": "（使眼色：你算计我，敦刻尔克！？）",
        "（目くばせ：えへへへ～★）": "（使眼色：嘿嘿嘿～★）",
        "しきかんは、\n責任を持ってくれるの？": "指挥官会负责吗？",
        "…はいはい、わかったよ。でもこれだけはよく覚えておくんだ。いいかい、俺は、\n彼女を運んだだけであって、「責任を持つ」とかはないから、こんな話絶対に言いふらすなよ。": "……好好，知道了。不过这点你要记清楚。听好了，我只是\n把她背过去而已，不是什么“负责”，绝对不许把这种事传出去。",
        "ところで、掃除機の吸引力程度じゃ、\n流石に成人を動かすことができないな。": "话说回来，吸尘器的吸力果然没法移动成年人。",
        "もちろんよ…": "当然……",
        "一時の出来心で掃除機でボルチモアの腕を弄ってみたが、\n彼女の髪が吸い込まれそうになるのをみて、やめることにした。": "我一时兴起用吸尘器碰了碰巴尔的摩的手臂，\n但看到她的头发差点被吸进去，只好停下了。",
        "はい。\nクレイヴンはちゃんと覚えた。": "好的。\n克雷文记住了。",
        "よし。\nやると決めたからには、さっさと行くとしよう！": "好。\n既然决定要做，就赶紧出发吧！",
        "あはは～高い～すごく揺れてる～": "啊哈哈～好高～摇得好厉害～",
        "あん～": "嗯～",
        "もぐもぐ…\nったく…": "嚼嚼……\n真是的……",
        "…えっ、なんで急に元気になったの？\nどういう原理？": "……诶，为什么突然精神起来了？\n这是什么原理？",
        "うーん、なんて言えばいいのか。私としてもずっと今の状態を維持したいんだけど、\n自分の部屋に入った途端、何もかもやる気を失ってしまうの。": "嗯，该怎么说呢。我也想一直保持现在的状态，\n但一进自己的房间，就会对一切都失去干劲。",
        "？家具や建材に問題があるのか？\n有害物質が揮発するとか。": "？是家具或建材有问题吗？\n比如挥发有害物质之类的。",
        "ふふ、いつもしかめっ面をしているくせに、\n後輩くんはやはり「後輩」くんだね。": "呵呵，明明总是一副苦瓜脸，\n后辈君果然还是“后辈”君呢。",
        "まあ、私のような器量の持ち主って、そこらじゅうにいるわけじゃないから、\n後輩くんはとても幸運だよ。": "像我这样有容貌又有能力的人可不是到处都有，\n后辈君真是幸运。",
        "先に口の中のものを食べてから言え。": "先把嘴里的东西咽下去再说。",
        "…どうやら、\n彼女はみかんを全部食べ終わってから話すつもりだ。": "……看来，\n她打算吃完所有橘子后再说话。",
        "あれ、あのみかんって俺のために剥いてくれたんじゃなかったの？\nなんで最初の一房しかくれないんだ？": "咦，那些橘子不是特意为我剥的吗？\n为什么只给我第一瓣？",
        "…最初の一房を俺にくれたのは、\nまさか味見させるつもりじゃなかっただろうな？": "……把第一瓣给我，\n该不会只是想让我尝尝味道吧？",
        "初めて会った時から思ってるけど——": "从第一次见面起我就一直觉得——",
        "俺はリンゴジュースを。": "我要苹果汁。",
        "とにかく、気分が乗ったらまた手伝いに来てほしいわ。\n報酬が欲しいなら遠慮せずに言って。": "总之，如果你有兴致，希望你再来帮忙。\n想要奖励的话尽管说。",
        "ここで報酬を決めろって言われてもな。\n「戦いに勝って、無事に帰ってきてくれ」としか言えないじゃん…": "就算让我在这里决定奖励，我也只能说：\n“赢下战斗，平安回来吧……”",
        "…それで終わり？\nこれだけ？": "……这就结束了？\n只有这些？",
        "な～に？後輩くんは感動しないの？\nああいう、千年の時を超えて何度でもあなたを見つけ出すってのは、疲れることじゃない？": "什么～？后辈君不感动吗？\n跨越千年时光，无论多少次都找到你，这不是很累吗？",
        "……\n（棒読み）ああ。わかった。感動シタヨ。": "……\n（毫无感情）啊，知道了。我感动了。",
        "あはは～超いい加減。\nその「愚かな人間」って顔、ネットで流行ってる猫さんとそっくり。": "啊哈哈～太敷衍了。\n你那张“愚蠢的人类”的脸，和网上流行的猫咪一模一样。",
        "じゃあ、この愉快な会話に、乾杯。": "那么，为这场愉快的谈话，干杯。",
        "どういうことだ？": "什么意思？",
        "あらら、最初からせっかちね。\n後輩くんは、真面目すぎて逆に損しちゃうタイプかも。": "哎呀，一开始就这么急性子。\n后辈君可能是太认真，反而容易吃亏的类型。",
        "一言じゃ言い尽せないよ、姉さん。\n魚雷ばかり搭載するのが格好いいと思ってるやつがいるって信じられる？": "一句话说不完啊，姐姐。\n你能相信有人觉得只装备鱼雷很帅吗？",
        "あいかわらず甘い考え方ね。\nまあ、あんまり頭よくないのもオークランドのかわいいところだけど☆": "你的想法还是一如既往地天真。\n不过，脑子不太灵光也是奥克兰可爱的地方☆",
        "ふんっ！\n射撃練習以外にも、私のやることだってたくさんあるんだから。": "哼！\n除了射击训练，我还有很多事情要做呢。",
        "その顔じゃ、きっと激射がどんなものかわかってないね。\n私が説明してあげるよ。": "看你这表情，肯定还不知道激射是什么吧。\n我来给你解释。",
        "人生で大切な1時間半が無駄に消えると——": "人生中宝贵的一个半小时白白流逝后——",
        "——その最後にこういうお茶目な感じを付け加えれば、\n完璧な激射になるんだよ。": "——最后再加上这种调皮的感觉，\n就成为完美的激射了。",
        "なるほど…完全に理解したとは言えませんが、\nあなたの仰った「激射」は、多種多様な意味が含まれる別称ですね。": "原来如此……虽然不能说完全理解，\n但您所说的“激射”是包含各种含义的别称吧。",
        "そうそう〜挨拶に使えるし、戦術にも使える。\n励ましのスローガンとしても最適で、使い勝手がいいでしょ？": "没错～既能用于打招呼，也能用于战术。\n作为鼓舞士气的口号也很合适，用起来很方便吧？",
        "そうですね。気合さえあれば何にでも使えます…\nさすがはオークランドのお姉さん。ある意味で彼女よりも個性がありますね。": "是的，只要有干劲，什么场合都能用……\n不愧是奥克兰的姐姐。某种意义上比她更有个性。",
        "本当によかった。こんな短時間でここまで理解してくれるなんて。\nオークランドより悟りが開けてるよ。どう？激射一門の未来を受け継がない？": "真是太好了，没想到你能在这么短的时间内理解到这种程度。\n你比奥克兰更有悟性。怎么样？要不要继承激射一门的未来？",
        "お、オークランド？\n何であなたのお姉さんはいきなり変な声でしゃべり出したの？": "奥、奥克兰？\n为什么你的姐姐突然用奇怪的声音说话？",
        "もったいぶってるだけだよ。\nエリザベスもいろいろ見てきたんだし、いちいち驚くことないでしょ？（もぐもぐ）": "她只是在故弄玄虚。\n伊丽莎白也见过各种事情了，没必要每次都大惊小怪吧？（嚼嚼）",
        "万が一そのようなことがあったら、警察に通報したほうがいいと思いますよ。\nあっ、うちの場合は朝日様と相談したほうがいいですね。": "万一发生那种事，我觉得还是报警比较好。\n啊，换成我们这里的话，应该和朝日大人商量。",
        "そうですか…": "这样啊……",
        "ならしょうがないさ。私の罪は私のものだ。\nそもそも手放す気なんてないし、わざわざお金を出すのも、もってのほかだ。": "那也没办法。我的罪就是我的罪。\n我本来就没打算摆脱它，更不可能特意花钱。",
        "みんなが彼女のようになるのは、\n決していいことではないでしょう。": "如果所有人都变成她那样，\n绝对不是什么好事吧。",
        "あら、エディンバラ、こんばんは。": "哎呀，爱丁堡，晚上好。",
        "これは朝日様、こんばんは。": "这不是朝日大人吗，晚上好。",
        "今日の送金額は、と〜ても大きいわね。\nどの子が払ったのかしら？": "今天的汇款金额真～大啊。\n不知道是哪位孩子付的呢？",
        "それは加賀様が心を込めて罪を償った結果です。": "那是加贺大人诚心赎罪的结果。",
        "へえ、それはそれで一大事だわ。\nご苦労でしたね。": "哦，那也算是一件大事了。\n辛苦你了。",
        "いえいえ。これで、ようやく前におしゃっていたあの経費を賄えますね？": "不不。这样终于能支付您之前提到的那笔费用了吧？",
        "さっきと比べてわずかに元気になったボルチモアだが（少なくとも上半身を起こした）、\nそれでもしょんぼりしているように見える。": "与刚才相比，巴尔的摩稍微精神了一些（至少坐起了上半身），\n但看起来仍然垂头丧气。",
        "うーん…外に出ましょう。\nその方が話しやすいわ。": "嗯……我们出去吧。\n那样更方便说话。",
        "調子が悪いなら、明日でもいいけど？": "如果状态不好，明天也可以哦？",
        "はは、もちろん別の意味よ。\nはっきり言っちゃうと面白くないけどね。": "哈哈，当然是别的意思。\n说得太直白就没意思了。",
        "あら、本当に芽生えたの？\n本当は適当に言っただけだけど、種さんは意外と頑張ってくれたわね。": "哎呀，真的发芽了吗？\n其实我只是随口一说，没想到种子小姐意外地努力呢。",
        "こうして、さんざん体力を無駄にしたあげく、\nようやく自分の指揮官室が一番安全な避難所と気がついた俺は、全力で逃げて帰った。": "就这样，在白白浪费了大量体力后，\n我终于意识到自己的指挥官室才是最安全的避难所，于是全力逃了回去。",
        "おもしろ～い～\n見た感じ後輩くんは細いのに、意外と体力あるよね。": "真有趣～\n看起来后辈君很瘦，没想到体力还不错嘛。",
        "と言っても普通の人間だから、君が無茶でもしたら体が持たなくなるぞ。\nだから節度を守るように。": "不过你毕竟是普通人，要是乱来，身体会撑不住的。\n所以要适可而止。",
        "はいはい。\nほら、みかんを食べて機嫌を直して。": "好啦好啦。\n来，吃个橘子消消气。",
        "今、一番大きな疑問は、これだけの時間をかけて、いったい何を得たのかということだ…\n心が疲れてる。よければまた太ももを借りたい。": "现在最大的问题是，花了这么多时间到底得到了什么……\n心累了。如果可以的话，还想再借用一下你的大腿。",
        "はいはい。\nみかんはいる？": "好好。\n要吃橘子吗？",
        "いやいや、脅迫を受けたことを公言するなって意味でもないって！\n悪者を見るような目であたしを見つめないで…": "不不，也不是叫你别公开说自己受到了威胁！\n别用看坏人的眼神盯着我……",
        "駆逐艦だったら可愛いじゃん？": "如果是驱逐舰，不是很可爱吗？",
        "巡洋艦だよ。\n私たちと同じ巡洋艦で、魚雷を搭載するために火砲をほとんど取り外したんだよ。": "是巡洋舰。\n和我们一样是巡洋舰，为了装备鱼雷，几乎拆掉了所有火炮。",
        "えっ、マジ？\n航空攻撃を受けたらドカーンじゃん？": "诶，真的？\n遭到航空攻击后不就会轰的一声吗？",
        "何だか訓練用弾薬の量が尋常じゃないと思ったら、\nまさか申請書類に手を加えていたとは。": "我还奇怪训练用弹药的数量怎么不正常，\n没想到你竟然篡改了申请文件。",
        "というわけで遊びに来たよ！": "所以我来玩啦！",
        "（いいアイデアはなかったってことね）": "（也就是说你没有好主意）",
        "ちぇっ…\nどうせお嬢様モードのエリザベスはつまんないし…": "切……\n反正大小姐模式的伊丽莎白也很无聊……",
        "先ほどは失礼いたしました。\nあいにくオークランドさんは離席中なので、よければ自己紹介をしていただけませんか？": "刚才失礼了。\n很不巧奥克兰小姐暂时离开了，如果方便的话，可以先自我介绍吗？",
        "げ、激射？": "激、激射？",
        "あっ、最初の挨拶からいい線行ってるね。\nオークランドはいい友達をもったもんだ。": "啊，从第一次问候就说得很不错嘛。\n奥克兰交了个好朋友。",
        "恐れ入ります。": "您过奖了。",
        "（思わず復唱しただけだけど…）": "（我只是下意识重复了一遍……）",
        "私はサンディエゴ。\nオークランドとはいとこ同士だけど、とっても仲よしだから、別にかしこまらなくていいよ。": "我是圣地亚哥。\n虽然和奥克兰是表姐妹，但关系非常好，不用拘谨。",
        "そう言えば、あなたは戦艦だね？\nだったら、やはり数よりは口径派？大きいほうがいいって感じ？": "说起来，你是战列舰吧？\n那你果然是比起数量更看重口径的类型？觉得越大越好？",
        "それは…主砲の話題、ですよね？\n恥ずかしいですが、私も愚妹も実用主義で、使い慣れた装備を愛用しています。": "那是……在说主炮吧？\n虽然有点不好意思，但我和妹妹都是实用主义者，喜欢使用熟悉的装备。",
        "本当！？古典派だったとは！？お嬢様なのに古き良きものを重んじるなんて、珍しいね。\nほら、金持ちってみんなすぐ新しいものを買うってイメージじゃん？": "真的！？没想到你是古典派！？明明是大小姐，却重视古老的美好事物，真少见。\n你看，大家不是都觉得有钱人会马上买新东西吗？",
        "それぞれ自分なりの好みがあるからでしょう。\n私の知る限り、オークランドは砲数派だが、サンディエゴさんは？": "因为每个人都有自己的喜好吧。\n据我所知，奥克兰是炮数派，那圣地亚哥小姐呢？",
        "私は激射派だよ！": "我是激射派！",
        "はあ…\nなるほど…": "唉……\n原来如此……",
        "あらら、ここまで頑張ってきたのに、\n最後の最後で本能に抗え切れないなんて、悲しい人ね。": "哎呀，努力到这种程度，\n却在最后关头无法抵抗本能，真是可悲的人呢。",
        "どうぞ遠慮しないでください。\nもしお口に合ったら、持ち帰っていただいても結構です。": "请不要客气。\n如果合您口味，也可以带回去。",
        "蝶々？綺麗なの？\nどこ？どこなの？": "蝴蝶？很漂亮吗？\n在哪里？在哪里？",
        "止まれ。とぼけて仕事をサボりたいだけだろ？全然信じていないくせに。\nそんな悪企みを考える暇があるなら、ちゃんと仕事をやれ。": "停下。你只是装傻想偷懒吧？明明根本不相信。\n有时间想这些坏主意，不如好好工作。",
        "いやん～捕まえちゃった～": "讨厌～抓到了～",
        "もう、私を催促するよりも自分でやった方が手取り早いのに、\n後輩くんがあの手この手使ってくるのは、私と一緒にいたいからだよね。私って罪の女～": "真是的，明明自己动手比催我更快，\n后辈君却想尽办法催促，是因为想和我待在一起吧。我真是罪孽深重的女人～",
        "あっ、さっきのこと？気にしなくてもいいよ。\n虫が怖いのって別に珍しいことじゃあるまいし。": "啊，刚才那件事？不用在意。\n害怕虫子又不是什么稀奇事。",
        "長身クールビューティーにこんな加点要素があるなんてずるいよ〜\n可愛すぎて羨ましい〜": "高挑冷酷美人竟然还有这种加分项，太狡猾了～\n可爱得让人羡慕～",
        "…条件はなんでしょうか？": "……条件是什么？",
        "ちょっ、待って待って！冗談だよ！\nまるで脅迫を受けたかのようにならないでよ！": "等等，等等！开玩笑的！\n别弄得像是受到了威胁一样！",
        "とにかく、今日のことは誰も教えないって約束するから、これでいいでしょう？\nもう怖がらなくていいから。": "总之，我保证不告诉任何人今天的事，这样可以了吧？\n不用再害怕了。",
        "あたしって約束をちゃんと守る人間だから、人を騙すような真似はしないよ。\nほらほら、デザートでも食べて落ち着いたら？すごく甘くて美味しいよ。": "我是个会遵守约定的人，不会做欺骗别人的事。\n来来，吃点甜点冷静一下？非常甜，也很好吃。",
        "…ありがとうございます。": "……谢谢。",
        "もともとあなたが用意したものだけどね。\nあはは〜": "本来就是你准备的东西嘛。\n啊哈哈～",
        "（まあ、先入観にとらわれるあたしが悪いってことか。\nアンソンはクールビューティーなんかじゃない、人見知りの娘で、ネコみたいな子だよ）": "（嗯，看来被先入观念束缚的我才是错的。\n安森不是什么冷酷美人，而是个怕生、像猫一样的女孩）",
        "会ったことはありませんが、\n資料は読ませていただきましたので。": "虽然我没见过她，\n但已经读过相关资料了。",
        "真面目だね。この基地はよく新人がやって来るから、機会があれば挨拶はするでしょうけど、\nなければ見知らぬ人のままでもいいってあたしは思うよ。": "真认真啊。基地经常有新人来，有机会的话打个招呼就好，\n没有机会的话，继续当陌生人也没关系，我是这么想的。",
        "…そうしたら、\nよく知らない人と一緒に出撃する場合は…": "……这样一来，\n和不熟悉的人一起出击时……",
        "特に問題ないと思うけど？\n戦闘海域までの道中で適当に喋れば、突撃屋とか遠距離一筋とかおおよそ見当がつくよ。": "我觉得没什么问题吧？\n在前往战斗海域的路上随便聊聊，大概就能判断对方是突击派还是远程派。",
        "あっ、ちょうどいい機会だから、当ててみるね。アンソンは、戦闘が開始したら思わず\nスピードダウンするタイプかな？あくまで世間話だから、他意はないよ。": "啊，正好趁这个机会让我猜猜。安森是战斗开始后会下意识\n减速的类型吗？只是闲聊，没有别的意思。",
        "確かに「お姉さん」なんですね。\n率直で…いいですなぁ。": "确实是“姐姐”呢。\n真直率……不错嘛。",
        "そんな格好してるんだからいちいち動くな！\n子供が見てるんだぞ！": "都穿成那样了，就别一直动来动去！\n孩子们还看着呢！",
        "こら。": "喂。",
        "「そんなに心配性じゃ、歳を取ったら大変だな」って。": "他说：“这么爱操心，老了以后可不得了。”",
        "感傷的になって誤魔化すつもりなのか、それとも喧嘩を売るつもりなのか、\nはっきりしたらどうだ？…": "你到底是想用感伤来蒙混过去，还是想挑衅？\n把话说清楚怎么样？……",
        "だから、\n私という先輩がそばにいてあげるね。": "所以，\n我这个前辈会陪在你身边。",
        "…どうでもいい。": "……无所谓。",
        "なるほど、了解。": "原来如此，了解。",
        "…「女性が最も嫌いな返事」をしたのに、\nよくそんな嬉しそうな顔できるな。": "……明明给出了“女性最讨厌的回答”，\n你居然还能露出那么开心的表情。",
        "わざと私をからかうなんて、好きな女の子にいたずらをするみたいな感じ？\nかわいい～ツンツン～": "故意捉弄我，就像恶作剧逗弄喜欢的女孩子一样？\n真可爱～戳戳～",
        "普通なら顔をつつくけど、なんで眉間を……\nまあいいけど。": "一般会戳脸吧，为什么要戳眉间……\n算了。",
        "もし疲れてるなら、\n少し寝たら？": "如果累了，\n要不要睡一会儿？",
        "…まあ、いいだろう。クッションを取ってきてくれ。\nまた足が痺れたら——": "……好吧。帮我拿个靠垫来。\n如果腿又麻了——",
        "あっ、さっきのは嘘だよ。\n全然痺れていなかったから。": "啊，刚才那是骗你的。\n我的腿根本没有麻。",
        "指揮官様は十分公正ではありませんか？そこまでにお悩みのようでしたら、\nいっそ差し押さえた書物を指揮官様に保管させるのはいかがでしょうか？": "指挥官大人不是已经很公正了吗？如果您烦恼到这种程度，\n不如干脆把扣押的书交给指挥官大人保管，如何？",
        "そ、それは…": "这、这个……",
        "ああ、お疲れ様です。": "啊，辛苦了。",
        "…まったく。善良な心をお持ちなのに、なぜかその言動はいつも人々を不安にさせます。\n彼女が信仰する神は、どのように考えているんでしょうか。": "……真是的。您明明拥有善良之心，不知为何言行却总让人不安。\n她所信仰的神明究竟会怎么想呢？",
        "ふ〜ふふ〜♪\nかわいい子羊は、どこにいるのでしょうか〜": "呼～呵呵～♪\n可爱的小羊羔在哪里呢～",
        "どうかご無理はなさらず。": "请不要勉强自己。",
        "…朝日はもう若くはないから、気にしてないけど、あなたはまだ若いわ。\nこれで本当にいいの？": "……朝日已经不年轻了，所以不在意，但你还年轻。\n这样真的没问题吗？",
        "まったく、いい歳してるのに、\n蝶々を追いかけまわすようなことばかりして…": "真是的，都一把年纪了，\n却整天做些追蝴蝶之类的事……",
        "うん。\nなんだ、もう会ったことあるの？": "嗯。\n怎么，你们已经见过面了？",
        "俺は嘆きながらゲーム脳をやめ、あの布切れを洗濯カゴに放り込み、\nこの部屋の掃除という困難に満ちた作業を続行することにした。": "我一边叹气一边停止游戏思维，把那块布扔进洗衣篮，\n继续进行打扫房间这一充满困难的工作。",
        "ことの始めは、今から30分前——": "事情的起因，要从30分钟前说起——",
        "何をだ？": "什么？",
        "ベッドの方を見ると、\n先ほどのてきぱきさが跡形もなく消えた声の主がいた。": "我看向床边，\n发现刚才那个干脆利落的声音主人已经完全变了样。",
        "ましてや俺は上官だし。": "更何况我还是上官。",
        "ごめんなさい…これは「インドアモード」。": "对不起……这是“宅居模式”。",
        "せめて起きてくれよ。": "至少起来一下吧。",
        "…頑張るわ。\nでも早くしないと…": "……我会努力的。\n但如果不快点……",
        "とにかく、私は自分の部屋に戻ると急に元気がなくなるから…\n部屋の片付けをお願いしたいのよ。": "总之，我一回到自己的房间就会突然没精神……\n所以想请你帮忙收拾房间。",
        "単刀直入だな…\nしかもよく上官にそんなことを頼めるよな君は。": "真是直截了当……\n而且你居然能向上官提出这种请求。",
        "お願い…": "拜托……",
        "床をピカピカに磨くなんてことまではしない。\n少し乱雑さを減らすだけだ。": "不用把地板擦得一尘不染。\n只要稍微减少杂乱就行。",
        "本当？\nお世辞でも嬉しいわ。": "真的吗？\n就算是客套话我也很开心。",
        "まあ、そんなに急がなくてもいいわね。\n私の部屋が極限状態になるには一ヶ月ほどかかるから、考える時間はたっぷりあるわ。": "嗯，也不用这么急。\n我的房间要达到极限状态还要一个月左右，有的是时间考虑。",
        "あんな惨状がたった一ヶ月で…\n無駄に力を入れなくてよかった。": "那种惨状居然只用一个月就形成了……\n幸好我没有白费力气。",
        "歩ける場所さえあればいいって、私は思ってるけどね。\nつまずいて転んで、床で一晩過ごすのは流石にしんどいから。": "我觉得只要有能走路的地方就行。\n但要是绊倒摔跤，在地板上过一夜还是太难受了。",
        "ゴミと汚れた服に囲まれながらか…わかった。\n気が乗ったら行くよ。": "要被垃圾和脏衣服包围吗……知道了。\n等我有兴致时就去。",
        "スペアキーはいる？": "要备用钥匙吗？",
        "俺はホテルのスタッフじゃない。": "我不是酒店工作人员。",
        "俺の苦労に、乾杯。": "为我的辛劳，干杯。",
        "チャンスなのか、それとも面倒なのか、\nそれはこれからの状況によって決まるものだ。": "这是机会还是麻烦，\n要看接下来的情况才能决定。",
        "まあ、以前よりは暇になったし、\nやることがあるってことは、別に悪いことでもないかな。": "嗯，比以前清闲了，\n有事情可做也不是什么坏事。",
        "あれ？大人になった平海ちゃんって、なんか違う感じかな？": "咦？长大后的平海，总觉得有点不一样？",
        "おかしいな、どこがズレているんだろう…": "奇怪，到底是哪里出了偏差……",
        "ごきげんよう、寧海さま。\nあなたが廊下で考え込むなんて、なにか愉快なことでもありましたか？": "您好，宁海大人。\n您居然在走廊上沉思，是遇到什么有趣的事了吗？",
        "魂の深くまで染められたわたくしのケダモノ様への服従が、この卑しい肉に抑えられるなんてありえないでしょう？北風のように骨を刺すケダモノ様の目に見られるたびに——": "我的灵魂早已被染上对野兽大人的服从，怎么可能被这卑贱的肉体压制？每当被野兽大人那如北风般刺骨的目光注视时——",
        "ふふん～♪": "哼哼～♪",
        "こら。\nまったく、俺はホテルのスタッフじゃないだろ…": "喂。\n真是的，我又不是酒店工作人员……",
        "と言いつつ、目の前に服やゴミが入りまじる混沌を見て、\n現代社会に生きる人間として流石に我慢できなかった。": "虽然这么说，但看着眼前衣服和垃圾混杂的混沌景象，\n作为生活在现代社会的人，我实在无法忍受。",
        "…変な虫が湧いたら面倒だから、\nとりあえず片付けよう。": "……要是滋生奇怪的虫子就麻烦了，\n总之先收拾一下吧。",
        "本人がそういうから、俺は先に部屋を出た。\nすると——": "既然本人这么说，我就先走出了房间。\n就在这时——",
        "指揮官様がそれに夢中になるなんて心配する必要はないですよ。": "不用担心指挥官大人会沉迷其中。",
        "怒らないで。いつも清々しく「罪を償う」指揮官様のことですから、こういうところはきちんと弁えているはずですし、加賀様への牽制にもなります。ぜひご一考ください〜": "别生气。指挥官大人总是坦然地“赎罪”，应该明白其中分寸，也能借此牵制加贺大人。请务必考虑一下～",
        "それでは、先ほどの無礼のお詫びに、\nどうぞこの小さなお茶会を楽しんでください。": "那么，为赔偿刚才的失礼，\n请享用这场小小的茶会。",
        "そこまでしてくれる必要は…まあ、\nデザートはもう出ているから、食べないのももったいないよね。": "不用做到这种程度……不过，\n甜点都已经端上来了，不吃也太浪费了。",
        "（っていうか、ピンク色の食器の数が多すぎ！\nこの人はヨークタウンのような、白か黒かしか使わないタイプだと思ったんだけど）": "（话说回来，粉色餐具也太多了！\n我还以为她是像约克城那样只用黑白两色的人）",
        "「上下関係を越える信頼」と解釈すべきか？\nったく…": "应该理解为“超越上下级关系的信任”吗？\n真是的……",
        "そうね、実は「指揮官は指揮官なりの楽しさを見つければいい」と思っていたけど、\n毎日女の子に囲まれて、免疫がついてるとは。来週はどうしようか困ってるわ。": "是啊，其实我本来觉得“指挥官找到属于自己的乐趣就好”，\n没想到你每天被女孩子包围，已经产生免疫了。下周该怎么办呢。",
        "勝手に俺のスケジュールを決めるなよ。\n自分でやれないなら掃除ロボットでも使ったらどうだ？": "别擅自决定我的日程。\n自己做不到的话，用扫地机器人不就好了？",
        "…本当ですか？お金を出さなかったせいで、\n寝室のドアに変な絵が描かれるなんてことはないんですね？": "……真的吗？不会因为我没出钱，\n就有人在卧室门上画奇怪的图吧？",
        "なんてな。このような世俗に身を置きながら、手にしている物は単なる清らかな水。\nシスター様の独立独行ぶりを見たら、流石に話をかけたくなる。": "开玩笑的。身处这样的世俗之中，手里拿的却只是纯净的水。\n看到修女大人如此特立独行，实在忍不住想和她聊聊。",
        "それでしたら、どうぞおかけください。\nそれぞれの形は違うにせよ、ここに来る者は、皆「罪」を償いたいと考えています。": "既然如此，请坐吧。\n无论形式各不相同，来到这里的人都想要偿还“罪”。",
        "…単なる「ケチ」は、美徳とは言えんな。": "……单纯的“吝啬”，可算不上美德。",
        "おっしゃる通りだと思います。": "我也这么认为。",
        "…そんなことだろうと思ったよ。": "……我就知道会这样。",
        "…わかったわ。\n自分の信念に従って頑張ってね。": "……知道了。\n遵循自己的信念努力吧。",
        "げっ！？": "咦！？",
        "あらら、フォッシュ様、\nどうしてそんな慌てた顔をしているんですか？": "哎呀，福煦大人，\n为什么露出这么慌张的表情？",
        "……ってか違うだろ？先に来たのは俺のはずだけど？": "……不对吧？应该是我先来的才对？",
        "後輩くん、はい、今日の仕事よ。\nわからないことがあったら、ダンケルク先輩に聞いてね。さあ、頑張って。": "后辈君，来，这是今天的工作。\n有不懂的就问敦刻尔克前辈。好了，加油。",
        "はい！ダンケルク先輩！新参者ですが、精一杯頑張ります！": "是！敦刻尔克前辈！虽然我是新人，但我会全力以赴！",
        "よくやった！！イータさんだったっけ？やっぱ才能があるね！\nでも「激射！！！」って声出す時に、もっと気合いが入るとなおさらいいぞ！": "干得好！！你是叫伊塔对吧？果然很有天赋！\n不过喊“激射！！！”时再更有气势一点就更好了！",
        "うぅぅ…増援はいつになったら来ますか？\n…弾薬も私の羞耻心も、そろそろ限界…": "呜呜……增援什么时候才来？\n……弹药和我的羞耻心都快到极限了……",
        "幸運なことにこの話を聞けた凡人どもよ、 まずはこのわたくし——\nヴィットリオ・ヴェネトに会えたことに万歳三唱して喜ぶがいい。 ": "幸运听到这番话的凡人们啊，首先为能见到我——\n维托里奥·维内托而欢呼三声庆贺吧。",
        "……うん。 ": "……嗯。",
        "それで、 今回もまた人にすこし煽られたから、 \nつい見栄えを張っちゃったの？ ": "所以这次又被别人稍微一激，就忍不住逞强了吗？",
        "見栄えを張るのではなく、 わたくしはただあの浅識な者たちに、 いちいち道化師の小細工\nみたいなものに驚く必要はないと親切に教えてあげていただけよ。 ": "不是逞强，我只是好心告诉那些浅薄之人，\n没必要每次都被小丑般的把戏吓到。",
        "心優しい皆さんを驚かせて、 わざわざ検査に運ばれてきてもらったのは申し訳ないが、 \nわたくしがあのような尋常な攻撃に傷を付けられるわけがないでしょう！ ": "虽然很抱歉吓到了善良的大家，还让你们特意被送来检查，\n但我怎么可能被那种普通攻击伤到呢！",
        "運ばれてきた途中、 すぐにでも死にそうな顔をしていたと聞いたけど……\nまあ、 確かに今の様子を見ると、 あまり心配しなくてもいいよね。 ": "听说被送来的路上，你一副马上就要死掉的表情……\n不过看现在的样子，确实不用太担心了。",
        "なら、 私はもう帰るね？ ちゃんと休んで、 体を休ませるんだよ。 \nそうだ、 あとで小動物の癒され動画でも送るよ。 ": "那我先回去了？好好休息，让身体恢复。\n对了，之后我给你发些治愈小动物的视频。",
        "待って！ ドーリア姉、 もう行くの？ 他に急用でもあるの？ わたくしのことで\n迷惑をかけたから、 お詫びにケーキでもおごらせてもらおうかと思っていたのに。 ": "等等！多里亚姐姐，这就要走了吗？还有急事吗？\n我给你添了麻烦，本来想请你吃蛋糕赔罪的。",
        "ちょっと心配で様子を見にきただけだから、 \n迷惑なんて全然……": "我只是有点担心，过来看看情况，\n根本谈不上麻烦……",
        "……でも、 たしかにアフターヌーンティーにはちょうどいい時間だね。 \nすこし息抜きするのも悪くないかもね。 ": "……不过，确实到了享用下午茶的时间。\n稍微放松一下也不错。",
        "任せてください！ \n全力を出して、 絶対満足させるわ！ ": "交给我吧！\n我会全力以赴，绝对让你满意！",
        "……ヴェネ、 ただのアフタヌーンティーのはずなのに、 \n今のはちょっとやりすぎじゃないか？ ": "……维内托，这应该只是下午茶吧，\n刚才是不是有点过头了？",
        "そんなことない。 ただ慌てて初春たちに備蓄の二十三種類のデザートを出してもらっただけで、 \n組み合わせも飾りつけも時間がないから、 ちゃんと出来ていないのよ。 ": "没有。我只是匆忙让初春她们拿出了储备的23种甜点，\n因为时间不够，搭配和装饰都还没做好。",
        "しかし安心するがいい！ このようなイベントで一番大事なのは、 心地よい雰囲気と楽しい話題よ。 \nそこに関しては、 わたくしがいれば、 ストゥルフォリの砂糖玉みたいに万全だわ。 ": "不过尽管放心！这种活动最重要的是舒适的氛围和愉快的话题。\n这一点只要有我在，就像斯特鲁福利的糖球一样万无一失。",
        "……そうね。 その比喩があっているかどうかはさておき、 ヴェネの情熱は伝わってきたよ。 \nそれじゃ——": "……是啊。先不论这个比喻是否恰当，维内托的热情确实传达到了。\n那么——",
        "ちょっと失礼。 \n今日リットリオを見かけていませんか？ ": "打扰一下。\n今天有看到利托里奥吗？",
        "リオ？ そういえば、 今日は会っていないね。 ": "利奥？说起来，今天确实没见到她。",
        "こっちもか……": "这边也没有吗……",
        "ちょっと、 何よその態度？ \n挨拶もせずにいきなり人の会話に割り込むなんて、 礼儀知らずにもほどがある。 ": "喂，你那是什么态度？\n不打招呼就突然插入别人的谈话，也太没礼貌了。",
        "この基地のあらゆる事務をつかさどる責任者として、 昼休み時間の後も\nデザートをテーブルいっぱいに並べてじゃれあうほど、 私は暇じゃないからね。 ": "作为负责基地所有事务的负责人，我可没闲到午休后还\n把甜点摆满桌子和人嬉闹。",
        "うぐ！ ": "呜！",
        "いきなり春のイベント予算申込書の整理をまかせろと言ってきたのはリットリオの方なのに、 \n明後日が期限のこの時にどこにも見当たらないとは。 ": "明明是利托里奥突然说要负责整理春季活动预算申请书，\n可眼看后天就是期限，却到处都找不到人。",
        "まさかと思うが、 いまさら出来そうにないと気づいて隠れて、 \n事が収まってから何事もなかったような顔で出てくるつもりではないよね？ ": "不会吧，难道她现在才发现做不完，于是躲了起来，\n等事情平息后再装作什么都没发生一样出现？",
        "そ、 それは……": "这、这个……",
        "……まあいい。 姉のしでかしたことを、 妹であるあなたのせいにするのも理不尽ですね。 \n私がすこし当てつけていました。 ": "……算了。把姐姐做的事怪到身为妹妹的你头上也不合理。\n是我有些迁怒了。",
        "わたくしの方がお姉さんよ！ 何回も言ったでしょう、 \nこのわたくし、 ヴィットリオ・ヴェネトこそがわが一族の長女と！ ": "我才是姐姐！我说过很多次了，\n我维托里奥·维内托才是家族长女！",
        "これ以上文書の間違いで、 \nあのバカ妹にわが一族の名に泥を塗られるのは——": "不能再因为文件错误，\n让那个笨妹妹给我们家族的名声抹黑了——",
        "あぁ、 もう好きにして。 \nまだ山積みの仕事が私を待っているのですよ。 ": "啊，随你吧。\n还有堆积如山的工作等着我。",
        "まったく、 あの 「オホホホ」 の笑い声がこんなに聞きたい日が来るとは……\nいいえ、 リットリオがわざと隠れているのなら、 そんな風に笑ったりしないか。 ": "真是的，没想到会有如此想听到那声“哦呵呵呵”的一天……\n不，如果利托里奥是故意躲起来的，应该不会那样笑吧。",
        "……嵐のように行っちゃったね。 ": "……她像一阵风似的走掉了。",
        "やはり礼儀知らずな野蛮人！ 見た目をそれなりに整えていても、 \n何があったらすぐ本性を現すのね！ ほんっとうに品のかけらもない！ ": "果然是没礼貌的野蛮人！即使外表打扮得像样，\n一遇到事情就立刻露出本性！真是一点品位都没有！",
        "そう怒らないで。 最近色々なことがごたごたしているからね。 \n場を引き締める役として、 彼女も平常心を保ちにくいでしょう。 ": "别这么生气。最近各种事情都乱成一团。\n作为负责稳定局面的人，她也很难保持平常心吧。",
        "今日私だって、 アルティたちの漬物作りを手伝っていたから、 \n港の方まであなたたちを迎えに行けなかった。": "今天我也在帮阿尔蒂她们做腌菜，\n没能去港口接你们。",
        "その後、 急にヴェネがもうだめだって聞いた時、 本当に驚いた。 ": "之后突然听说维内托不行了，我真的吓了一跳。",
        "！ そういえばまだせっかくのアフターヌーンティーを楽しんでいないね。 \n過ぎたことはもう気にしなくていいから、 何か楽しい話題にしましょう。 ": "！说起来，我们还没好好享用下午茶呢。\n过去的事不用再在意了，聊点开心的话题吧。",
        "リオのことが気にかからないのかい？ ": "你不担心利奥吗？",
        "あの子もいい大人だから、 そろそろ自分のしたことに責任を持ってもらわないと。 \nいつまでもわたくしが後始末してあげるのもだめでしょう。 ": "那孩子也已经是大人了，差不多该为自己的行为负责。\n我也不能永远替她收拾残局。",
        "……リオも確かに底の知れないところがある子だし、 \n何をしようとしているのか、 一応様子を見てみよう。 ": "……利奥确实是个深不可测的孩子，\n先看看她到底想做什么吧。",
        "ふふ、 本当に楽しみだわ。 ": "呵呵，真令人期待。",
        "——なわけないよ！ \nこの状況はどう考えても典型的なやらかしては逃げるパターンでしょう！ ": "——怎么可能！\n怎么看这都是典型的闯祸后逃跑模式吧！",
        "ドーリア姉にデザートとお茶をちゃんと楽しんでもらえるために、 \n心配していないと無理に嘘をついていたけど、 このままでは……": "为了让多里亚姐姐好好享用甜点和茶，\n我一直勉强装作不担心，但这样下去……",
        "（ 去年の年末はいろいろあったから、 ちゃんと祝えなかった。 \n皆さんも今回の春の催しでその穴を埋めようと思っている。  ）": "（去年年末发生了很多事，没能好好庆祝。\n大家也想借这次春季活动弥补遗憾。）",
        "（ それぞれやりたいイベントがある人も結構いるから、 物資も資金も必要だ。 \nこういう時みんなに金を出してもらうのはもちろんありえない。 申請すれば予算が付くから。  ）": "（有不少人都有想举办的活动，所以需要物资和资金。\n这种时候当然不能让大家出钱，只要申请就会有预算。）",
        "（ しかし、 文書やリストの作成と整理は口で言うほど簡単ではない。 申請の隙を突こうと\n企んでいる輩もいるでしょうから、 実際に確認してから決めてもらわないといけない。  ）": "（不过，制作和整理文件、清单并没有说起来那么简单。\n肯定有人想钻申请的空子，所以必须实际确认后才能决定。）",
        "（ そして今はリオがその仕事を受取った。 \nもしリオが途中で投げ出したせいで、 全部水の泡になったら…… ）": "（而现在利奥接下了这份工作。\n如果利奥半途而废，让一切努力化为泡影……）",
        "絶対あの嫌な輩たちに延々と文句言われる羽目になる！ \nあんなことが丸一年間も続いたら、 たまったものではないよ！ ": "绝对会被那些讨厌的家伙没完没了地抱怨！\n那种事要是持续整整一年，可就受不了了！",
        "……いまさら、 あのバカ妹を当てにするのはもう出来ない。 \nわたくしが何とかするしかないみたいね。 ": "……事到如今，已经不能再指望那个笨妹妹了。\n看来只能由我来想办法。",
        "いい方向に考えると、 こういう実績を積み重ねていけば、 \n皆さんも自然とわたくしこそ長女という事実を認めてくれるでしょう。 \n": "往好的方面想，只要不断积累这样的成绩，\n大家自然会承认我才是长女这一事实。\n",
        "それから、 それらの文書の間違いを訂正するように要求するのも、 \n受け入れられやすくなるに違いない。 ": "这样一来，要求他们改正那些文件中的错误，\n也一定会更容易被接受。",
        "そうしましょう！ \n明日からみんなに予算申請の詳細を聞いて、 それを記録してから——": "就这么办！\n从明天开始询问大家预算申请的详情，记录下来后——",
        "……いけない。 わたくしは本当はこの仕事を頼まれていないのに、 \nどうやって皆さんに説明すればいいの？ ": "……不行。明明没有人真正拜托我做这件事，\n我要怎么向大家解释？",
        "まあ、 わたくしには洗練された社交スキルがあるから、 \nなんとか気づかれないうちに話を聞けるわ、 うん！ ": "不过，我拥有高超的社交技巧，\n一定能在不被察觉的情况下打听到详情，嗯！",
        "我が激射の道——阻む者なし！": "我的激射之道——无人能挡！",
        "へい☆！！": "嘿☆！！",
        "うあああ！？": "呜啊啊啊！？",
        "な、なんという豪放な戦い方！\n砲弾が湯水のように…": "这、这是多么豪放的战斗方式！\n炮弹像流水一样……",
        "激射！\n弾薬はこっちの弾薬庫からこっそりいただいたんだよ！": "激射！\n弹药是我从这边的弹药库偷偷拿来的！",
        "ひぇっ！？\nげ、激射…？": "咿！？\n激、激射……？",
        "その通り、激射、だよ！！\nはは～ムーバーの姉さん、なかなか筋がいいね！": "没错，就是激射！！\n哈哈～搬运者姐姐，你很有天赋嘛！",
        "こんなに砲弾を撃ち放題の機会なんて滅多にないから、思う存分激射しよ！\n超華麗に、激射——！！": "能这样随心所欲发射炮弹的机会可不多，尽情激射吧！\n华丽地，激射——！！",
        "があああ！？": "嘎啊啊啊！？",
        "そう、 このわたくしこそ最新鋭の戦艦ヴィットリオ・ヴェネト級の長女。 \nこの身には悠久かつ高貴な伝統と栄光を、 世界中に——": "没错，我正是最新锐战列舰维托里奥·维内托级的长女。\n我的身上承载着悠久而高贵的传统与荣光，向全世界——",
        "きゃ～ヴェネト様～\n砲弾が飛んできたよ～": "呀～维内托大人～\n炮弹飞过来了～",
        "魚雷も来ました～怖いですよヴェネトさま～\n助けてください～": "鱼雷也来了～好可怕啊维内托大人～\n救救我～",
        "待ちなさい！ どうしてわたくしの方に向かってくるの？ \nあなたたちは駆逐艦ですから、 速度を上げれば避けられるでしょう？ ": "等一下！为什么要朝我这边来？\n你们可是驱逐舰，提高速度就能躲开吧？",
        "えぇ～？ \nでもエリザベスお姉さんなら、 何も言わずにあたしたちを庇ってくれるのに～？ ": "诶～？\n可是伊丽莎白姐姐的话，会一言不发地保护我们呀～？",
        "それとも、 あのような勇敢なことができるのはエリザベスお姉さまだけですか？ \nそれもそうですね～あたるのは確かに怖いですもんね～": "难道能做出那种勇敢行为的只有伊丽莎白姐姐吗？\n说得也是～被击中确实很可怕呢～",
        "ふ、 ふん！ あんなこと、 戦艦の装甲にしてみれば、 \n取るに足りないお安い御用よ。 ": "哼、哼！那种攻击对战列舰装甲来说，\n根本不值一提，小菜一碟。",
        "よく見るがよい。 \nこのわたくし、 ヴィットリオ・ヴェネトが敵の攻撃を優雅に受け止める姿を——": "看好了。\n看我维托里奥·维内托优雅地承受敌方攻击——",
        "きゃああああああああ！ ！": "呀啊啊啊啊啊啊啊！！",
        "……ひっく……うぅ……こわいよ……": "……呜咽……呜……好可怕……",
        "ちょっと失礼するね——": "失礼一下——",
        "驚嘆せよ！ これぞこそ世界で最も先進的な装甲の強度だわ！ \n魚雷にせよ、 砲弾にせよ、 この装甲の前では蚊も同然よ！ ": "惊叹吧！这就是世界上最先进装甲的强度！\n无论是鱼雷还是炮弹，在这副装甲面前都和蚊子一样！",
        "……誰もついていないよ。 ": "……没有人跟着呢。",
        "うえぇぇ～～ドーリア姉～～\nこわいよ～～～本当に怖かったよ～～～！ ": "呜呜～～多里亚姐姐～～\n好可怕～～～真的好可怕～～～！",
        "はいはい、 装甲は貫通されていないでしょう？ \nもう泣かないで、 怪我がなくて何よりだ。 ": "好了好了，装甲不是没有被击穿吗？\n别哭了，没受伤就是万幸。",
        "ヒッ……！？": "咿……！？",
        "でも急に撃たれたあの時ほんとに痛かった……\nあと音もすっごく怖くて……それからの爆発も……": "可是突然被射中的时候真的很痛……\n而且声音也特别吓人……后面的爆炸也……",
        "うえぇ！ 全部あのデカ帽子金ぴかドリルのせいよ！ わざわざあんなものを受け止めるなんて、 \n絶対マヨネーズに漬かっているフィッシュ・アンド・チップスの食べすぎよ！ ": "呜呜！全都是那个大帽子金光闪闪的钻头怪的错！居然特意去承受那种东西，\n肯定是炸鱼薯条泡蛋黄酱吃太多了！",
        "何を言っているのか、 まったく分からなかったけど……\n気持ちは大体わかる。 ": "虽然完全听不懂你在说什么……\n但大概能理解你的心情。",
        "ほら、 顔を拭いて。 \n後で部屋を出た時、 人にバレたら嫌でしょう？ ": "来，擦擦脸。\n待会儿离开房间时，要是被别人发现就不好了吧？",
        "…でしたら、仕方ないですね。": "……既然如此，也没办法了。",
        "すまんな。なにせ、私は「良識」というものと距離がありすぎるんだ。\nさあ、その距離に、乾杯！": "抱歉。毕竟，我和所谓的“常识”距离太远了。\n来，为这段距离干杯！",
        "ヒッパー様…実に独立独行なお方ですね。\n指揮官様の言う通り、「眩しいほど黒い」ような感じがします。": "希佩尔大人……真是位特立独行的人。\n正如指挥官大人所说，感觉“黑得耀眼”。",
        "駆逐艦の子たちの寮のリニューアル費用ね。\nごめんなさい、上はお金を出す気なんてないけど、やっぱりあの子たちのために…": "是驱逐舰孩子们宿舍的翻新费用。\n抱歉，上面根本不打算出钱，但为了那些孩子们，果然还是……",
        "どうか気にしないでください。罪を償うためのお金が善意のために使われるなら、\nこの世界も少しずつよくなってくると思います。": "请不要在意。如果赎罪的钱能用于善意之事，\n我想这个世界也会一点点变好。",
        "信仰が実現することによって、いずれ他の人のお金もこのサイクルに加わることになります。\nですから、それが原因で、気に病んだり、立ち止まったりする必要は全くありませんよ。": "随着信仰实现，其他人的钱最终也会加入这个循环。\n所以完全不必因此忧心或停下脚步。",
        "それは無理です。罪とは、他人に損をさせてしまったものです。\nですから、お金を払わずにそれを手放したいというのは、理にかなっていません。": "那是不可能的。罪就是让他人蒙受损失。\n所以，不付钱就想摆脱罪责，是没有道理的。",
        "ありがとう。経費自体については、今回の払い込みで、\nあと少し工面できればいい状況になったから、もう工事を始めてもいい状態だわ。": "谢谢。至于经费本身，这次付款后只要再筹到一点，\n就已经可以开始施工了。",
        "でしたら、最後のお祝いとして、\n指揮官様のお財布にお願いしてみましょうか？": "既然如此，作为最后的庆祝，\n要不要请指挥官大人的钱包再帮个忙？",
        "ふふ、よしてあげて。指揮官くんはもう結構出してくれてるから、\nこれ以上せびるのはもうカツアゲになっちゃうわ。最後の分くらいは朝日が出すわよ。": "呵呵，放过他吧。指挥官已经出了不少钱，\n再继续索要就成勒索了。最后这部分就由朝日出吧。",
        "もっと皆様に「罪を償わせる」って、意味でしょうか？もちろんこれからも\n頑張ってまいります〜引き続き、集めたお金の使う道を考えてくださいね。": "您的意思是要让更多人“赎罪”吗？当然，我今后也会\n继续努力～还请继续考虑筹集的钱该如何使用哦。",
        "うん、朝日に任せて。": "嗯，交给朝日吧。",
        "ふふ♪": "呵呵♪",
        "その仕事って君の担当なんだけど…": "那份工作不是由你负责的吗……",
        "ああ、お上手～この仕事が終わったら、先輩としてプライベートなご褒美をあげようかしら。\n何が欲しい？": "啊，做得真好～工作结束后，要不要让我以身为前辈的身份给你一份私人奖励？\n你想要什么？",
        "先輩の嬉しい笑顔が見てみたいです！": "我想看看前辈开心的笑容！",
        "よく考えてみたら、あたしみたいなのがうるさいってことなんだろう。\n今度うちの妹を紹介してあげるよ。似た者同士だし、気が合うかもよ。": "仔细想想，像我这样的人大概确实很吵吧。\n下次介绍我妹妹给你认识。你们性格相似，说不定很合得来。",
        "平海さんですか？": "是平海小姐吗？",
        "（「別に問題ないじゃない？(<ゝω·)☆」…）": "（“不是没什么问题吗？(<ゝω·)☆”……）",
        "も、申し訳ございませんが、\n私にはあまり貯金がないので、お金は出せないです…": "对、对不起，\n我没有多少存款，实在拿不出钱……",
        "それに、「贖宥」は、あくまで自分の罪を償うことですから、\n罪のない人が「贖宥」する必要はないですし、それが理由で不幸になることもありません。": "而且，“赎罪”终究是为了偿还自己的罪，\n没有罪的人不需要“赎罪”，也不会因此变得不幸。",
        "あら？指揮官？\n珍しいわね。もう仕事は終わったの？": "哎呀？指挥官？\n真稀奇。工作已经结束了吗？",
        "あっ、ボルチモアか。そう言えば、これも君のおかげだったな。君が教えてくれた方法を採用したことで、\n仕事の進みが格段と順調になった。ついさっき、今日の最終業務を発注したぞ。": "啊，是巴尔的摩啊。说起来，这也多亏了你。采用你教的方法后，\n工作进展顺利多了。刚才已经下达了今天的最后一项工作。",
        "そうでしょう？確かに仕事量は多いけど、\nコツさえ掴めば、エリザベスたちに頼まなくても終わらせるはずだわ。": "对吧？工作量确实很大，\n但只要掌握诀窍，应该不用拜托伊丽莎白她们也能完成。",
        "これで、俺もようやくみんなに頼りっぱなしのルーキー状態から脱却できるのか。\nもう二年目になるか…長いな。": "这样一来，我终于也能摆脱一直依赖大家的新手状态了吗。\n已经要进入第二年了啊……真久。",
        "…普通の知り合いであっても、\n部屋に誘っておいて自分が真っ先に寝るなんてどうかと思う。": "……就算只是普通熟人，\n把人邀请到房间后自己却第一个睡着，也未免太不像话了。",
        "SBWB-7にて、輝かしきNO.1に輝いた指揮官に送られる最強の証しフレーム！皆の手本となり、これからも最強艦隊で世界を守ってくれ！": "SBWB-7中授予荣登耀眼NO.1的指挥官的最强证明头像框！请成为大家的榜样，今后也用最强舰队守护世界！",
        "ゴールドフレーム-7": "黄金头像框-7",
        "SBWB-7にて、頂きを競うNUMVBERSに輝いた指揮官に送られる精鋭の証しフレーム！切磋琢磨を繰り返し、競い合い頂点を狙うは君だ！": "SBWB-7中授予荣登顶峰竞争者NUMVBERS的指挥官的精英证明头像框！不断切磋竞争、瞄准顶峰的人就是你！",
        "シルバーフレーム-7": "白银头像框-7",
        "SBWB-7にて、惜しくもAPPROVARに輝いた指揮官に送られる強者の証しフレーム！上にはNUMVBERSに、そしてNO.1が犇めく大海域に挑め！": "SBWB-7中授予荣登APPROVAR的指挥官的强者证明头像框！向上挑战NUMVBERS以及NO.1云集的广阔海域吧！",
        "ブロンズフレーム-7": "青铜头像框-7",
        "高貴な美徳フレーム": "高贵美德头像框",
        "第4期Command Sennkiイベントの指揮本部の指令-究極機密を購入時に即時獲得する事ができる大成の証し、プリンツ・オイゲンの水着着せ替えを体現したフレーム。全体的に要素を満載に取り入れてたハイビスカスの指し色がアクセントが効いている！": "第4期Command 战姬活动指挥总部指令—终极机密购买后可立即获得的成功证明，体现欧根亲王泳装换装的头像框。整体融入大量元素，木槿花的点缀色十分醒目！",
        "夏姫祭イベント-【鶴鷹の舞う海】にて、獲得できる限定フレーム！簡単なデザインだが、隼鹰の髪飾りやピンクを基調とした隼鹰らしさが散りばめれられた隼鹰ファンには堪らない一品となっている。": "夏日战姬祭活动—可在【鹤鹰飞舞的海】中获得的限定头像框！设计简洁，却散布着隼鹰的发饰与粉色基调等风格元素，是隼鹰粉丝不容错过的珍品。",
        "隼鹰の記念フレーム": "隼鹰纪念头像框",
        "『美食マスターの道』イベントにて、獲得できる限定フレーム！黄色を基調として、さっぱりした印象、この暑い夏にぴったりの一品！": "可在『美食大师之路』活动中获得的限定头像框！以黄色为基调，给人清爽的印象，是炎热夏日的绝配！",
        "さっぱりフレーム": "清爽头像框",
        "どちらが取り、どちらが捨てるべきか?どっちが良くてどっちが悪い?": "哪个该拿起，哪个该舍弃？哪个是好，哪个是坏？",
        "相対する姿のフレーム": "相对姿态头像框",
        "指令イベント！夏姫祭スペシャルのイベントにて、獲得できるフレーム": "可在指令活动！夏日战姬祭特别活动中获得的头像框",
        "砂浜のフレーム": "沙滩头像框",
        "友達と過ごしたあの短い時間は、夏の忘れられない思い出になる": "与朋友共度的短暂时光，将成为夏日难忘的回忆。",
        "夏の思い出": "夏日回忆",
        "赤のフレーム": "赤的头像框",
        "忘れられない記憶がある": "有一段难以忘怀的记忆",
        "第8期Command Sennkiイベントにて獲得する事ができる大成の証し、寿司屋の雰囲気たっぷりのフレーム！": "第8期Command 战姬活动可获得的成功证明，充满寿司店气氛的头像框！",
        "寿司屋の指令フレーム": "寿司店指令头像框",
        "強敵にもめげず、力戦し、勝ち誇った者が優勝である。": "不畏强敌、奋力战斗并胜利的人才是冠军。",
        "Command Sennkiバレンタインスペシャルイベントにて、獲得できる大成の証し！改めて戦姫との絆を確認しよう！": "可在Command 战姬情人节特别活动中获得的成功证明！再次确认与战姬之间的羁绊吧！",
        "LOVEフレーム": "LOVE头像框",
        "戦闘や各種の事務作業だけでなく農作業もこなす戦姫たちにはさすがです。": "战姬们不仅能战斗、处理各种事务，还能完成农活，真是厉害。",
        "はたらく戦姫": "劳动战姬",
        "牡羊座のフレーム": "白羊座头像框",
        "サービス1ヶ月を記念して、限定フレームを配布いたします。（リリース日-4月7日~5月7日）": "为纪念服务满1个月，发放限定头像框。（发布日期—4月7日～5月7日）",
        "牡牛座のフレーム": "金牛座头像框",
        "ゴールデンウィーク限定のフレーム、GW記念特別ログイン報酬一日目に獲得可能です。": "黄金周限定头像框，可在GW纪念特别登录奖励第一天获得。",
        "ゴールデンフレーム": "黄金头像框",
        "愛宕＆高雄の初登場イベントを記念して作られたフレーム、イベントでのみ獲得する事が可能です。": "为纪念爱宕＆高雄首次登场活动而制作的头像框，仅可在活动中获得。",
        "雨宿りフレーム": "避雨头像框",
        "人生は夢のようだ。たとえ夢であっても、一部の人にとってはもはや完全ではない。": "人生如梦。即使是梦，对某些人来说也已不再完整。",
        "残夢フレーム": "残梦头像框",
        "【第七期】最強艦隊イベントにて入手可能な限定フレーム。": "可在【第七期】最强舰队活动中获得的限定头像框。",
        "ミューのフレーム": "缪的头像框",
        "第9期Command Sennkiイベントにて、獲得できる大成の証し！改めて戦姫との絆を確認しよう！": "可在第9期Command 战姬活动中获得的成功证明！再次确认与战姬之间的羁绊吧！",
        "桜のフレーム": "樱花头像框",
        "6月1日の月始め記念として、限定フレームを配布いたします。": "为纪念6月1日月初，发放限定头像框。",
        "双子座フレーム　": "双子座头像框",
        "『Pixiv ｘ 蒼藍の誓-ブルーオース』イラストコンテスト特別記念フレーム": "『Pixiv × 苍蓝誓约-蓝色奥斯』插画大赛特别纪念头像框",
        "Pixivイラコン記念フレーム": "Pixiv插画大赛纪念头像框",
        "3月31日の月末記念として、限定フレームを配布いたします": "为纪念3月31日月末，发放限定头像框。",
        "2022牡羊座のフレーム": "2022白羊座头像框",
        "日本リリース100日記念フレーム": "日本上线100日纪念头像框",
        "祝100日記念フレーム": "庆祝100日纪念头像框",
        "7月31日の月締め記念として、限定フレームを配布いたします": "为纪念7月31日月末，发放限定头像框。",
        "獅子座フレーム": "狮子座头像框",
        "トナカイの引く橇にはたくさんの贈り物が積まれています": "驯鹿拉着的雪橇上堆满了礼物。",
        "夢と素晴らしき運び屋": "梦想与伟大的搬运者",
        "異星生物入りの料理を征服した勇者に授けるフレーム": "授予征服异星生物料理的勇者的头像框",
        "第12期Command Sennkiイベントにて、獲得できるフレーム！": "第12期Command 战姬活动可获得的头像框！",
        "Command Sennkiのフレーム（第12期）": "Command 战姬头像框（第12期）",
        "9月30日の月末記念として、限定フレームを配布いたします": "为纪念9月30日月末，发放限定头像框。",
        "てんびん座フレーム": "天秤座头像框",
        "10月31日の月末記念として、限定フレームを配布いたします": "为纪念10月31日月末，发放限定头像框。",
        "蠍座のフレーム": "天蝎座头像框",
        "【桜が舞う季節-パールベイに春が訪れる】イベントに登録するだけでもらえます": "只需登记参加【樱花飞舞的季节—春临珍珠湾】活动即可获得",
        "花咲き物語のフレーム": "花开物语头像框",
        "【蕾綻ぶ季節に】の報酬として入手できる期間限定フレーム！": "作为【花蕾绽放的季节】奖励获得的限时头像框！",
        "伊168フレーム": "伊168头像框",
        "2022おうし座のフレーム": "2022金牛座头像框",
        "いて座フレーム": "射手座头像框",
        "12月31日の月末記念として、2021年最後の星座限定フレームを配布いたします。": "为纪念12月31日月末，发放2021年最后一款星座限定头像框。",
        "やぎ座のフレーム": "摩羯座头像框",
        "弥生の宝箱イベントにて、獲得できる限定フレーム！ひなまつりを模した和風と桜添えがまた新たな季節を連想させるデザイン！": "可在弥生宝箱活动中获得的限定头像框！仿照女儿节的日式风格搭配樱花，令人联想到全新季节的设计！",
        "ひなまつりフレーム": "女儿节头像框",
        "うお座フレーム": "双鱼座头像框",
        "2022双子座のフレーム": "2022双子座头像框",
        "2022蟹座のフレーム": "2022巨蟹座头像框",
        "2022てんびん座のフレーム": "2022天秤座头像框",
        "2022蠍座のフレーム": "2022天蝎座头像框",
        "初回【戦域の友‐アンノウンM編】イベントにて、獲得できる漆黒フレーム！漆黒に包まれたその風貌は、まさにアンノウンMを模した禍々しさを漂わせる！": "可在首次【战域之友—Unknown M篇】活动中获得的漆黑头像框！被漆黑包裹的外观正是模仿Unknown M，散发着不祥气息！",
        "アンノウンMフレーム（漆黒）": "Unknown M头像框（漆黑）",
        "初回【戦域の友‐アンノウンM編】イベントにて、獲得できる黄金フレーム！黄金に包まれたその風貌は、アンノウンM制服した強者の証し！": "可在首次【战域之友—Unknown M篇】活动中获得的黄金头像框！被黄金包裹的外观，是制服Unknown M的强者证明！",
        "アンノウンMフレーム（黄金）": "Unknown M头像框（黄金）",
        "朝日のプチ思い出を具現化したフレーム！これからも朝日はどんな思い出を紡いでいくのだろう～": "将朝日的小小回忆具象化的头像框！今后朝日还会编织出怎样的回忆呢～",
        "1st思い出フレーム": "1st回忆头像框",
        "今回のイベントが終了したので、指揮官のイベント限定コインを回収させていただきます。代わりに下記の報酬を配布させていただきます。": "本次活动已结束，将回收指挥官的活动限定硬币，并改为发放以下奖励。",
        "今回のイベント「MS【赤の編】前夜祭イベントついに完結！」が終了したので、指揮官のイベントコインを回収させていただきます。代わりに下記の報酬を配布させていただきます。": "本次活动“MS【赤之篇】前夜祭活动终于完结！”已结束，将回收指挥官的活动硬币，并改为发放以下奖励。",
        "「赤と青」フレーム": "“赤与蓝”头像框",
        "エディンバラフレーム": "爱丁堡头像框",
        "美食フレーム": "美食头像框",
        "【激射少女の元気弾－期間限定出現ガチャ】の40連確定報酬として獲得できる記念フレーム！サンディエゴをモチーフとしたデザイン！可愛い一品だ！": "作为【激射少女的活力弹－限时出现卡池】40连保底奖励可获得的纪念头像框！采用圣地亚哥为主题的设计！真可爱！",
        "【お金に夢中する修道姫－期間限定出現ガチャ】の40連確定報酬として獲得できる記念フレーム！エディンバラをモチーフとしたデザイン！": "作为【沉迷金钱的修道姬－限时出现卡池】40连保底奖励可获得的纪念头像框！采用爱丁堡为主题的设计！",
        "【第二期】美食マスターの道イベントにて、獲得できる限定フレーム！暖色を基調として、ポカポカした印象、美味しそうな一品！": "可在【第二期】美食大师之路活动中获得的限定头像框！以暖色为基调，给人温暖的印象，看起来非常美味！",
        "【夏の大運動会】ミッション報酬配布": "【夏日大运动会】任务奖励发放",
        "【夏の大運動会】ランキング報酬配布": "【夏日大运动会】排名奖励发放",
        "【夏の大運動会】ポイント報酬配布": "【夏日大运动会】点数奖励发放",
        "今回のイベントが終了したので、指揮官の受け取っていないミッション報酬を配布させていただきます。": "本次活动已结束，现发放指挥官尚未领取的任务奖励。",
        "今回のイベントが終了したので、指揮官の受け取っていないポイント報酬を配布させていただきます。": "本次活动已结束，现发放指挥官尚未领取的点数奖励。",
        "【夏の大運動会】シーズンⅠが終了したので、指揮官のランキング報酬を配布させていただきます。": "【夏日大运动会】赛季Ⅰ已结束，现发放指挥官的排名奖励。",
        "個人ランキング報酬配布": "发放个人排名奖励",
        "大艦隊ランキング報酬配布": "发放大舰队排名奖励",
        "【懸賞ミッション】イベントが終了したので、指揮官のPT報酬を配布させていただきます。": "【悬赏任务】活动已结束，现发放指挥官的PT奖励。",
        "【懸賞ミッション】イベントが終了したので、指揮官の個人ランキング報酬を配布させていただきます。": "【悬赏任务】活动已结束，现发放指挥官的个人排名奖励。",
        "【懸賞ミッション】イベントが終了したので、指揮官の大艦隊ランキング報酬を配布させていただきます。": "【悬赏任务】活动已结束，现发放指挥官的大舰队排名奖励。",
        "「美食マスターの道」イベントが終了しましたため、イベントアイテムを回収致します。以下の報酬を補填させていただきます。": "“美食大师之路”活动已结束，将回收活动道具。现补发以下奖励。",
        "突破MAXではない戦姫のみを表示します": "仅显示突破未达到MAX的战姬",
        "レベルMAXではない戦姫のみを表示します": "仅显示等级未达到MAX的战姬",
        "このイベントは既に始まりましたので、只今参加する事ができません": "该活动已经开始，当前无法参加",
        "受取る回数が使い切りました": "领取次数已用尽",
        "このミッションを受取るには、既に受取ったミッションをクリアする事が必要です。": "领取该任务前，必须先完成已领取的任务。",
        "受取済のプレイヤー": "已领取的玩家",
        "大艦隊PT報酬を受取るには、個人PTが指定数に到達する必要があります": "领取大舰队PT奖励需要个人PT达到指定数量",
        "ミッションの受取る回数：": "任务领取次数：",
        "このミッションはもう受け取りました": "该任务已经领取",
        "大艦隊ランキング": "大舰队排名",
        "ただいま開催期間ではありません。": "当前不在活动期间。",
        "購入回数の上限に到達しました。本日はこれ以上購入する事ができません": "已达到购买次数上限，今天无法继续购买。",
        "受取る": "领取",
        "1.「大艦隊レイドボス」イベントには東の拠点、南の拠点、西の拠点、北の拠点の4つの海域拠点が設置されています。任意の海域拠点に挑戦する事ができます。\n2.挑戦回数は毎日三回まで支給され、指定な任務をクリアする事で挑戦回数を獲得する事が可能です。挑戦回数が毎日0時にリセットされます。\n3.各海域拠点にて、敵対象にダメージを与える事で、その貢献度によって一定のレイドボスPTを入手する事ができます。敵対象へのダメージが高いほど、入手できるレイドボスPTが多くなります。大艦隊レイドボスPTとは、メンバー全員のレイドボスPTの合計になります。\n4.複数の隊員が同じボスを挑戦する場合は、戦闘終了の順番にダメージ量が計算されます。戦闘終了時、ボスの残りの耐久値が実際ダメージ量に不足している場合、ボスの残りの耐久値を最終のダメージ量とします。戦闘終了時、ボスが既に他の隊員に撃沈される場合、今回のダメージ量が計算されません、挑戦回数が消費されません。\n5.同名戦姫は一日に一度のみ出撃する事ができます。\n6.戦闘から脱退する場合は、挑戦回数が消費されません。\n7.比例ダメージとは、一度の戦闘で達成値までダメージを与える事で獲得できる追加報酬となります。\n8.敵対象を撃沈する事で、大艦隊の全メンバーに「撃沈報酬」を獲得する事ができます。※大艦隊の「レイドボス」画面をタップする事で、「撃沈報酬」を受け取る事ができます。\n9.イベント終了後、戦闘評価によって大艦隊の全メンバーに「評価報酬」を獲得する事ができます。\n10.※「大艦隊レイドボス」イベントの開催後に大艦隊に入隊した際は、次回のイベントから挑戦する事ができます。\n11.「大艦隊レイドボス」イベント期間中に大艦隊から脱退した際は、「大艦隊レイドボス」のイベント報酬を受け取る事はできません。": "1.“大舰队Raid Boss”活动设有东部据点、南部据点、西部据点、北部据点4个海域据点，可任意挑战海域据点。\n2.每天最多发放3次挑战次数，完成指定任务可获得挑战次数。挑战次数每天0点重置。\n3.在各海域据点对敌方目标造成伤害，可根据贡献度获得一定Raid Boss PT。对敌方目标造成的伤害越高，获得的Raid Boss PT越多。大舰队Raid Boss PT为全体成员Raid Boss PT之和。\n4.多人挑战同一BOSS时，按战斗结束顺序计算伤害。战斗结束时，若BOSS剩余耐久值低于实际伤害量，则以BOSS剩余耐久值作为最终伤害量。战斗结束时，若BOSS已被其他成员击沉，本次伤害不计入，挑战次数也不会消耗。\n5.同名战姬每天只能出击1次。\n6.中途退出战斗时不会消耗挑战次数。\n7.比例伤害是单场战斗造成达到目标值的伤害后可获得的追加奖励。\n8.击沉敌方目标后，大舰队全体成员都可获得“击沉奖励”。※点击大舰队的“Raid Boss”界面即可领取“击沉奖励”。\n9.活动结束后，根据战斗评价，大舰队全体成员可获得“评价奖励”。\n10.※在“大舰队Raid Boss”活动开始后加入大舰队，只能从下次活动开始挑战。\n11.活动期间退出大舰队后，无法领取“大舰队Raid Boss”活动奖励。",
        "1.EXモードをプレイいただく際は、任意の強化因子を選択し、難易度調整して、挑戦する事ができます。\n2.強化因子を多く選択するほど敵に有利な効果を付与します、慎重にお選びください。\n3.強化因子を選択してクリアすることで、★評価を獲得する事ができます。★評価ごとに報酬は異なります。★評価獲得後、強化因子を選択しなくても、EXモード海域をクリアする度に、歴代最高★評価上に記録された基準より報酬を獲得することができます。\n4.EXモードをクリア後、且つ、強化因子を選択してない場合、EXモード海域は安全状態となります；また任意の強化因子を選択した場合、極限状態となります。\n5.毎日クエスト全4種類のEXモードの★評価を総計し、総計した★評価分の報酬を獲得する事ができます。": "1.游玩EX模式时，可选择任意强化因子调整难度后进行挑战。\n2.选择的强化因子越多，就会赋予敌方越有利的效果，请谨慎选择。\n3.选择强化因子并通关后可获得★评价。不同★评价对应的奖励不同。获得★评价后，即使不选择强化因子，每次通关EX模式海域也可按照历史最高★评价记录的标准获得奖励。\n4.通关EX模式后，若未选择强化因子，EX模式海域将处于安全状态；选择任意强化因子后则进入极限状态。\n5.累计每日任务全部4种EX模式的★评价，即可获得对应总★评价的奖励。",
        "あたしの指揮官になるには、いろんな<color=#BE1F4D>【戦術】</color>をマスターすることが必須！": "想成为我的指挥官，必须掌握各种<color=#BE1F4D>【战术】</color>！",
        "練度をあげることが大事、ちなみに、あたしは休むより出撃したほうが好きよ！": "提升熟练度很重要。顺带一提，我比起休息更喜欢出击！",
        "コツを教えてやるわ！<color=#BE1F4D>【スキル素材】</color>は<color=#BE1F4D>【デイリークエスト】</color>で入手することができるよ": "教你个诀窍！<color=#BE1F4D>【技能素材】</color>可以在<color=#BE1F4D>【每日任务】</color>中获得。",
        "明日もちゃんとログインしなさい、いいことがあるかもしれないわ♪": "明天也要记得登录，说不定会有好事发生♪",
        "1.SKM（SenKiMemorial）はイベント開始当日からメイン画面の指揮官室の窓から見える海辺上に光る物体をタップして獲得する事ができます。\n2.毎日1個づづ漂着いたします。最大*15個のSKMが存在し、ランダムのSKMを獲得する事ができます。（イベント終了後も引き続き獲得する事が可能です）\n3.SKMを獲得する事で、イベントページのタグボタンの色が変化します。報酬を獲得できる合図となっております。お忘れなく獲得しましょう！\n4.イベント報酬は最大8個分です。\n5.獲得したSKMは倉庫にて、記録を確認する事ができます。": "1.SKM（SenKiMemorial）可从活动开始当天起，点击主界面指挥官室窗外海边可见的发光物体获得。\n2.每天会漂来1个，最多存在15个SKM，可随机获得。（活动结束后仍可继续获得）\n3.获得SKM后，活动页面标签按钮的颜色会变化，表示可以领取奖励。不要忘记领取！\n4.活动奖励最多可获得8个。\n5.获得的SKM可在仓库中查看记录。",
        "す、すごい…\nでも激射って一体なんなんですか…": "好、好厉害……\n但是，激射到底是什么啊……",
        "…じゃなくって！それよりも…\nオークランドさん、あの戦姫は…あなたたちの仲間？": "……不对！比起这个……\n奥克兰小姐，那位战姬是……你们的伙伴？",
        "私のいとこ…\nサンディエゴ姉さんだよ。": "我的表姐……\n是圣地亚哥姐姐。",
        "いとこ…？\nあれ？しかしあなたって確か…": "表姐……？\n咦？可是你不是……",
        "私も、よくわからなくて…": "我也不太清楚……",
        "それに、サンディエゴ姉さんは私たちと一緒にブルースフィアを出たわけじゃないのに。\nどうしてここに…": "而且，圣地亚哥姐姐明明没有和我们一起离开蓝星。\n为什么会在这里……",
        "どうしたの？オークランド、しばらく見ないうちにたじたじになったじゃないか！\nもしかして、激射の道を忘れたじゃないよね？それはいけないな！": "怎么了？奥克兰，才一段时间没见，你怎么变得畏畏缩缩了！\n该不会是忘了激射之道吧？那可不行！",
        "サンディエゴ姉さん！\nどうして…": "圣地亚哥姐姐！\n为什么……",
        "まあまあ、難しいことは後でいいから！\n今は、激射の道を歩んできた私たちの修行成果を試す時だよ！": "好啦好啦，复杂的事以后再说！\n现在是检验我们沿着激射之道修行成果的时候！",
        "…そ、そうだよね、サンディエゴ姉さん！": "……对、对啊，圣地亚哥姐姐！",
        "よ〜しっ！オークランド、忘れてないよね？あれで行くよ。\nそっちのムーバーのお姉さんも一緒に——": "好～！奥克兰，你没忘吧？我们就用那个出发。\n那边的搬运者姐姐也一起来——",
        "——えっ？\nな、なに…？": "——诶？\n什、什么……？",
        "さあ行くよ！": "出发！",
        "この間はお世話になりました。\n治療までさせていただいて、感謝してもしきれません。": "前段时间承蒙照顾。\n还劳烦您为我们治疗，实在感激不尽。",
        "もしできれば、ラフィーのケガ具合が少しよくなったら…\nここを発つつもりです。": "如果可以的话，等拉菲的伤势稍微好转……\n我们打算离开这里。",
        "どこへ行くつもり？": "你们打算去哪里？",
        "MCCに連れて行かれた私たちの仲間、\nモーリエを探しに行きます。": "我们要去寻找被MCC带走的伙伴，\n莫里埃。",
        "三人だけで？": "只有你们三个人？",
        "何か手がかりがある？": "有线索吗？",
        "時間が経てば経つほど、モーリエの身が危険になります。\n…それとも、みなさんは協力してくれるのですか？": "时间拖得越久，莫里埃就越危险。\n……还是说，大家愿意帮助我们？",
        "どこを探す？": "去哪里找？",
        "えっ？天……ゆい……\nえっと、ええええ～～～？？？": "诶？天……由依……\n呃，诶诶诶诶～～～？？？",
        "「オミクロン」か…ふん。\n悪くないじゃないか。": "“奥米克戎”吗……哼。\n还不赖嘛。",
        "——いいぞ、そう呼んでくれ。": "——很好，就这么叫我吧。",
        "せ、 先輩！ ？ ": "前、前辈！？",
        "いきなり一人で騒ぎ出して……大丈夫ですか？ ": "突然一个人吵闹起来……您没事吧？",
        "ふふ～\nやはり私の目には狂いがない。 ": "呵呵～\n看来我的眼光果然没错。",
        "……あいつ、 さっきこっちが選んだのをスルーしてたよな？ \nこれってゲームのキャラとして大丈夫なの？ ": "……那家伙刚才无视了我们选的选项吧？\n作为游戏角色这样真的没问题吗？",
        "こっちの火力支援にあまり拒絶する態度を見せていないね。\n前はあれほど「悪いお姉さん」だのなんだのと敵視していたのに。": "她似乎没有太抗拒我们的火力支援。\n明明之前还把我们视为“坏姐姐”之类的敌人。",
        "今はこっちの助けを受け入れるんだね。\nその理由……やはり気になるわ。": "现在却接受我们的帮助了。\n其中的理由……果然很让人在意。",
        "指揮官くん、これは確かに、ムーバー側から\n情報を手に入れるチャンスと思うわ。": "指挥官，这确实是从搬运者那边\n获取情报的机会。",
        "前線のみんなに、できるだけあのムーバーの駆逐艦ちゃんを\n被弾させないようにと伝えて。": "告诉前线的大家，尽量不要让那位搬运者驱逐舰\n受到炮击。",
        "今艤装付けていないに加えて、もともと駆逐艦なんだから。\n一発食らっても、相当危険だからね。": "她现在还没有装备舰装，而且本来就是驱逐舰。\n哪怕挨上一发，也会非常危险。",
        "……勇敢な選択だ。 ": "……勇敢的选择。",
        "うん、 逃げるなら、 最後まで逃げる。 \n唯一で最大な遺憾がそれなら、 望むところだよ。 ": "嗯，要逃就逃到最后。\n如果那是唯一且最大的遗憾，那正合我意。",
        "（うわっ瑞鶴さん今超cooooooool~な感じじゃない？ \nやはりこういうセリフは一度ちゃんとポーズをとって言ってみたかったよね！ ）": "（哇，瑞鹤现在是不是超——酷啊？\n果然很想摆好姿势说一次这样的台词呢！）",
        "はい、その通りでございます。": "是的，正如您所说。",
        "フッド様が警戒するのも当然のことです。\nしかし、それでも——": "胡德大人保持警惕也是理所当然的。\n然而，即便如此——",
        "どういうこと、 なになになんでいきなり爆発！ ？ ": "怎么回事，什么什么，为什么突然爆炸！？",
        "ボスモブ": "首领怪",
        "宇宙から来た侵略者だ！ ": "来自宇宙的侵略者！",
        "いきなり宇宙人が出てきたとか唐突すぎない！ ？ ": "突然冒出宇宙人，也太突兀了吧！？",
        "何を言う、 現実ではすでにほかの世界の者とさんざんやりあっていたじゃないか。 \nゲームで宇宙人が出てきたくらいどうってことないでしょう。 ": "说什么呢，现实里我们不是已经和其他世界的人打过很多交道了吗？\n游戏里出现宇宙人而已，没什么大不了的吧。",
        "いや、 そんなこと言われると返事に困るけど……": "不，被你这么一说，我还真不知道该怎么回答……",
        "とにかくうちの母星はもうすぐ滅ぶ。 \n生存のために、 今からこの星の侵略を開始する！ ": "总之，我们的母星马上就要毁灭了。\n为了生存，现在开始侵略这颗星球！",
        "こんな設定を最初から明かすなんて、 \n全然かわいそうに思えないんだけど！ ": "一开始就把这种设定说出来，\n我完全同情不起来啊！",
        "でもこの星の人間は面白いから人類の皆殺しはなしだ！ ": "不过这颗星球的人类很有趣，所以就不把人类全部杀光了！",
        "侵略者としてこんなに適当で本当にいいの！ ？ ": "作为侵略者，这么随便真的没问题吗！？",
        "野郎ども、 こいつをやっつけろ！ ": "喂，你们这些家伙，干掉他！",
        "モブたち": "路人们",
        "イーーーーーっは！ ！ ": "伊————哈！！",
        " 「ニゲルエネルギー」 の所有者として、 今この侵略者たちに対抗できるのは、 あんたしかいない。 ": "作为“尼格尔能量”的拥有者，现在能对抗这些侵略者的，\n只有你了。",
        "さあ、  「変身」 と叫び、 戦う力を手に入れるのだ。 ": "来吧，喊出“变身”，获得战斗的力量。",
        "いや、 あたし何といっても女の子だから、 もっとキラキラな呪文とかないの？ \n 「変身」 って……": "不管怎么说我也是女孩子，有没有更加闪闪发光的咒语？\n“变身”什么的……",
        "うわぁぁ！ ？ ": "呜哇啊啊！？",
        "……簡単に変身したけど。 \nそれになに、 この服？ ": "……就这么轻易地变身了。\n而且，这身衣服是什么？",
        "さっき 「変身」 と言った。 \nだからクリスマス守護ガール真紅メノウに変身した。 ": "你刚才说了“变身”。\n所以就变成了圣诞守护少女绯红玛瑙。",
        "知るか！ ！ こういう普段うっかり言い出した時の\n安全システムすらない変身、 誰が要るの！ ！ ": "谁知道啊！！这种平时一不小心说出口就会发生、\n连安全系统都没有的变身，谁想要啊！！",
        "モブA": "路人A",
        "あの……もういいですか？ 結構待っていたので……\nこの仕事受けた時、 こんなに待たされるとは聞いていないんだけど……": "那个……已经可以了吗？我等了很久……\n接这份工作时，可没人告诉我要等这么久……",
        " 「ニゲルエネルギー」 の所有者として、 今この侵略者たちに対抗できるのは、 \nあんたしかいない。 ": "作为“尼格尔能量”的拥有者，现在能对抗这些侵略者的，\n只有你了。",
        "っく、 このやらなくちゃっていうプレーシャーが、 \nあたしが逃げ続けていたことへの罰なのか……": "可恶，这种不得不战斗的压力，\n难道就是我一直逃避的惩罚吗……",
        "仕方ない、 普段の装備でやるしかないみたいね。 \nかかってこい！ ": "没办法，看来只能用平时的装备了。\n放马过来！",
        "グルグル——": "转啊转——",
        "\n\n END 5\n  『プロジェクト未定』 ": "\n\n END 5\n《项目未定》",
        "わかったわ。 \nなんにせよ、 その気遣いには感謝しておくわ。 ": "知道了。\n无论如何，感谢你的关心。",
        "そうだ、 そう言われてみれば、 もうすぐ春よね。 \n時間の移り変わりとは早いものだわ。 ": "对了，这么一说，春天马上就要到了呢。\n时间流逝得真快。",
        "そうですね。 \nいつの間にか一年の時間がすぎてしまったとは、 本当に感慨深いことです。 ": "是啊。\n不知不觉一年已经过去，真是令人感慨。",
        "それで最近のあのイベントの時にお祝いに何かするつもりはない？ ": "那么，最近那个活动期间，你们不打算做点什么庆祝吗？",
        "ふふ～聞いて驚かないでくださいね。 \n今年私たちはソロダンスの交流会を開催しようと考えています。 ": "呵呵～听了可别惊讶。\n今年我们打算举办单人舞蹈交流会。",
        "へー？ それは意外と風情のあるものだね。 \n高級なデザートを揃えて食べるだけのようなものかと思っていたが。 ": "哦？那还挺有情调的。\n我还以为只是准备各种高级甜点一起吃而已。",
        "あのようなリラックスできるものもいいと思いますが、 せっかくビスマルクさんとイータさんが\nいらしたので、 やはり何か特別なものにしたほうがいいかと。 ": "那种能让人放松的活动也不错，但毕竟俾斯麦小姐和伊塔小姐\n难得来了，我觉得还是安排点特别的比较好。",
        "正直、 このメンツでダンスが出来ないのは私だけとは予想外でした。 \nいったいどうしていつの間にかこんなお嬢様集団に入ったのか……": "说实话，没想到这群人里只有我不会跳舞。\n我到底是怎么在不知不觉间加入这种大小姐团体的……",
        "大丈夫ですよ、 イータ様。 昨日はもう基本ステップを覚えたでしょう？ \n次は熟達するまで練習すれば、 きっとなにも問題ありませんよ。 ": "没关系，伊塔大人。您昨天已经学会基本舞步了吧？\n接下来练习到熟练为止，一定不会有问题。",
        "私のあのアヒルみたいにぶるぶるする動きを、 あなたが手本見せた時の動きと比べてみると、 \nどう考えても問題大有りですが……": "把我那像鸭子一样抖个不停的动作，和您示范时的动作比较一下，\n无论怎么看问题都大了……",
        "帰って定期報告を済ませた後、 やはり早くこちらに戻って練習を続けた方がいいみたいですね。 \n本番の時にあまり恥を見せないといいですが。 ": "回去完成定期报告后，看来还是尽快回来继续练习比较好。\n希望正式上场时不要太丢脸。",
        "そういえば、 あのちょうどここにいない悪女もダンスがむかつくほど得意らしいね。 \n足を下ろす時毎回地面をすごい力で叩いて、 人を驚かせているのに。 ": "说起来，那个现在不在这里的坏女人似乎也擅长跳舞，厉害得让人火大。\n明明每次落脚都用很大力气踩地，把别人吓一跳。",
        "そういえば、 ちょうどいい機会ですね。 \nよかったら、 ヴェネトさんも参加してみませんか？ ": "说起来，这正是个好机会。\n如果您愿意，维内托小姐也来参加吧？",
        "昔からヴェネトさんがダンスに長けていると聞いていましたが、 \nなかなかお目にかかれる機会がなくて。 ": "我以前就听说维内托小姐很擅长跳舞，\n只是一直没有机会亲眼见识。",
        "まあ、 そんなこと、 淑女にとってはただの最低限の教養に過ぎないわ。 \n皆さんより少しだけ得意なだけよ。 ": "哎呀，那对淑女来说不过是最低限度的修养。\n只是比大家稍微擅长一点罢了。",
        "それはつまり来てくれるという意味ですか？ \n嬉しいです。 ": "也就是说，您愿意来参加吗？\n太好了。",
        "えっと、 別に参加しても構わないが、 何せわたくしは色々と忙しいのよ。 \nだから、 なんというか……": "呃，参加倒也不是不行，只是我平时很忙。\n所以，该怎么说呢……",
        "やはりだめ……ですか？ ": "果然还是不行……吗？",
        "うっ……こんなに真摯に誘ってくれるなら、 \nすこし時間を作ってあげるのも別に構わないわ。 ": "呜……既然你这么诚恳地邀请我，\n抽点时间出来倒也没关系。 ",
        "うん、 どうやら基本な礼儀は心配しなくてもいいようだね。 \nよろしい。 具体的にどんなことをする予定なの？ ": "嗯，看来基本礼仪方面不用担心了。\n很好。具体打算做些什么？",
        "ふふ、 それは当日の楽しみとして取っておきましょう。 ": "呵呵，这就留到当天再揭晓吧。 ",
        "そ、 それはよくないではないかしら？ \n参加者として、 わたくしも事前の準備を多少手伝うべきでしょう。 ": "这、这样不太好吧？\n作为参加者，我也应该帮忙做些前期准备。 ",
        "ただ格好付けて、 何もせずに他の人がしてくれるのを待つのは、 \n育ちの悪い成金みたいな輩がすることだわ。 ": "只顾装模作样，什么都不做，等着别人替自己完成，\n那是没教养的暴发户才会做的事。 ",
        "そんな風に言われても……": "你这么说也……",
        "躊躇うことはないでしょう。 このヴェネト様が手伝うと言ったのよ、 何も問題ないでしょう。 \nさあ、 はやく洗いざらい全部吐きなさい。 ": "没什么好犹豫的。这位维内托大人都说要帮忙了，能有什么问题。\n来，快把所有计划一五一十地说出来。 ",
        "（ ……この人、 なんだか言動が少し怪しいですね ）": "（……这个人，总觉得言行有点可疑）",
        "（ イータ様、 人見知りだからって、 \nあんな風に睨みつけると向こうが怖がりますよ ）": "（伊塔大人，您就算怕生，\n那样瞪着别人也会把对方吓到的）",
        "（ そこまで睨んでいないでしょう！ ？  ）": "（我哪有瞪得那么厉害！？）",
        "うぅ……これが大体な予定です。 \nどうか負担にならない範囲でお願いします。 ": "呜……这就是大致计划。\n请在不会给您造成负担的范围内帮忙。 ",
        "もちろん、 上客としてわたくしの本分はイベントを楽しむことだから、 \nなにもかも代わってしまい、 あなたの立場を乗っ取ったりしないわ。 ": "当然，作为贵客，我的本分是享受活动，\n不会把所有事情都替你做掉，更不会取代你的立场。 ",
        "あ～急用を思い出した～\n今すぐ行かなくちゃいけないことだから、 これで失礼するね～": "啊～我想起有急事了～\n必须马上去处理，先失陪啦～",
        "えっ！ ？ そんなに急いでいるのですか？ \nせめてお茶を一杯——": "诶！？有那么急吗？\n至少喝杯茶——",
        "……慌てていってしまいましたね。 ": "……她慌慌张张地走了。 ",
        "あの人、 やはり怪しいでしょう？ \nまるで交流会の予定を聞くためだけに来たようです。 ": "那个人果然很可疑吧？\n简直像是专程来打听交流会计划的。 ",
        "もしかすると、 何か他の人に言い辛い事情があるかもしれません……": "说不定她有什么难以对别人说的隐情……",
        "ただいま。 おや？ どうしたの、 みんな揃って変な顔して。 \nわたしが手続きを済ませた間、 何かあったのですか？ ": "我回来了。咦？怎么了，大家都摆着奇怪的表情。\n我去办手续的时候，发生什么事了吗？ ",
        "あっ、 ロドニーさん、 お疲れ様でした。 \n実は、 先ほどヴェネトさんが——": "啊，罗德尼小姐，辛苦了。\n其实，刚才维内托小姐她——",
        "最後は優勝賞品の巨大ぬいぐるみっと……よし！ ": "最后是冠军奖品的巨大毛绒玩偶……好！ ",
        "さすがにこの短期間で、 どんなぬいぐるみがいいのかまで聞くには無理がある……\n仕方がない、 再審査の時の補完項目にしておこうか。 ": "这么短的时间内，连大家喜欢什么样的毛绒玩偶都去问，确实不现实……\n没办法，留到复审时再补充吧。 ",
        "あの頭の高い野蛮女も 「ないよりまし」 と分かるでしょうし、 もうすぐ締め切りのこの時に、 \nわざわざけちをつけるようなことはしないでしょう。 ": "那个傲慢的野蛮女人也知道“总比没有好”，在马上截止的现在，\n应该不会特意挑三拣四。 ",
        "……にしても、 今回をきっかけにこの基地を一回りしてみたら、 変人が多すぎないか？ \nたとえば——": "……话说回来，借这次机会在基地转了一圈，怪人是不是太多了？\n比如——",
        "え～？ イベントするの～？ それともしないの～？ 申請書を出すためじゃないなら、 \n急いで決めないとだめなことでもない、 だよね～？ ": "诶～？要办活动吗～？还是不办呢～？如果不是为了提交申请书，\n也不是必须马上决定的事吧～？ ",
        "ヴェネトお姉様！ そんなことより、 どうやってヴェネトお姉様みたいに\n優雅でセクシーになれるのか、 教えてもらえませんか！ ？ やはり太ももあたりが肝心？ ": "维内托姐姐！比起那些，能不能教教我怎样才能像维内托姐姐一样\n优雅又性感！？果然大腿才是关键？ ",
        "平海離してよ！ \nこのいきなり 「キャラかぶり」 とか言い出すやつに礼儀というものを思い知らせてやる！ ": "平海，放开我！\n我要让这个突然说什么“角色重叠”的家伙明白什么叫礼貌！ ",
        "ヴェネト殿！ 早く頭を低くしてください！ ": "维内托殿下！请快低下头！ ",
        "今研究会で——": "我现在在研究会——",
        "っひ！ 今思い出してもまで冷や汗が止まらない……": "咿！现在回想起来还是冷汗直流……",
        "まったく、 それもこれも\n統括する者がちゃんと仕事を出来ていないせいだわ。 あんなふざけた輩を放置するなんて。 ": "真是的，这一切都是因为负责统筹的人没有好好工作。\n居然放任那种胡闹的家伙不管。 ",
        "はぁ、 もうこんな時間。 残りは明日いち早くするしかないみたいね。 \n一応まだ何が残っているのかチェック——": "唉，已经这么晚了。剩下的看来只能明天尽早完成。\n先检查一下还有什么没做完——",
        "あれ？ この 「運命と英知」 、 二十四時間営業って書いてある？ \n責任者は……デューク・オブ・ヨーク？ ": "咦？这个“命运与睿智”写着二十四小时营业？\n负责人是……约克公爵？ ",
        "確か、 占いのおばあちゃんみたいなことをやっている人だったね。 まったく、 \nこんな子供騙しのものにこんなにいっぱいのコメントがあるとは、 ここの人はセンスがないね。 ": "我记得她好像在做类似算命婆婆的事。真是的，\n这种骗小孩的东西居然有这么多评价，这里的人真没品味。 ",
        "まあ、 そんなことはどうでもいいわ。 \n二十四時間営業なら、 今から向かっても構わないよね。 ": "算了，那些都无所谓。\n既然二十四小时营业，现在过去也没关系吧。 ",
        "ヴィットリオ・ヴェネトのご来訪よ！ \n扉を開けなさい。 ": "维托里奥·维内托来访！\n开门。 ",
        "聞き覚えのない女性の声": "陌生女性的声音",
        "待っていた。 入れ。 ": "等你很久了。进来。 ",
        "もう役になっているの？ \n思っていたより職人精神があるね。 ": "已经进入角色了吗？\n没想到你还挺有匠人精神。 ",
        "どんなことを生業にしても、 仕事に真面目な人には高く評価するわ。 \n遅い時間に失礼するよ。 ": "无论从事什么工作，我都很欣赏认真工作的人。\n这么晚来打扰了。 ",
        "変なポーズを取る女性": "摆出奇怪姿势的女性",
        "ふん、 やはり一秒の狂いもないか。 \nあの魔女、 確かになかなかの腕前だ。 ": "哼，果然一秒都不差。\n那个魔女，确实相当有本事。 ",
        "あれ？ あなたはデューク・オブ・ヨーク……ではないよね？ \nすこし癖のある金色のロングヘヤのはず——": "咦？你不是约克公爵……吧？\n她应该有一头很有特色的金色长发——",
        "これは魔法ではない。 \nもちろんあなたがその呼び方を変えることもない。 ": "这不是魔法。\n当然，也不是你改变称呼的结果。 ",
        "ヒー！ ！ \nいきなり背後から出てきて出口を塞ぐとはどういうつもり！ ？ ": "咿！！\n突然从背后出现，还堵住出口，你想干什么！？ ",
        "ことが起こる前に暇を持て余しているから、 \n散歩に出かけて、時間通りに帰ってきただけ。 ": "事情发生前我闲得没事做，\n所以出去散步，只是按时回来了。 ",
        "わわわわたくしはこんなインチキに惑わされないわ！ \nどうせああいう事前に手配しておいた演出みたいなものでしょう！ ？ ": "我我我我才不会被这种骗术迷惑！\n反正就是事先安排好的表演吧！？ ",
        "あれは…！": "那是……！",
        "信号弾だ！\nオークランドたちが成功した！": "是信号弹！\n奥克兰她们成功了！",
        "よかった…": "太好了……",
        "——ちっ、グズグズしやがって。": "——啧，磨磨蹭蹭的。",
        "オークランドたちは成功したようね。こちらもさっさと退散しましょう。\n弾薬も底尽きたし、私たちが捕まったら本末転倒だわ。": "奥克兰她们似乎成功了。我们也赶紧撤退吧。\n弹药也用光了，要是我们被抓就本末倒置了。 ",
        "言わなくてもわかってる。\nだけどな…！": "不用你说我也知道。\n但是……！",
        "こっちにも敵が現れたの！？\nこれじゃさすがにまずいわね…": "这边也出现敌人了！？\n这样下去可不妙……",
        "その言葉、待ってました！": "我等的就是这句话！",
        "うわああ！？": "呜哇啊啊！？",
        "どうでした？同胞が危機に瀕する時に救いの手を差し伸べるわたくしの姿！\n称賛に値しますよね。": "怎么样？在同胞陷入危机时伸出援手的我！\n值得称赞吧。 ",
        "ここで姿を見せるべきか迷っていたが…\n確かに高みの見物をする場合じゃなさそうだわ。": "我本来还在犹豫是否该现身……\n现在确实不是袖手旁观的时候。 ",
        "エセックス、絶好なタイミングだ。\nよくできました。": "埃塞克斯，时机正好。\n做得很好。 ",
        "ふふ、ありがとう〜\nやはり褒められると、ぐっとやる気が出ますね。": "呵呵，谢谢～\n果然一被夸奖，干劲就一下子上来了。 ",
        "あなたたちは…": "你们是……",
        "…一応心当たりがあります。\n一つ一つ回るつもりです。": "……我大致有些头绪。\n打算一个个去确认。 ",
        "ついに追いついたのね。あれでしょう？\nいたいけな少女から艤装を奪った悪いやつめ～": "终于追上来了。就是那个吧？\n从惹人怜爱的少女手里夺走舰装的坏家伙～",
        "ほっほ～……拘束を実行しているあの二隻も行動できないらしいね。": "哦哦～……看来那两艘正在执行拘束的船也无法行动。",
        "でもあの<color=#f14966>【警戒範囲】</color>には近づかないでね。\n危険な雰囲気がするわ。": "不过别靠近那个<color=#f14966>【警戒范围】</color>哦。\n感觉很危险。",
        "まずは<color=#f14966>【警戒範囲】</color>の外からあの二体をやっつけたほうが妥当よ。": "首先从<color=#f14966>【警戒范围】</color>外解决那两个比较妥当。",
        "うんうん、 終わりよければ全てよし、 だっけ？ ": "嗯嗯，结果好一切都好，是这么说的吧？ ",
        "私の理性が 「おかしいでしょう！ 」 って叫んでるけど、 \nもうツッコミの気力がありません……": "我的理智一直在喊“这不对吧！”，\n但我已经没有吐槽的力气了……",
        "瑞鶴さま、 さあ、 深呼吸してください。 そしてスーッと吐いて。 \nリラックスです。 もう全部終わりましたよ。 ": "瑞鹤大人，来，深呼吸。然后慢慢呼气。\n放松，已经全部结束了。 ",
        "あなたは…エセックス、ソサエティのエセックスなのか？\nそっちはボルチモア…どうしてここに？": "你是……埃塞克斯，Society的埃塞克斯？\n那边是巴尔的摩……你们为什么会在这里？",
        "こちらMC13-0426-0027、異常な…ん？": "这里是MC13-0426-0027，异常……嗯？",
        "予想外の組み合わせが！": "意料之外的组合！",
        "予想外の場所で！！": "在意料之外的地方！！",
        "予想外に登場…": "意外登场……",
        "——って、なぜ僕がこんなセリフを言わなきゃならないんだ！！！": "——等等，为什么我非得说这种台词不可！！！",
        "これも作戦の一部だ。\nそしてこの作戦は、あなたの仲間を助けるためのもの。": "这也是作战的一部分。\n而这次作战是为了帮助你的伙伴。",
        "話せば長くなるから、できればゆっくりできる場所で、\nエリザベスさんと思い出を語り合いたいですね。": "说来话长，如果可以的话，我想找个能慢慢聊的地方，\n和伊丽莎白小姐一起谈谈回忆。",
        "とにかく、私たちは今あなたたちを敵に回すつもりはありませんよ。\nこのことだけ、伝えておきましょう。": "总之，我们现在没有和你们为敌的打算。\n我只把这件事告诉你们。",
        "利害は一致してるし、一時手を結んでも問題はないだろう？\nクイーン・エリザベス。": "我们的利益一致，暂时联手也没问题吧？\n伊丽莎白女王。",
        "…その言葉、信じていいわよね、ボルチモア？": "……这句话，我可以相信吧，巴尔的摩？",
        "ソサエティの名にかけて、\nこの包囲網の突破に助力致そう。": "以Society之名，\n我会协助你们突破包围网。",
        "あなたたちが合流する予定の仲間の元に、\n私たちとはぐれた仲間もいると思いますからね。": "你们准备会合的伙伴那里，\n应该也有与我们走散的伙伴。",
        "…なるほど。\nわかったわ。私についてきて！": "……原来如此。\n知道了，跟我来！",
        "エリザベスでさえ頑張っているのに、\nあなたももっと合わせるべき。": "连伊丽莎白都在努力，\n你也应该更加配合。",
        "ううっ……………": "呜呜……………",
        "恥ずかしいいぃぃぃ……": "好丢脸啊啊啊……",
        "…おかしい。なぜ反応しない？\nまさかこれでも目立たないのか？": "……奇怪，为什么没有反应？\n难道这样还不够显眼吗？",
        "…て…て…敵……": "……敌、敌人……",
        "——敵襲だ！！！\n変な敵が現れたぞ！！！": "——敌袭！！！\n奇怪的敌人出现了！！！",
        "でも…錬金術って、本当に存在するの？": "但是……炼金术真的存在吗？",
        "やったね。\nそれじゃあ早速、撤退しよう～": "成功了。\n那我们马上撤退吧～",
        "でも油断しちゃだめなのよ～\nよく見てごらん、あの駆逐艦の艤装……": "但可不能大意哦～\n仔细看看那艘驱逐舰的舰装……",
        "勝手に行動しているらしいね……\n誰も操作していないのに。": "看来它在自行行动……\n明明没有人在操纵。",
        "しかも、あの駆逐艦ちゃんに向かって進みながら、\n魚雷で他の戦鬼を攻撃している……": "而且它一边朝那艘驱逐舰前进，\n一边用鱼雷攻击其他战姬……",
        "かわいいことね。\nこんな素敵なペット、朝日も欲しくなちゃったわ。": "真可爱。\n朝日也想要这样一只棒棒的宠物了。",
        "ふわぁ……赤城先輩も本当手加減しないね。 かわいい後輩を気絶させるなんて。 \nうぅぅまだ頭ふらふらする……": "呼啊……赤城前辈真是一点也不手下留情。居然把可爱的后辈打晕。\n呜呜呜，头还在晕……",
        "……にしても、 うちの学園の保健室って、 結構豪華だな。 ": "……话说回来，我们学院的医务室还挺豪华的。 ",
        "んー、 名門校だしな。 \n血の気が多い生徒も結構いるし、 保健室に金をかけるのも納得ね。 ": "嗯，毕竟是名校。\\n而且精力旺盛的学生也不少，在医务室上多花点钱也合理。 ",
        "でもそう考えると、 なんで年末のイベントで資金不足になるの？ \nあの超がつくほどのお嬢様たちがちょっと家からもらってくれば解決じゃない。 ": "但这样想的话，为什么年末活动会资金不足？\\n那些超级大小姐们从家里拿一点钱来不就解决了吗。 ",
        "おや、 瑞鶴さん、 お目覚めですか。 ": "哎呀，瑞鹤小姐，您醒了吗？ ",
        "気分はいかがですか？ 検査した時あまり目立った外傷がありませんでしたが、 \n油断は禁物です。 どこが痛いところがあったら、 すぐ先生に言うんですよ？ ": "感觉怎么样？检查时没有发现明显外伤，\\n但不能掉以轻心。哪里疼的话，要马上告诉老师哦？ ",
        "はい！ この瑞鶴、 \nセクシーな保健の先生の胸か太ももに顔を埋めなければ治らない病気を患っております！ ": "是！我瑞鹤患上了一种病，\\n只有把脸埋进性感保健老师的胸口或大腿才能治好！ ",
        "どうか慈悲深い心でこのあたしの命を助けてください！ ": "请您发发慈悲，救救我这条小命吧！ ",
        "ふふ、 どうやらもう大丈夫みたいですね。 ": "呵呵，看来已经没事了。 ",
        "ですが、 そういう冗談は誰にでも\n言っていいものではありませんよ？ ": "但是，这种玩笑可不是对谁都能说的哦？",
        "自分も女の子だから大丈夫と思っているようですが、 \nいつか本当にトラブルになりかねませんよ。 ": "你似乎觉得自己也是女孩子，所以没关系，\\n但总有一天可能真的惹上麻烦。 ",
        "もちろん、 分かってますよ。 \nあたしがこんな風に甘えるのは、 心の広い美人のお姉さんだけです。 ": "当然，我明白。\\n我只会向心胸宽广的漂亮姐姐这样撒娇。 ",
        "ジョージ5世教頭先生": "乔治五世副校长老师",
        "うぅ……騒がしいわ……": "呜……好吵……",
        "確か、 校門前に来たら急に首あたりが痛くなって……いけない、 今は何時！ ？ ": "我记得来到校门前后脖子突然疼了起来……糟糕，现在几点了！？ ",
        "うわ！ ？ 教頭先生？ \n今朝校門前で疲れで倒れたと聞いたけど、 保健室に運ばれたのか。 ": "哇！？副校长老师？\\n听说她今天早上因为疲劳在校门前晕倒，被送到医务室了吗？ ",
        "おやおや、 教頭先生、 いけませんよ。 ": "哎呀呀，副校长老师，这可不行。 ",
        "先生は疲労で貧血症状が起きています。 \nかなりひどいですよ、 今はちゃんと休まないと。 ": "老师因为疲劳出现了贫血症状。\\n情况相当严重，现在必须好好休息。 ",
        "最高責任者として、 私がいなければ話になりません。 ": "作为最高负责人，没有我可不行。 ",
        "……実は、 先生がいないほうが、 事態はうまく収まるのですが。 ": "……其实，没有老师在，事情反而会更顺利。 ",
        "え～い❤": "嘿～❤",
        "バチン！ ！ ": "啪！！ ",
        "うぃぃぃ……": "呜呜呜……",
        "バタン。 ": "砰。 ",
        "あっ、 ああぁぁあ……あ、 あの、 高雄せんせい……\nあたし、 もう大丈夫だと思うので、 これで失礼いたしま……": "啊，啊啊啊……那、那个，高雄老师……\n我觉得自己已经没事了，先告辞了……",
        "瑞鶴さん、 お待ちください。 ": "瑞鹤小姐，请留步。 ",
        "はい！ \n何でもしますのでどうか命だけは！ ！ ！ ": "是！\\n我什么都做，请饶我一命！！！ ",
        "ふふ、 ご冗談を。 別に今、 教頭先生の座を手に入れるために\n彼女を仕留めた現場を目撃されたわけでもないのに——ね？ ": "呵呵，开个玩笑。又不是被你撞见我为了夺取副校长之位\\n而干掉她的现场——对吧？ ",
        "ヒィィイ——！ ！ ？ ？ ": "咿——！！？？",
        "え！ ？ でもあたしは来た初日に瑞鶴さまに殺されかけたけど！ ？ \n人のこと怖いとか言うの！ ？ ": "诶！？可我第一天来就差点被瑞鹤大人杀掉啊！？\\n你居然说别人可怕！？ ",
        "あ、 あれはただの気の迷いだ。 ": "那、那只是我一时糊涂。 ",
        "気の迷いで殺されてたまるか！ ゲームの中のこいつ、 少なくともちゃんとことを解決したんだ！ \n瑞鶴さまもその辺見習ってくださいよ！ ": "一时糊涂就能杀人吗！游戏里的这个家伙至少把事情解决了！\\n瑞鹤大人也请学学这一点！ ",
        "……返す言葉もない。 ": "……无言以对。 ",
        "とにかく、 どうやらこのエンディングでは、 他の人が何とかしてくれたそうです。 \nつまり、 事件を解決するのはヒロインじゃなくてもいい、 ということですか？ ": "总之，看来在这个结局里，是其他人设法解决了问题。\\n也就是说，不一定非要由女主角来解决事件？ ",
        "冗談です。 \n説明の前に、 まずはこちらの計画書に目を通してもらえませんか？ ": "开玩笑的。\\n说明之前，能先看看这份计划书吗？ ",
        "これは……年末のイベントの企画書？ \nそれもクリスマスと新年をそれぞれテーマにしたものが一つずつ？ ": "这是……年末活动企划书？\\n而且圣诞节和新年主题的各有一份？ ",
        "うわ、 でもこれ完成度結構高いな。 ": "哇，不过这份完成度还挺高。 ",
        "会場の設置と資材の統計まで全部終わってる……\n本当に投票で決めるなら、 どっちかは確実に廃棄されるのに。 ": "连会场布置和物资统计都完成了……\\n如果真的通过投票决定，其中一份肯定会被废弃。 ",
        "……瑞鶴さん、 こんなうわさを耳にしたことありませんか？ \n教頭先生は学園長の座を簒奪しようとしているっと。 ": "……瑞鹤小姐，您听说过这样的传闻吗？\\n听说副校长老师想要篡夺学院长之位。 ",
        "えっと……聞いたことがあるかどうかと言われても……\nそもそも教頭先生、 この学園に来た初日からそれを隠す気ないですよね？ ": "呃……要说有没有听过……\\n副校长老师从来到学院第一天起就没打算隐藏这件事吧？ ",
        "何が起こるかと結構気になっていたけど、 \n結局何事もなくて、 拍子抜けしました。 ": "我本来很在意会发生什么，\\n结果什么事都没发生，反而有点失望。 ",
        "そうですね。 本当を言いますと、 就職初日で学園長のやり方に口を出すだけでなく、 \n自分ならもっといい結果を出せると宣言する人がいるとは思いませんでした。 ": "是啊。说实话，我没想到有人在就任第一天不仅对学院长的做法指手画脚，\\n还宣称自己能做得更好。 ",
        "わたくし、 あの時思うのですが——": "我当时在想——",
        "この女、 目ざわりですね~っと♪": "这个女人真碍眼呢～♪",
        "あんたの言う通りだな。 ": "你说得对。 ",
        "その前に、 ちょっと水を飲んでくる。 ": "在那之前，我去喝点水。 ",
        "ふふ、 どうやら瑞鶴さんも見た目ほど大胆不敵なわけではないようです。 \nなら冗談はこの辺にして、 ちゃんと説明しますか。 ": "呵呵，看来瑞鹤小姐也没有外表看起来那么大胆无畏。\\n那就先开到这里，认真说明吧。 ",
        "そのあと、 わたくしはその人の弱点を——あっ、 \nここは同僚として交流を深めたいと言ったほうがいいですかね。 ": "之后，我就去找那个人的弱点——啊，\\n这里还是说想以同事身份加深交流比较好吧。 ",
        "とりあえず、 少しの間で、 ジョージ5世という人がどんな人なのか、 \nすぐに理解しました。 ": "总之，没过多久，我就明白乔治五世是怎样的人了。 ",
        "才能を見せれば認めてもらえると思っている。 \nどうしたらことが完璧に進むか、 それを考えるだけで幸せそうな笑顔を見せる。 ": "她认为只要展现才能就能得到认可。\\n光是思考如何让事情完美进行，就会露出幸福的笑容。 ",
        "——そんな、 簡単で単純な女の子です。 ": "——就是这样一个简单单纯的女孩。 ",
        "学園長が自分に取って代わろうとする彼女に信頼を置くのも、 出張で留守を余儀なくされる時に\n彼女を代役として選んだのも、 納得します。 ……うれしくはないですが。 ": "学院长信任这个试图取代自己的人，在出差不得不离开时选择她代任，我也能理解。\\n……但我并不高兴。 ",
        "……しかし、 やはり彼女だけに任せるのは限界があります。 ": "……不过，果然还是不能把一切交给她。 ",
        "たとえどんな万全の準備をしたとしても、 \n皆さんがそもそも投票で決めること自体に賛同しないなら、 すべてが水の泡になります。 ": "无论准备得多么周全，\\n如果大家根本不赞成通过投票决定，一切都会化为泡影。 ",
        "お邪魔します！ ": "打扰了！ ",
        "高雄先生——あれ、 瑞鶴さんもいらっしゃるのですか？ ": "高雄老师——咦，瑞鹤小姐也在吗？ ",
        "アラバマ先生！ ？ 歴史の授業と美しいヒップラインをありがとうございますでも\nここは早く逃げたほうが——": "阿拉巴马老师！？感谢您的历史课和美丽的臀部曲线，不过\\n我们还是赶紧逃走比较好——",
        "おや、 瑞鶴さん、 なんだか声が少しかすれていませんか？ \nのどの様子を見ますね。 はい、 口を大きく開けて——": "哎呀，瑞鹤小姐，您的声音好像有点沙哑？\\n让我看看喉咙。来，把嘴张大——",
        "うぐ！ ？ あが！ ？ ": "呜咕！？啊嘎！？ ",
        "あの、 治療中ですか？ \nならまた改めて……": "那个，正在治疗吗？\\n那我之后再来……",
        "ふふ、 構いませんよ。 \n何か用がありましたらどうぞ。 ": "呵呵，没关系。\\n有什么事请说。 ",
        "そうですか……分かりました。 \n実は赤城さんのことです。 ": "这样啊……知道了。\\n其实是关于赤城小姐的事。 ",
        "既に卒業した赤城さんはどうしてなのか、 学園に戻ってきました。 \nそれだけでなく、 多くの生徒を集めてもうすぐ始まる投票に反対しています。 ": "已经毕业的赤城小姐不知为何回到了学院。\\n不仅如此，她还召集许多学生，反对即将开始的投票。 ",
        "おや、 それは大変ですね。 ": "哎呀，那可真是麻烦。 ",
        "……うぅ……ひっく……いきなり熱が出たなんて……": "……呜……抽泣……居然突然发烧……",
        "？ \nどういう意味？ ": "？\\n什么意思？ ",
        "トントントン": "咚咚咚",
        "ど、 どなたかしら？ \n（ ああタオルタオル早く顔を拭かなきゃ！  ）": "谁、谁啊？\\n（啊，毛巾毛巾，得赶快把脸擦干净！）",
        "ヴェネトさん、 お休みのところ失礼いたします。 フッドです。 \n風邪をひかれて体調を崩したと聞いたので、 お見舞いに参りました。 ": "维内托小姐，休息时打扰了。我是胡德。\\n听说您感冒身体不适，所以前来探望。 ",
        "そうなの？ 実は別にたいしたことではないが……\n（ 枕を戻してシーツを整えて早く早く！  ）": "是吗？其实没什么大不了的……\\n（快把枕头放回去，把床单整理好！）",
        "さらに生徒たちから聞いた話ですが、 たとえ許可がもらえなくても\n自分が責任者として、 去年と同規模のイベントを開催しようとしているとか……": "另外，我还听学生们说，就算得不到许可，\\n她也打算以负责人身份举办与去年同规模的活动……",
        "あぁ、 それは悪くないかもしれませんね？ ": "啊，那或许也不错？ ",
        "ふふ、 アラバマ先生、 そんなに緊張しなくても。 \n何とかなりますよ。 ": "呵呵，阿拉巴马老师，不用这么紧张。\\n总会有办法的。 ",
        "ぐぅえ！ ？ ": "呜诶！？ ",
        "ブルースフィアのワルモノ！\n一体何をして…": "蓝星的大坏蛋！\n你到底在做什么……",
        "おお！\n貫通したぞ！": "哦！\n贯穿了！",
        "では、ルームメイトよ、先に失礼する！\n": "那么，室友，我先告辞了！\n",
        "また会おう！": "再会！",
        "…………………………えっ？": "…………………………诶？",
        "待っ…えええ！？\nいつ拘束から抜け出したの！？": "等……诶诶诶！？\n你什么时候从拘束中逃出来的！？",
        "うそ、ラフィーも連れて行ってよ！？\nブルースフィアの大ワルモノ——！！！": "骗人，带上拉菲一起走啊！？\n蓝星的大坏蛋——！！！",
        "…あれ？\nなんか…………………………いや、気のせいっか。": "……咦？\n总觉得…………………………不，应该是错觉。",
        "ん？\nおまえは…": "嗯？\n你是……",
        "…シリアルコードMF13-H-0081。\n救援信号を受け、増援に来ました。": "……序列号MF13-H-0081。\n收到救援信号，前来增援。",
        "はあ…？\n聞いたことのない型番っすね。": "哈……？\n没听说过的型号呢。",
        "下層兵士がそんなこと聞いてどうするつもり？\n速やかに現在状況の報告を。": "下层士兵问这个做什么？\n立即报告当前状况。",
        "……！！\nはっ、申し訳ないっす！": "……！！\n是，十分抱歉！",
        "現在、正体不明の敵から攻撃を受けてるっす。\n正面戦闘を避けながら、地形を利用してるんで、実に狡猾な相手っす！": "目前正遭到身份不明的敌人攻击。\n对方避开正面战斗并利用地形，实在是狡猾的对手！",
        "あぁ！でもみんな迎撃に派遣されたから、捕まるのも時間の問題かと。\nはい！": "啊！但大家都被派去迎击了，我们被抓只是时间问题。\n是！",
        "反乱者を移送していると聞いたが、それが敵の狙いかもしれません。\n問題がないでしょうね？": "听说正在转移叛乱者，那可能就是敌人的目标。\n应该没问题吧？",
        "はっ！\n裏切り者とブルースフィアの捕虜はしっかりそっちに拘束して…んん？": "是！\n叛徒和蓝星俘虏已经在那里牢牢拘束……嗯？",
        "…どうしました？": "……怎么了？",
        "…あの、その方も貴官の同行者っすか？": "……那个，那位也是您的同行者吗？",
        "…その通り。\nシリアルコードMF13-O-0135、私と同型の者です。": "……正是。\n序列号MF13-O-0135，与我是同型号。",
        "そう、私はMF…M………………………………\nえっと…": "对，我是MF……M………………………………\n呃……",
        "MFなんだっけ": "MF什么来着",
        "…そう！\n彼女の同型なんだ。": "……对！\n和她是同型号。",
        "（ドキドキ、そわそわ…）": "（心怦怦跳，坐立不安……）",
        "…おまえら、やっぱ怪しいっす！\nそこから動くな、上官に報告する…！！": "……你们果然很可疑！\n不许动，我要向上级报告……！！",
        "（ど…どうしよう、イータさん！）": "（怎、怎么办，伊塔小姐！）",
        "（エリザベスたちの援護のおかげで潜入できたけど、\nこのままじゃバレてしまうよ！）": "（多亏伊丽莎白她们掩护，我们才能潜入，\n但这样下去会暴露的！）",
        "（だ…大丈夫！慌てないで！）": "（没、没关系！别慌！）",
        "（近くに他の敵はいなさそうです。\nかくなる上は…！）": "（附近似乎没有其他敌人。\n既然如此……！）",
        "何事ですか。\n何を騒いでいる？": "出什么事了？\n你们在吵什么？",
        "隊長！\nいいところに来てくれたっす！": "队长！\n您来得正好！",
        "この二人、増援に来た新型って言うけど…やっぱ怪しいっす！\n特に変な服を着てるあいつ、どう見ても味方じゃないっしょ！！": "这两人说是来增援的新型机……果然很可疑！\n尤其是那个穿奇怪衣服的家伙，怎么看都不像自己人吧！！",
        "……へへ、 瑞鶴、 投票は今日の午後って言ったよね。 じゃあ、 一緒にいくよ。 \nもうすぐすっごくにぎやかになるって予感がするのよ。 ": "……嘿嘿，瑞鹤，你说投票在今天下午吧。那就一起去。\\n我有预感，那里马上就会变得非常热闹。 ",
        "えっと……先輩、 本当に行くの？ いや、 伊勢先輩のことだから、 絶対行くと思うけど、 \nただ見に行くだけですよね？ 火に油を注ぐみたいなことはしませんよね？ ": "呃……前辈，真的要去吗？不，按伊势前辈的性格，我知道您肯定会去，\\n但只是去看看吧？不会做火上浇油的事吧？ ",
        "このすき焼きという料理は、本当においしいですね……！": "寿喜烧这道料理真的很好吃……！",
        "へへ、グナイゼナウさんのお口に合ってよかったです！": "嘿嘿，能合格奈泽瑙小姐的口味太好了！",
        "……でも、私も情けないですね……\n島風ちゃんみたいな子供にまで心配をかけるなんて。": "……不过，我也真没用……\n居然让岛风这样的孩子都担心了。",
        "いえいえ、私はただ念のためにと思って、グナイゼナウさんに話しかけて\nみただけですから……こちらこそ、いらぬお節介でなければいいのですが。": "不不，我只是想着以防万一，才试着和格奈泽瑙小姐搭话，\n希望没有给您添不必要的麻烦……",
        "そんなことありません。島風ちゃんは元気のない私に気付いて、\n親切に話しかけてくれたのです。とても感謝しています。": "没有那回事。岛风注意到我没精神，\n还亲切地主动和我说话。我非常感谢您。",
        "実は……こういう風に誰かとゆっくりおしゃべりするのは久しぶりです。\nとても嬉しいです。": "其实……像这样和别人悠闲聊天，已经很久没有过了。\n我非常开心。",
        "グナイゼナウさんとお話ができて、私もとても嬉しいです。まだ出会ったばかりなのに、\nなんだかとても親近感がわくと言いますか、一緒にいて心地いいです。": "能和格奈泽瑙小姐聊天，我也很开心。明明才刚认识，\n却不知为何觉得非常亲切，和您待在一起很舒服。",
        "それは……実は私もです！私、普段はすこし人見知りなので、誰かと一緒にいるときは\nつい緊張してしまいます……でも島風ちゃんと一緒にいるときは全然そんなことありません。": "那……其实我也是！我平时有点怕生，和别人在一起时总会\n不自觉地紧张……但和岛风在一起时完全不会这样。",
        "えへへ、そう言っていただけると、私も嬉しいです。": "嘿嘿，您这么说我也很开心。",
        "さっそく仲良くなったか……\nまあ、意外でも何でもないけどね。": "这么快就变亲近了吗……\n嗯，也没什么好意外的。",
        "島風のお嬢ちゃんもグナイゼナウお嬢も大人しい子だ。\nそれは明るくて、ちょっとマイペースな子とも相性がいいかもしれないが……": "岛风小姑娘和格奈泽瑙小姐都是文静的孩子。\n她们或许也会和开朗、稍微我行我素的孩子合得来……",
        "あの二人にとって、やはり自分とすこし似たような同類と一緒にいる時のほうが\nリラックスできるでしょう。": "对她们两人来说，果然还是和与自己有些相似的同类在一起时\n更容易放松吧。",
        "……でもすき焼きはひどいよ！こっちは事態がややこしくなるのを避けるために、\nバッグの中で匂いを嗅ぐことしかできないのに！": "……但寿喜烧太过分了！为了避免事态变复杂，\n我只能在包里闻味道而已！",
        "くそ~ ！\n帰ったら絶対お嬢ちゃんにもう一度すき焼きを奢ってもらうからね～！": "可恶～！\n回去后我一定要让小姑娘再请我吃一次寿喜烧～！",
        "……そういえば、グナイゼナウさんは先ほど、\n「誰かとゆっくりおしゃべりするのは久しぶり」とおっしゃっていましたが……": "……说起来，格奈泽瑙小姐刚才说过，\n“已经很久没和谁悠闲聊天了”……",
        "もしかして、普段は話し相手がいないのですか？": "难道您平时没有聊天对象吗？",
        "連絡を取り合っている友達は数人いますが、年末年始のこの期間はみんな自分の身内と\n一緒に過ごしたいだろうと思って、つい連絡するのをためらってしまいます。": "有几个保持联系的朋友，但我想着年末年初这段时间大家都想和家人\n一起度过，所以总是不由得犹豫要不要联系。",
        "私も……姉ともっと一緒にいたいですから。一緒にお正月の準備をしたり、\n家を飾ったりしたいです。でも……姉は最近ずっと公務に追われています。": "我也……想和姐姐多待在一起。想一起准备新年，\n装饰家里。但是……姐姐最近一直忙于公务。",
        "もう一週間以上会っていません。\n大晦日の日には必ず帰ってくると姉は言っていましたが……": "已经一个多星期没见面了。\n姐姐说除夕一定会回来……",
        "私たちはやっぱり戦姫ですし、姉さんは上の方々に信頼を置かれていますから、\nいつも忙しいです。その日になって、やはり帰れないってこともよくあります。": "我们毕竟是战姬，姐姐又深受上层人士信任，\n总是很忙。到了那天，临时无法回来也很常见。",
        "そう思うと、お正月の準備に力を入れる気もなくなってしまいました……\nこんな簡単なこともちゃんとできないなんて、私って、本当に情けないですね。": "一想到这些，我连准备新年的干劲都没有了……\n连这么简单的事都做不好，我真是没用。",
        "そんなことはありませんよ。\nそれは全部グナイゼナウさんとお姉さんととても仲がよいからでしょう？": "没有那回事。\n正因为格奈泽瑙小姐和姐姐感情非常好，才会这样吧？",
        "お姉さんのことが大好きですから、一緒にいられない分、\n寂しさや悲しさを感じるのではないでしょうか。": "因为您很喜欢姐姐，无法一起度过的那部分时间，\n就会让您感到寂寞和难过吧。",
        "私には姉妹がいませんので、グナイゼナウさんのように、\nこんなに仲のいいお姉さんがいるのがとてもうらやましいです。": "我没有姐妹，所以非常羡慕格奈泽瑙小姐，\n能有这样关系亲密的姐姐。",
        "グナイゼナウさんのお姉さんも、グナイゼナウさんが自分との時間を\nこんなに大切にしているのを知ったら、きっとすごく嬉しいはずです！": "如果格奈泽瑙小姐的姐姐知道您如此珍惜和她相处的时间，\n一定也会非常开心！",
        "……そう……ですか？": "……是……这样吗？",
        "あれ、もしかして私、何か変なこと言いました……？": "咦，难道我说了什么奇怪的话……？",
        "そんなことありません！ただ……実はついこの前まで、私は姉さんのことがすこし苦手\nなんです。姉はとても頼もしくて、すこし厳しい人ですから。私なんかと全然違って……": "没有那回事！只是……其实就在不久前，我还有点不擅长和姐姐相处。\n因为姐姐非常可靠，也有点严厉，和我完全不同……",
        "姉との距離が近くなって、もっと仲良くなったのはつい最近のことです。それも、\n本当に仲良くなったのか、島風ちゃんにそう言われるまで、確信が持てませんでした。": "我和姐姐关系变近、变得更加亲密，也是最近的事。甚至直到岛风这么说之前，\n我都无法确定我们是否真的变亲近了。",
        "……そうですか、私と姉さんは、\n島風ちゃんから見て、仲が良いのですか。": "……这样啊，我和姐姐，\n在岛风看来关系很好吗？",
        "ええ、間違いありません。\nグナイゼナウさんのお姉さんに会った事はありませんが……": "是的，绝对没错。\n虽然我没见过格奈泽瑙小姐的姐姐……",
        "グナイゼナウさんがお姉さんに会えなくて悲しんでいるということは、\nそれだけ姉妹で過ごす時間が幸せだということではありませんか。": "格奈泽瑙小姐因为见不到姐姐而难过，\n不正说明和姐妹共度的时光很幸福吗？",
        "グナイゼナウさんのお姉さんも、\nきっとグナイゼナウさんのことを大切に思っているに違いありません。": "格奈泽瑙小姐的姐姐，\n一定也非常珍惜格奈泽瑙小姐。",
        "お姉さんが「大晦日には必ず帰ってくる」と約束した以上、必ず大好きな\nグナイゼナウさんとの約束を守って、グナイゼナウさんの傍に帰ると思います。": "既然姐姐约定“除夕一定会回来”，她就一定会遵守和最喜欢的\n格奈泽瑙小姐的约定，回到您的身边。",
        "……そうですね！\n姉さんはきっと約束を守って帰ってきます。": "……对！\n姐姐一定会遵守约定回来的。",
        "だったらちゃんと準備して、\nシャルン姉さんと一緒に最高の大晦日を過ごしたいですね！": "那我得好好准备，\n和沙恩姐姐一起度过最棒的除夕！",
        "ありがとう、島風ちゃん。\nあなたのおかげで元気が出ました。": "谢谢你，岛风。\n多亏了你，我打起精神了。",
        "いいえ、私は何も。\nでも、もし何かお手伝いすることがありましたら、いつでも呼んでくださいね。": "不，我什么都没做。\n不过，如果有需要帮忙的地方，随时叫我哦。",
        "そういえば、大晦日に年越しパーティーが開かれると瑞鶴さんが言っていました。\n内部のネットワークで生中継を観ることができるらしいですよ。": "说起来，瑞鹤小姐说除夕会举办跨年派对。\n听说可以通过内部网络观看直播。",
        "そうですか？またパールベイの方々が面白そうなことをするのですね。\nそれなら観ない手はありませんね。教えてくれてありがとう、島風ちゃん。": "是吗？珍珠湾的各位又要举办有趣的活动了。\n那当然不能错过。谢谢你告诉我，岛风。",
        "ところで、大晦日の晩御飯のメニューは決まりましたか？\nせっかくお姉さんと一緒に食べる晩御飯です、何にしますか？": "对了，除夕晚餐的菜单决定了吗？\n难得和姐姐一起吃晚饭，打算吃什么？",
        "迷っていたのですが、島風ちゃんが今日この店に連れてきてくれたおかげで、\nやっと決めました。": "我之前还在犹豫，多亏岛风今天带我来这家店，\n我终于决定了。",
        "鍋料理にしようと思います。このお店で出された料理のようにおいしく作れる自信は\nありませんが……今日食べたこのおいしい料理を、お姉さんにも食べさせたいです。": "我想做锅料理。虽然没有信心做得像这家店的料理一样美味，\n但我想让姐姐也尝尝今天吃到的美味料理。",
        "大晦日に鍋料理……姉妹でこたつを囲んで、温かい鍋料理をいただくのですか。\n素敵ですね！": "除夕吃锅料理……姐妹围着暖炉桌享用热腾腾的锅料理。\n真不错！",
        "（ファッションスタイルのせいでバレてしまったの——！？）": "（难道因为服装风格暴露了——！？）",
        "なんですって？\nこの二人が？": "你说什么？\n这两个人？",
        "…そう言えば、\n確かに新型の増援が来る予定でしたね。": "……说起来，\n确实原本计划会有新型增援到来。",
        "（…ええっ！？）": "（……诶！？）",
        "状況は理解した。ここは拙者が引き受けよう。\nパトロールに戻りなさい。": "情况我明白了。这里交给在下。\n回去巡逻吧。",
        "あっ…はい。\n了解っす。": "啊……是。\n了解。",
        "あなた……": "你……",
        "…ここでお会いできるとは思いもしませんでした。\nフッド様。": "……没想到会在这里见到您。\n胡德大人。",
        "私のことを知っているのですか？": "您认识我吗？",
        "シリアルコードSC02-0127-0013。\n拙者は昔ソロモン基地の駐屯部隊にいまして、フッド様の指揮下にありました。": "序列号SC02-0127-0013。\n在下以前驻扎于所罗门基地，曾在胡德大人麾下。",
        "そうなんですか？\nすみません、見分けられなくて…": "是这样吗？\n抱歉，我没认出您……",
        "取るに足らない者ですので、フッド様が気に病む必要はありません。\nそもそもフッド様が離任するまで、こうしてお話しする機会もありませんでした。": "在下不过是个无足轻重之人，胡德大人不必介怀。\n况且在胡德大人离任前，我们也没有这样交谈的机会。",
        "…しかし、あなたは今ここにいることは、\nつまり——": "……但是，您现在在这里，也就是说——",
        "…はい。フッド様が離任されたのち、\nソロモン基地に駐屯した部隊はそれぞれ異なる部隊に編入されました。": "……是的。胡德大人离任后，\n驻扎在所罗门基地的部队分别被编入了不同部队。",
        "フッド様のようにMCCには抗えず、\nお恥ずかしい限りでございます。": "没能像胡德大人一样抵抗MCC，\n实在惭愧。",
        "…なら、先ほどどうして私たちを庇ったのですか？": "……那么，刚才为什么要庇护我们？",
        "こんな拙者でも、今度は勇気を出して、正しいことをしたかったのです。\n…これは、許されないことでしょうか？フッド様。": "即使是在下，也想这次鼓起勇气做正确的事。\n……这样做是不被允许的吗？胡德大人。",
        "…こんなことがバレたら、\nあなたの身にも危険が及びます。": "……这种事一旦暴露，\n您自身也会有危险。",
        "それでもなお、私たちに協力したいというのですか？": "即便如此，您还是想帮助我们吗？",
        "——信じてくださいっと、言わせてください。\nフッド様はラフィー様のために来たのでしょう。拙者が案内します。": "——请相信我，让我这么说吧。\n胡德大人是为了拉菲大人而来的吧。在下为您带路。",
        "急げ、さっさと戦線の穴を塞げ！": "快点，赶紧堵住战线缺口！",
        "くそ、腰抜けどもめ、姿を見せたらどうだ！？": "可恶，你们这些胆小鬼，有本事现身啊！？",
        "よいっしょ——": "嘿咻——",
        "よいっしょ——！": "嘿咻——！",
        "ふ…ふ…疲れたの…": "呼……呼……累了……",
        "あのワルモノ…一体どうやってこのコウソクから脱出したの？\nラフィーは芋虫になったみたいで…しくしく。": "那个大坏蛋……到底是怎么从这个拘束中逃出去的？\n拉菲好像变成毛毛虫了……呜呜。",
        "イモムシ、嫌いなの…\nブルースフィアのイヤな生き物なの…": "拉菲讨厌毛毛虫……\n那是蓝星的讨厌生物……",
        "…だめ！ラフィーが弱気になっちゃだめなの。\n出口はもうすぐなの。": "……不行！拉菲不能泄气。\n出口马上就到了。",
        "よいっしょ！\nよいっしょ！…": "嘿咻！\n嘿咻！……",
        "こちらです、フッド様。": "这边请，胡德大人。",
        "…なんだか不安だね…\nイータさん、本当に彼女を信じていいの？": "……总觉得不安……\n伊塔小姐，我们真的可以相信她吗？",
        "あっ、別にイータさんの昔の仲間を疑っているわけでは！\nでもイータさんだって、その子のことがよく覚えていないって言ったから…": "啊，我不是在怀疑伊塔小姐以前的伙伴！\n只是伊塔小姐也说过，记不清那孩子了……",
        "今は…彼女のことを信じるしかなさそうですね。\nただし、警戒だけは怠らないで。": "现在……看来只能相信她了。\n不过，警戒绝不能放松。",
        "…急がないと。\nMCCの注意を惹きつけているオークランドたちが、どれだけ持つかわからないので…": "……得快点。\n不知道吸引MCC注意的奥克兰她们还能撑多久……",
        "着きました、フッド様。\nこちらです。": "到了，胡德大人。\n就是这里。",
        "ラフィー様はここに拘束されて——": "拉菲大人被拘束在这里——",
        "なんですか！？": "什么！？",
        "ううぅぅ…！": "呜呜……！",
        "ラフィー、踏まれちゃったの…": "拉菲被踩到了……",
        "ラフィーちゃん！？": "拉菲！？",
        "！\n泣き虫の姉ちゃん！！": "！\n爱哭的姐姐！！",
        "正しい時に、 向かうべき場所で、 やるべきことをする。 それだけだ。 ": "在正确的时间，于该去的地方，做该做的事。仅此而已。 ",
        "『なんだか後ろめたい気持ちになります。 』": "『总觉得有些内疚。』",
        "ああ、この鏡餅、かわいいですね~ !": "啊，这个镜饼真可爱～！",
        "鏡餅……もちってことは食べものなのか？": "镜饼……既然是年糕，就是食物吗？",
        "はい、食べられますよ。": "是的，可以吃。",
        "丸いお餅二個をひと重ねにして、その上に家が代々続くようにと\n縁起を担う橙（だいだい）を乗せたものが鏡餅と言います。": "把两个圆年糕叠在一起，再放上象征家族世代延续的\n吉祥橙子，这就是镜饼。",
        "昔から鏡には神様が宿るとされています。貴重なお米で作ったお餅を\n神様にお供えすると、神様は新しい年の幸せを運んできてくれると言われています。": "自古以来，人们认为镜子中寄宿着神明。把用珍贵米制作的年糕\n供奉给神明，据说神明就会带来新一年的幸福。",
        "神様、ね……": "神明啊……",
        "それで、最後は食べるの？": "所以最后要吃掉吗？",
        "おおおお！いいじゃないか！食欲がそそる！": "哦哦哦哦！不错嘛！让人食欲大开！",
        "うーん、でもそれは伝統的な鏡餅のことで……ここにあるのはなんだか違うようです。\nこれは餅と橙ではなく、プラスチックで作られた鏡餅のような形の置物のようです。": "嗯，不过那是传统镜饼……这里的好像不太一样。\n这不是年糕和橙子，而是用塑料制成的镜饼形状摆件。",
        "ええええ——！なによ、がっかりした。\nせっかく神様の食べ物を盗み食いする機会があると思っていたのに……": "诶诶诶诶——！什么嘛，真失望。\n我还以为终于有机会偷吃神明的食物了……",
        "シグマさん、そんな発言はすこし失礼ですよ。": "西格玛小姐，这种说法有点失礼。",
        "鏡餅を食べるのは、神様から盗み食いするのではなく、神様にお供えした後で、\n感謝の気持ちを込めて、ありがたくいただくことで……あれっ？": "吃镜饼不是从神明那里偷吃，而是在供奉给神明后，\n怀着感谢之情恭敬地享用……咦？",
        "よく見ると、これには「まる餅入り」と書いてありますね……ああ！\nこれは開けられるのです。中には小さな丸いお餅がたくさん入っていますよ、シグマさん！": "仔细一看，上面写着“内含圆年糕”……啊！\n这个可以打开。里面有很多小圆年糕哦，西格玛小姐！",
        "よーし！じゃあ次はそっちに行ってみよう……？\n待てよ、そこにいるのは……": "好！那接下来去那边看看……？\n等等，那里的是……",
        "……はあ。": "……唉。",
        "何かに困っているような方ですね。\nシグマさんの知り合いですか？": "她好像遇到了什么烦恼。\n是西格玛小姐的熟人吗？",
        "うん、そうよ。\nどこから説明したらいいのか……": "嗯，是的。\n该从哪里说起呢……",
        "前も言ったでしょう。私は昔本能的にこっちに引きつけられて来たことがあって、 \n黒歴史をいっぱい作ってしまったって。": "我之前说过吧。我以前曾凭本能被吸引到这里，\\n结果留下了许多黑历史。",
        "あの子が私がバカしていた間、\nいろいろと世話を焼いてくれた子よ。": "我胡闹的时候，\n是那孩子一直照顾我。",
        "そういえば、島風のお嬢ちゃんにちょっと似ている子だね。\nどっちかと言うと内気なほうで、何でも心に溜めてしまう。": "说起来，她和岛风小姑娘有点像。\n比较内向，什么事都憋在心里。",
        "あっ、今はそんなことを気にする場合ではありませんね。": "啊，现在不是在意这种事的时候。",
        "つまり、あそこの女性は確かに何かを悩んでいる、ということですね。\n私なんかで、何か力になることはあるでしょうか？": "也就是说，那位女性确实在为什么事烦恼。\n我能帮上什么忙吗？",
        "優しいお嬢ちゃんは、\nきっとあの子に寄り添うことができるよ！": "善良的小姑娘，\n一定能陪伴她、理解她！",
        "あなたが相手なら、グナイゼナウお嬢も悩みを打ち明けやすいはずだ。\nなんなら、私もついているじゃないか！": "如果对方是你，格奈泽瑙小姐应该更容易说出烦恼。\n再说，不是还有我陪着吗！",
        "というわけで、島風お嬢ちゃんにお願いできるかい？あの子に声をかけて、\n何があったのか聞いてほしい。あのときお世話になった恩に報いたいのだ。": "所以，可以拜托岛风小姑娘吗？去和那孩子搭话，\n问问她发生了什么。我想报答她当时照顾我的恩情。",
        "やれやれ。 いつのまにか、 今年もこの時期を迎えたのか。 ": "真是的。不知不觉又到了今年这个时候。 ",
        "どこに行ってもすごい盛り上がりようで、 派手な装飾品があちこちに飾られていて、 \n見ているだけで目が痛くなる。 ": "不管走到哪里都热闹非凡，到处挂着华丽的装饰，\\n光是看着眼睛都疼。 ",
        "悪徳商人がこの機に乗じて儲けるために作り出した罠だって、 \n子供も大人もほいほい飛び込んでしまう……": "这是奸商趁机牟利设下的陷阱，\\n可大人小孩都一个个心甘情愿地跳进去……",
        "やれやれだぜ。 \nどうしてみんなそんなにクリスマスに夢中なのかな？": "真是的。\\n大家为什么都这么沉迷圣诞节呢？",
        "ねえ、 ジョン？": "对吧，约翰？",
        "クリスマスはとても大事な行事だよ。 そんなクリスマスを馬鹿にするような言い方は、 \nクリスマスにも、 クリスマスを楽しみにしている皆さんにも失礼だ。 ": "圣诞节是非常重要的节日。用这种方式嘲笑圣诞节，\\n无论对圣诞节还是期待圣诞节的大家都很失礼。 ",
        "それに、 ホーエル。 \nあなたは口ではそう言っているが、 どう見ても乗り乗りではないか……": "而且，霍埃尔。\\n你嘴上这么说，但怎么看都很兴奋吧……",
        "どう見ても、 あなたはもう完全にクリスマスバージョンのホーエルになっているでしょう！ ": "怎么看你都已经完全变成圣诞版霍埃尔了吧！ ",
        "日向さんも、 どうして伊勢さんを止めてくれないのよ！ ": "日向小姐，你为什么也不阻止伊势小姐！ ",
        "いいんですか、 それで？ \nあなたのお姉ちゃんはどんどん図にのってしまうのよ？ ": "这样真的好吗？\n你的姐姐会越来越得意忘形的哦？ ",
        "……それは、 お姉ちゃんへの悪口ですか？ ": "……这是在说姐姐的坏话吗？ ",
        "あっ……いいえ、 違います。 \nすみません、 間違いました。 ": "啊……不，不是。\\n抱歉，我说错了。 ",
        "す、 すごい殺気……やはり、 \n「決して日向さんの前で伊勢さんの悪口を言ってはいけない」という情報は本当だったのね！ ": "好、好强的杀气……果然，\\n“绝不能在日向小姐面前说伊势小姐坏话”这条情报是真的！ ",
        "でも、 どうして伊勢さんまで……\n「サンタクロースになる」とはどういうことですか？ ": "可是，为什么连伊势小姐也……\\n“要成为圣诞老人”是什么意思？ ",
        "なんだ、 ジョンストンちゃんは知らないの？ ": "什么，约翰斯顿还不知道吗？ ",
        "もうすぐクリスマスでしょ。 \nクリスマスといえば、 プレゼントとサンタクロース！ ": "马上就是圣诞节了吧。\\n说到圣诞节，就是礼物和圣诞老人！ ",
        "はい。 \n愛宕先輩なら、 そんな感じではないと思います。 ": "是的。\\n如果是爱宕前辈，我觉得不会是那种感觉。 ",
        "長ねぎは斜め切り、しめじは石づきを切って、小房に分けました。\n牛肉は高級のものを購入しましたし、豆腐やこんにゃくなどの食材も揃いました……": "大葱切成斜片，蟹味菇切去根部并分成小朵。牛肉买的是高级货，豆腐和魔芋等食材也都准备好了……",
        "うん、準備はばっちりですね！": "嗯，准备得非常充分！",
        "さすがにタレを上手に作れる自信はありませんから、\n市販のものを用意しましたが……": "我实在没信心把酱汁做得好，\n所以准备了市售酱汁……",
        "島風ちゃんオススメのタレですから、味はきっと問題ありません！使い方を間違わないように、\n実は既に一回作って、自分でこっそりいただいたことはシャルン姉さんに内緒ですね。": "这是岛风推荐的酱汁，味道肯定没问题！为了不弄错用法，\n其实我已经做过一次偷偷尝过了，这件事要瞒着沙恩姐姐。",
        "……姉さん、遅いですね。\nまだ帰ってこないんですか？": "……姐姐，回来得真晚。\n还没回来吗？",
        "というわけで、 私たちの中の誰かがサンタクロースになって、 \n夢と希望が詰まったプレゼントを子どもたちに届ける予定なんだ。 ": "所以，我们中的某个人要成为圣诞老人，\\n把装满梦想与希望的礼物送给孩子们。 ",
        "……えっ？ ": "……诶？ ",
        "でもジョージ5世さんのことだ。 \nたとえこういうことでも、 彼女は真剣なんだ。 ": "不过这是乔治五世小姐的作风。\\n即使是这种事，她也会认真对待。 ",
        "直接誰かを指名するのは、 他の人にとって不公平だって。 \nだから今日の面接会なんだよ。 ここで、 みんながサンタさんになるチャンスを競うのだ。 ": "直接指定某个人对其他人不公平。\\n所以才有今天的面试会，大家要在这里竞争成为圣诞老人的机会。 ",
        "……このエンディングは平和だな。 ラスボスがいきなり出てきて\n光線攻撃で街を真っ二つ！ みたいな展開はないの？ ": "……这个结局真和平。不会出现最终BOSS突然现身，\\n用光线攻击把城市劈成两半之类的展开吗？ ",
        "せっかく事が収まったのだ、 \nそんなことを期待して何になるの。 ": "好不容易事情平息了，\\n期待那种事又有什么意义？ ",
        "（にしても、 まさかこんな形でまたあいつに会うことになるとは……\nいいえ、 これは会えたとは言えない。 ただ 「姿を見えた」 だけか）": "（话说回来，没想到会以这种形式再次见到那家伙……\n不，这不能算见面。只是“看到了身影”而已吗）",
        "……こんな第三の選択肢とは言えない、 \n 「選択」 から独立している 「結末」 は、 本当に存在するのか？ ": "……这种不能称为第三选项、\n独立于“选择”之外的“结局”，真的存在吗？ ",
        "……ゼーっちは何について聞いているんですか？ ": "……泽泽，你到底在问什么？ ",
        "（というか秘書よりもメイドに見えるけど！ ってツッコミたいけど、 あまり首つっ込むと\n 「掃除」 されちゃうかもって勘がそう告げている！ 瑞鶴さんは空気を読めるからね！ ）": "（话说看起来更像女仆而不是秘书！虽然很想吐槽，但直觉告诉我，管太多可能会被“打扫”掉！瑞鹤可是很会察言观色的！）",
        "……島風ちゃんからですか。": "……是岛风发来的吗？",
        "写真もついていますね……わあ、本当に賑やかですね！\nふふ、パールベイの方々は相変わらず元気ですね。": "还附了照片……哇，真的很热闹！\n呵呵，珍珠湾的各位一如既往地有活力。",
        "いいですね。\n大晦日をみんなと一緒に過ごすのはきっと楽しいでしょう。": "真不错。\n和大家一起度过除夕一定很开心。",
        "姉さんはまだ帰っていません……\n島風ちゃんにどう返事したらいいでしょう……": "姐姐还没回来……\n我该怎么回复岛风呢……",
        "あれ、島風ちゃんからまた新しいメールが？": "咦，岛风又发来新邮件了？",
        "「もしかしてグナイゼナウさんのお姉さんは、やはり仕事で帰りそうにないのですか？\nでしたらグナイゼナウさんもこちらに来て、私たちと一緒に過ごしませんか？」": "“难道格奈泽瑙小姐的姐姐果然因为工作回不来吗？\n那格奈泽瑙小姐也来这里，和我们一起度过吧？”",
        "「席ならまだいくらでもあります。\nグナイゼナウさんが来てくれましたら、きっと皆さんも喜びます！」": "“还有很多座位。\n格奈泽瑙小姐愿意来的话，大家一定都会很高兴！”",
        "うわ～最悪だ～！ \n伊勢さん初っ端から子供のクリスマスへの夢をぶち壊してしまったよ！ ": "呜哇～糟透了～！\n伊势小姐一开始就把孩子们对圣诞节的梦想毁了！ ",
        "こんなんじゃ夢と希望を運ぶサンタクロースになれる資格なんてない。 \n伊勢さんアウト、 ホーエル大~勝~利~！ ": "这样根本没资格成为传递梦想与希望的圣诞老人。\n伊势小姐出局，霍埃尔大~胜~利~！ ",
        "待って待って！ 今のなしだ！ ": "等等等等！刚才那句不算！ ",
        "どちらにしても、 これで事情がはっきりしましたね。 \nお疲れ様でした、 カールスルーエさん。 ": "不管怎样，事情已经清楚了。\\n辛苦了，卡尔斯鲁厄小姐。 ",
        "あ、島風ちゃんの返信が来ました。はやいですね。": "啊，岛风回复了。真快。",
        "「そうですね。\n年に一度だけの大晦日は、やっぱり姉妹で過ごすのが一番ですね。」": "“是啊。\n一年只有一次的除夕，果然还是和姐妹一起度过最好。”",
        "「こちらこそ、お出かけのお誘いをいただけるなんて嬉しいです。\nお正月といえば初詣ですね、楽しみです！」": "“我才要谢谢您，能收到出门邀请我很开心。\n说到新年就是初诣呢，我很期待！”",
        "ふふ。私も楽しみです。": "呵呵。我也很期待。",
        "……よし！シャルン姉さんが帰ってきたら、\nすぐにあつあつの鍋が食べられるように、鍋を火にかけましょう！": "……好！等沙恩姐姐回来，\n就能马上吃到热腾腾的火锅，把锅先烧起来吧！",
        "すまない、グナイ！\n遅くなった——": "抱歉，格奈！\n我回来晚了——",
        "シャルン姉さん！お帰りなさい。": "沙恩姐姐！欢迎回来。",
        "あ……ああ。ただいま。\nすまない、ちょっとしたトラブルがあって、遅くなったんだ。": "啊……嗯。我回来了。\n抱歉，遇到一点小麻烦，所以回来晚了。",
        "大丈夫です、姉さん、気にしないでください。\nそれより、早くあがりましょう？": "没关系，姐姐，别在意。\n比起这个，快进来吧？",
        "とんでもありません。 ": "哪里的话。 ",
        "えっ！ ？ まさか、 あたしなにかシュぺーを悲しませるような話、 しっちゃった？ \nごめんね！ わざとじゃないんだ。 あたし、 ちょっと鈍感なところがあるから……": "诶！？难道我说了什么让修佩难过的话？\n对不起！我不是故意的。我有点迟钝……",
        "気にしないでください。 私個人からすれば、  「祝日」 というのは楽しむためのものです。 \nみんなは自分なりのやり方で祝日を楽しめばいいです。 ": "请别在意。在我看来，“节日”就是用来享受的。\n大家按自己的方式享受节日就好。 ",
        "ああ、分かった。\nいい匂いがするんだが……何だろう？": "啊，知道了。\n闻起来好香……是什么？",
        "鍋ですよ！正確には「すき焼き」と言います。\n先日知り合った友達と一緒に食べたのです。": "是火锅！准确地说叫“寿喜烧”。\n前几天我和刚认识的朋友一起吃过。",
        "本当においしかったので、せっかくの大晦日に、\nこの美味しい料理を姉さんと一緒に食べられたらいいなと思いまして。": "因为真的很好吃，所以我想在难得的除夕，\n和姐姐一起吃这道美味的料理。",
        "新しい友達ができたのか。それはよかった。\nではこの「すき焼き」をいただきながら、その新しいお友達の話を聞かせてくれないか？": "交到新朋友了吗？那太好了。\n那就一边吃这份“寿喜烧”，一边给我讲讲那位新朋友吧？",
        "ええ、もちろんです!": "好的，当然！",
        "あれ、シャルン姉さん、それは……？": "咦，沙恩姐姐，那是……？",
        "ケーキだ。お前は昔この味のケーキを食べた時、とても美味しそうに食べていたから、\nこういうのが好きかと思って、一個予約しておいた。": "是蛋糕。你以前吃这种口味的蛋糕时，看起来非常开心，\n所以我想你喜欢，就预订了一块。",
        "でも受け取りに向かったら、店の方の手違いでかなり時間がかかってしまって……\nそれで遅くなったんだ。ほんとにすまない。": "但去取的时候，因为店家的疏忽花了很长时间……\n所以回来晚了。真的很抱歉。",
        "私にケーキを買うために……": "为了给我买蛋糕……",
        "でも確かに言われてみれば、 クリスマスから新年まで結構間が空いていますね。 \n摩耶さん、 もしよければ、 うちのテーマパークに遊びに来ませんか？ ": "不过仔细想想，圣诞节到新年确实间隔很久。\n摩耶小姐，如果愿意，要不要来我们的主题乐园玩？ ",
        "本当！ ？ シュぺーもいっしょでしょ？ \nじゃあたし行く！ ": "真的！？修佩也一起去吧？\n那我去！ ",
        "（投票なんてどうでもいいですよ。 学校のイベントなんて、 適当にやればいいのに。 \n青春の思い出を残すとか、 したい人は自分なりにするでしょう）": "（投票什么的无所谓。学校活动随便办办就好。\n想留下青春回忆的人，自然会按自己的方式去做）",
        "そうだ、 瑞鶴さんも——": "对了，瑞鹤小姐也——",
        "お？ こっちは平和的じゃないか？ いいぞいいぞ。 \nあの政治家気取りのやつらみたいに、 投票依頼とか、 迷惑なことするんじゃないよ。 ": "哦？这边不是挺和平的吗？很好很好。\n别像那些自以为是政治家的人一样，做拉票之类的麻烦事。 ",
        "さあ、 学園が提供したアフタヌーンティーだ。 \n遠慮なくどうぞ。 ": "来，这是学院提供的下午茶。\n请不要客气。 ",
        "やった！ 美しくてかっこいいポートランド姉さん、 ありがとう！ \n今度ご飯を盛り付ける時も多めにくれるとうれしいな！ ": "太好了！美丽又帅气的波特兰姐姐，谢谢！\n下次盛饭时也多给我一点就好了！ ",
        "調子に乗るなよ。 ": "别得意忘形。 ",
        "はっ！ ？ \n助っ人呼ぶのかよ、 ずるいぞ！ ": "哈！？\n居然叫帮手，太狡猾了！ ",
        "ならあたしも仲間を呼ぶ！ \nゆけ、 イモウト——！ ": "那我也叫伙伴！\n去吧，妹妹——！ ",
        "ヒュウガガ！ ": "日向嘎！ ",
        "日向さん！ \nすぐ伊勢さんのおふざけに付き合わないでくださいよ！ ": "日向小姐！\n别这么快就陪伊势小姐胡闹啊！ ",
        "しかも自分がポ○モンである設定をそんなにすぐ受け入れないでくださいよ！ \n鳴き声まで考えたってどういうことですか！ ": "而且别这么快接受自己是宝○梦的设定啊！\n连叫声都想好了是怎么回事！ ",
        "え！ ？ うそ！ ？ 本当にここで選択肢出すの！ ？ \n本当に行くかどうか決めるの！ ？ うそでしょう！ ？ ": "诶！？骗人！？真的要在这里出现选项！？\n真的要决定去不去！？不会吧！？ ",
        "瑞鶴さま！ 頼む！ \n是非彼女を舞台に上がらせてくれ！ ": "瑞鹤大人！拜托了！\n请务必让她登上舞台！ ",
        "あなたはただ面白がっているだけでしょう。 ": "你只是觉得有趣而已吧。 ",
        "ちょっと待ってくれ。 \nはっきりしておかなくてはならないことがある。 ": "等一下。\n有件事必须先说清楚。 ",
        "頭のおかしい女の子が歌ったり、 踊ったりすれば、 \n大勢の人が見に来るのか？ どういう心理なんだ？ ": "脑子不正常的女孩唱歌跳舞，\n就会有很多人来看吗？这是什么心理？ ",
        "あれは間違っている認識です！ \n早く忘れて選択肢を選んでください！ ": "那是错误的认知！\n快忘掉它，选择选项！ ",
        "サンフランシスコの誘いを断る": "拒绝旧金山的邀请",
        "サンフランシスコの誘いを受ける": "接受旧金山的邀请",
        "謎のゲストΗ": "神秘嘉宾Η",
        "ゴホン。えっと……このたびパールベイの年越しパーティーにお招きいただき、\n大変光栄です。主催者の方々にとても感謝しています。": "咳。呃……这次能受邀参加珍珠湾的跨年派对，\n我深感荣幸。非常感谢主办方各位。",
        "……私の故郷では、このように皆で集まり、新年を盛大に祝うことができません。\n今回の経験はとても新鮮で、とても……忘れがたいものです。": "……在我的故乡，无法像这样大家聚在一起盛大庆祝新年。\n这次经历非常新鲜，也非常……令人难忘。",
        "謎のゲストΛ": "神秘嘉宾Λ",
        "おいしいものがたくさんある。ラムダ、うれしい！たくさん食べたよ！\n歌と漫才も面白くて、にぎやかで楽しかった！": "有好多好吃的。拉姆达很开心！吃了好多！\n歌曲和漫才也很有趣，热闹又开心！",
        "ラムダはこの楽しい気持ちを、\nここにいない姉ちゃんや、おともだちに分ける！": "拉姆达要把这份开心的心情，\n分享给不在这里的姐姐和朋友们！",
        "ふふ、かわいい願いですこと。\n叶うといいですね。": "呵呵，真可爱的愿望。\n希望能够实现。",
        "……まったく……ブルースフィアのやつらはのん気だな。": "……真是的……蓝星的家伙们太悠闲了。",
        "こうして見ると、まともなやつがほとんどいないじゃないか。ブルースフィアは\nみんなそうなのか？それともパールベイのやつらが緊張感なさ過ぎなのか？": "这么一看，正经人几乎一个都没有。蓝星的人\n都这样吗？还是珍珠湾的家伙们太缺乏紧张感？",
        "そもそもあなたたち、 ここは室内ですよ！ ？ \n何を考えているの、 はやくやめなさい……って、 聞いています！ ？ ": "话说你们，这里可是室内啊！？\n到底在想什么，快停下……喂，听见了吗！？ ",
        "ドカーン——！ ！ ！ ": "轰——！！！ ",
        "ツェッペリン？何を見ている。": "齐柏林？你在看什么？",
        "あっちの生中継を見て、もしかしたらこっちにも似たようなことをしている者が\nいるかもしれないと思って、ちょっと探してみたんだが。": "我看了那边的直播，想着这边或许也有人在做类似的事，\n所以稍微找了一下。",
        "なっ……まさかうちにも似たような馬鹿な真似をしているやつがいるというのか？\n一体どこのどいつだ！？": "什么……难道我们这里也有人在做类似的蠢事？\n到底是哪来的哪个家伙！？",
        "あそこです！行け！ゼーっち！\nファイトです～！": "就是那里！上吧，泽泽！\n加油～！",
        "ふん！すこし泳がせたら、すぐ調子に乗るとは！\n己の弱さを知れ！": "哼！才放任你一会儿，就马上得意忘形！\n认清自己的弱小吧！",
        "必殺！ 瞬！鶴！殺——！！！": "必杀！瞬！鹤！杀——！！！",
        "イエーイ！勝ちました！！\nさすがゼーっちですね~！": "耶——！赢了！！\n不愧是泽泽～！",
        "ご覧の通り、「ゼーっちと一緒に！大晦日熱血バトル」特別企画がはじまってから、\nゼーっちは既に50戦無敗の偉業を達成しました！": "如大家所见，自“和泽泽一起！除夕热血战斗”特别企划开始后，\n泽泽已经达成50战不败的伟业！",
        "これはとんでもない！ゼーっち超カッコいい！と思った視聴者の方は、\n何をすればいいのか、あたしに言われなくてもお分かりですよね？": "太不得了！觉得泽泽超帅的观众，\n不用我说也知道该做什么吧？",
        "おい、変なことをするな。\nそういうことをするためにここに来たんじゃない——": "喂，别做奇怪的事。\n我不是为了做那种事才来这里的——",
        "分かってます分かっています。優しいゼーっちは、外で仕事している皆さんに大晦日を\nひとりで寂しく過ごさせたくないから、あたしに付き合っているのでしょう？": "知道了知道了。温柔的泽泽是不想让在外工作的人独自寂寞地度过除夕，\n所以才陪我的吧？",
        "みんな……！こんなにも優しくてツンツンのゼーっちを見て、頭の中に浮かぶ言葉は！？\nチャットで書いてくださいね！何と言っても、やっぱりあれでしょう——": "大家……！看到这么温柔又傲娇的泽泽，脑中浮现的词是什么！？\n请写在聊天里！不用说，果然就是那个——",
        "ゼーっち、かわいい～～～！！": "泽泽，好可爱～～～！！",
        "いきなりなにを言い出す……なっ！？チャットがいきなりいっぱい増えた！？\nかわいいなんて……お前らまでふざけるなよ！！": "突然说什么呢……什么！？聊天消息突然暴增！？\n可爱什么的……你们也别跟着胡闹！！",
        "……ここにも緊張感のないバカがいる！\nどうしたんだよ、瑞鶴さん！しっかりしてくださいよ！？": "……这里也有个毫无紧张感的笨蛋！\n怎么了，瑞鹤小姐！振作一点啊！？",
        "おっ！瑞鶴さまの生中継番組じゃないか。\nオークランドさまとツェッペリンさまも観ているのか！": "哦！这不是瑞鹤大人的直播节目吗。\n奥克兰大人和齐柏林大人也在看吗！",
        "ツェッペリンさま、「そば」をお持ちしました。\n調理法に問題はないと思いますし、このバカに盗み食いされたこともありません。": "齐柏林大人，我把“荞麦面”带来了。\n料理方法应该没问题，也没有被这个笨蛋偷吃过。",
        "はあ？そばって……あのねツェッペリン、\nまさかあなたまでのんびり新年を迎えるつもりなんて言わないよね？": "哈？荞麦面……我说齐柏林，\n你该不会也打算悠闲地迎接新年吧？",
        "なぜそう驚く？適度に休むことにより、仕事の効率が上がるのだ。\nそれに、異文化を身をもって体験できるいい機会だ。": "为什么这么惊讶？适度休息能提高工作效率。\n而且这是亲身体验异文化的好机会。",
        "ブルースフィアでは「年越しそば」を食べる風習がある。実に面白い。\n新しい年になる瞬間に、具体的には噛む、飲み込むのどっちの動きを実行したほうがいいと思う？": "蓝星有吃“跨年荞麦面”的习俗，真有意思。\n在新年到来的瞬间，你觉得具体应该做咀嚼还是吞咽的动作？",
        "さすがに、ちっとも進展なくてちょっと行き詰まってるんだ。\nあっ、そういえば…": "说实话，一点进展都没有，稍微陷入僵局了。\n啊，说起来……",
        "オークランドさん、ゼリーありがとうございました！\n独特な味と口当たりだったけど、あれって何味なんですか？": "奥克兰小姐，谢谢您的果冻！\n味道和口感都很独特，那到底是什么口味？",
        "そんな細かいことを気にしなくてもいいだろう！\nというか、僕はてっきりあなただけはそっち側になることはないと思ってたのに……！": "不用在意这种细节吧！\n话说我还以为只有你不会站到那边去……！",
        "そんなこと言わないでくださいよ。せっかくの新年だ、楽しく過ごして何が悪い？\nオークランドさまも一緒にそばを見ながら瑞鶴さまを食べようよ！": "别这么说嘛。难得的新年，开心度过有什么不好？\n奥克兰大人也一起边看荞麦面边吃瑞鹤大人吧！",
        "逆だ！瑞鶴さまを食べるな！！": "反了！别吃瑞鹤大人！！",
        "ですが……僭越ながら、私もそう思います。今日は向こうも攻めてきたりしないでしょう。\n万が一何か動きがあった場合、すぐ前線のものが教えてくれますし。": "不过……恕我冒昧，我也这么认为。今天对方应该不会进攻。\n万一有任何动静，前线的人会立刻通知我们。",
        "思いつめていることがあるかもしれませんが、今日は休んで、\n新年を楽しんでみませんか、オークランドさま。": "您可能有什么烦心事，但今天不如休息一下，\n享受新年吧，奥克兰大人。",
        "はい！これ、オークランドさまの分のそばだよ！": "是！这是奥克兰大人的荞麦面！",
        "うわ、あっつ……！？\nでも、いい匂いだ……": "哇，好烫……！？\n不过，闻起来真香……",
        "来年が良い年になりますように、いただきます——！": "愿明年是美好的一年，我开动了——！",
        "ズルズル……うん、悪くない味だ。": "嗦嗦……嗯，味道还不错。",
        "……ほら。\nオースジェルじゃん。ラベル付いてるし。": "……你看。\n这不是奥斯杰尔吗，上面还有标签。",
        "ライザ、どうしてオースジェルを食べ物と勘違いしたのよ！？": "莱莎，你为什么会把奥斯杰尔误认为食物！？",
        "……あきれた。": "……真是无语。",
        "あっ、何かが光ってるみたい…": "啊，好像有什么东西在发光……",
        "やばいよ！！すでに意識が混乱しているじゃないか！！\nやっぱオースジェルなんて食べちゃだめなんだ！": "糟了！！意识已经混乱了不是吗！！\n果然奥斯杰尔不能吃啊！",
        "違う、本当に光ってる！！": "不，真的在发光！！",
        "作り途中だった採取地調合器が……！？\nもしかして……成功した、ってこと？": "还在制作中的采集地调合器……！？\n难道……成功了？",
        "ブルースフィアでは、オークランドさんは「アンノウンΟ」と呼ばれている": "在蓝星，奥克兰小姐被称为“Unknown Ο”",
        "…なんだって？": "……你说什么？",
        "ブルースフィアでは、フッドさんは「アンノウンΗ」と呼ばれている": "在蓝星，胡德小姐被称为“Unknown Η”",
        "『巧みに名目を立てて、 良い所を全部独り占めする。 \nさすがあの自己中心的な外道がやることですね。 ふん。 』": "『巧立名目，把好处全部独占。\n不愧是那个以自我为中心的恶徒，哼。』",
        "……どうせ無理矢理奪うには、 ここであんたたちと一戦交えなければならないだろう。 \nめんどい。 やめた。 ": "……反正要强行夺走的话，就得在这里和你们打一场。\n麻烦，算了。 ",
        "（それにこんなやり方では、 ばれない方がおかしいでしょう）": "（而且用这种方法还不暴露才奇怪吧）",
        "……何とか秘密裏にこちらの者に説明しておきます。 博士がこちらの研究に\n手を貸したことも一回や二回じゃありませんし、 この件で反目することはないでしょう。 ": "……我会想办法私下向这边的人解释。博士帮助我们研究也不是一次两次了，\n不会因为这件事反目吧。 ",
        "うん。 ちょっと頼りになった姉ちゃん、 おつかれ。 ": "嗯，稍微可靠的姐姐，辛苦了。 ",
        "ありがとう。 ラムダちゃんはあのヒトに言われたことをきちんとこなせばいいのよ。 \n難しいことは、 私たち大人に任せて。 ": "谢谢。拉姆达只要好好完成那个人交代的事就行。\n复杂的事交给我们大人。 ",
        "そ、そんなことはありません！\n私、とても嬉しいです！本当に……とても嬉しいです！": "没、没有那回事！\n我非常开心！真的……非常开心！",
        "そうか。それならいいんだが……": "这样啊。那就好……",
        "……はは。たかがケーキでそんなに喜ぶのか？\nおおげさだな。": "……哈哈。只是蛋糕而已，有这么开心吗？\n太夸张了。",
        "う……ご、ごめんなさい。\nつい……": "呜……对、对不起。\n一不小心……",
        "謝ることはない。\nお前が喜んでくれるから、ケーキを買ってきた者として、私も嬉しい。": "不用道歉。\n看到你开心，作为买蛋糕的人我也很高兴。",
        "……だいぶ待たせたから、不安になったり、気分が沈んだりしていないか心配していたが……\n大丈夫のようだな。": "……让你等了这么久，我还担心你会不安或情绪低落……\n看来没事了。",
        "お前は本当に成長したね。\nまあ、姉としては少し複雑な気分だが……": "你真的长大了。\n不过，作为姐姐心情还是有点复杂……",
        "シャルン姉さん？何か言いましたか？": "沙恩姐姐？您刚才说什么？",
        "はい。 \nシュぺーお嬢様のご家族の依頼で参りました。 ": "是的。\n我是受修佩小姐家人的委托来的。 ",
        "あ、 学園で何かあるんじゃないか心配しているのね？ \nごめんね、 うち最近確かにちょっとごたごたしている。 ": "啊，你是在担心学院是不是出了什么事？\n抱歉，我们最近确实有点混乱。 ",
        "このお茶は謝罪というかなんというか、 どうぞ遠慮なく。 ": "这杯茶算是赔礼之类的，请不要客气。 ",
        "……ありがとうございます。 ": "……谢谢。 ",
        "よし、 ではあたしはこれで失礼するよ。 \n他の人にお茶を配らなきゃ。 ": "好，那我先告辞了。\n还得给其他人分茶。 ",
        "学園も太っ腹だね。 \n": "学院还真大方。\n",
        "全員分があるなんて、 配るのも大変だけど。 ": "居然有全员份，分发起来也很辛苦。 ",
        "はぁ、 資金不足だから、 こういうあまり金のかからないもので\nみんなの機嫌をとるしかないってことか。 ": "唉，因为资金不足，只能用这种不太花钱的东西\n来安抚大家吗。 ",
        "でも、 赤城先輩たちみたいな人は、 \nどう考えても、 お茶の一杯で何とかできるわけないでしょう。 ": "不过，像赤城前辈她们那样的人，\n无论怎么看都不可能靠一杯茶解决吧。 ",
        "残念ながら、 それが最も難しい点ですね。 ": "遗憾的是，这正是最棘手的地方。 ",
        "まあまあ、 何とかなるでしょう。 \nあたしたちは、 ケンカしなければいいってことで。 ": "好啦好啦，总会有办法的。\n我们只要不吵架就行。 ",
        "じゃあこの瑞鶴から、 あたしたちの友情のために、 乾杯～": "那就由我瑞鹤开始，为我们的友情干杯～",
        "ふふ、 はい。 \n友情のために。 ": "呵呵，好。\n为了友情。 ",
        "カンパイ～！ ": "干杯～！ ",
        "お二人さん、 少々お待ちください。 ": "两位，请稍等。 ",
        "えっ？ あの……メイドのお姉さん？ \nどうしたんですか？ ": "诶？那个……女仆姐姐？\n怎么了？ ",
        "いいえ、 お待ちになっていただくのはこちらの 「お二人さん」 だけです。 \n瑞鶴さまはお気になさらず。 ": "不，需要等待的只有这“两位”。\n瑞鹤大人无需在意。 ",
        "これは必要な犠牲です。 ": "这是必要的牺牲。 ",
        "カールスルーエさん！ ！ ": "卡尔斯鲁厄小姐！！ ",
        "パタン——": "啪嗒——",
        "いやいやいやいや！ いきなりどうした！ みんながあたしの毒見を待っているかのようなこの展開！ \nこんな状況で平気で飲む人いないでしょ！ ": "不不不不！突然怎么了！这展开搞得像大家都在等我试毒一样！\n这种情况下谁能安心喝啊！ ",
        "待っ！ ？ \nうぐぅ……ごくぅ……": "等！！？\n呜咕……咕咚……",
        "くはぁっ！ ど、 毒が入っているかどうかはわからなかったけど、 \n危うくむせて死んじゃうところだったよ……": "咳哈！虽、虽然不知道有没有毒，\n但差点就呛死了……",
        "……確かにやりすぎました。 お嬢様のご友人には失礼なことをしました。 \nお詫び申し上げます。 ": "……确实做得过火了。对小姐的朋友失礼，我向您道歉。 ",
        "まあ大丈夫ですよ、 気にしていませんから。 \nむしろクールな美人お姉さんに無理矢理飲まされるのも成長の一歩？ なんて、 冗談……": "没关系，我不介意。\n被冷酷漂亮的姐姐强行灌茶，说不定也是成长的一步？开玩笑的……",
        "あれ、 なんかおかしい——": "咦，总觉得不对——",
        "ジョン、 これには理由があるのだ。 \nあたしはね、 誠意を見せているのよ。 ": "约翰，这其中是有理由的。\n我是在表达诚意。 ",
        "お姉ちゃんの言うとおりです。 ": "姐姐说得对。 ",
        "でもまあ、 言うまでもないが、 サンタクロースなんて実在しない。 \nだから私たちがやらないと、 子供たちの靴下にプレゼントを入れる人なんてどこにもいないんだ。 ": "不过不用说，圣诞老人并不存在。\n所以如果我们不做，就没有人往孩子们的袜子里放礼物了。 ",
        "はい！お肉はもちろんですが、私は豆腐としめじも大好きです！\n味が染み込んでいて、とっても美味しいです！": "是的！肉当然好吃，但我也最喜欢豆腐和蟹味菇！\n吸满汤汁，非常美味！",
        "ほんのり甘い、やさしい味わいですね。\n食べているうちに、身も心もぽかぽかになりますね。": "微甜而温和的味道。\n吃着吃着，身体和心都暖起来了。",
        "生卵をつけて食べると聞いた時はびっくりしましたが、\nお肉と生卵の組み合わせはこんなにおいしいとは思いませんでした！": "听说要蘸生鸡蛋吃时我很惊讶，\n没想到肉和生鸡蛋搭配起来竟然这么美味！",
        "……えっ……でも、 でも……": "……诶……可是，可是……",
        "うん？ そんなに驚く必要があるのか？ ジョンストンちゃんはこのことを知らなかったようだけど……\nさてはうちのBBSをあまり見ないでしょう？ ": "嗯？有必要这么惊讶吗？约翰斯顿似乎不知道这件事……\n你是不是不怎么看我们的BBS？ ",
        "でも残念、 もう締め切りだから、 今から参戦しようとしても間に合わないよ~？ ": "不过很遗憾，报名已经截止了，现在想参赛也来不及啦～？ ",
        "いいえ、 そういう問題じゃなくて……そんなことよりも。 ": "不，不是这个问题……比起这个。",
        "——サンタクロースって、 実在しないのですか？ ": "——圣诞老人，不是真实存在的吗？ ",
        "……えっ。 ": "……诶。 ",
        "では左から順に、 自己紹介と、 サンタクロースになりたい理由、 それからもし合格したら、 \nどのようにしてサンタクロースとしての役目を果たすのか、 聞かせてもらいましょうか。 ": "那么就从左边开始，请依次介绍自己、说明想成为圣诞老人的理由，\\n以及如果合格，打算如何履行圣诞老人的职责。 ",
        "……くっ！ ": "……可恶！ ",
        "ジョージ5世さんが握っていた万年筆がまるでポテトチップスのように粉々になってしまった！ ？ ": "乔治五世小姐手里的钢笔竟像薯片一样碎成了粉末！？ ",
        "すまない、 怖がらせてしまいましたね。 \nせっかくグローウォームちゃんが自分の休憩時間を使って、 手伝いに来ているのに……": "抱歉，吓到你了。\n明明萤火虫还特意利用休息时间来帮忙……",
        "ある程度予想はしていたのに、 いざ本当にこいつの口からあんなふざけたことを聞いたら、 \nやっぱり我慢できなくて……！ ": "虽然多少预料到了，但真的从这家伙嘴里听到那么胡闹的话，\n果然还是忍不住……！ ",
        "……す、 すごいね。 ミニライブとはいえ、 \n目の前できれいな女の子がピカピカの服を着て、 歌ったり踊ったり……": "……好、好厉害。虽说是迷你演唱会，\n但漂亮女孩穿着闪闪发光的衣服在眼前唱歌跳舞……",
        "では、 もしあなたが今年のサンタクロースに選ばれたとしたら、 \n具体的に何をするつもりですか？ ": "那么，如果你被选为今年的圣诞老人，\n具体打算做些什么？ ",
        "それはもちろん、 基地のみんなの好みや需要を元に、 \nみんなに最高のクリスマスプレゼントを選ぶに決まっているじゃないですか！ ": "当然是根据基地大家的喜好与需求，\n为大家挑选最棒的圣诞礼物！ ",
        "ただ、 みんなに素敵な夢と希望を届けるために、 \n多少の費用がかかるかもしれませんが——": "不过，为了给大家带来美好的梦想与希望，\n可能需要花费一些费用——",
        "結局金が目当てなのか！ ！ ！ ": "结果还是冲着钱来的啊！！！ ",
        "きゃー！ 机に大きな穴が！ ！ ！ ": "呀——！桌子上出现大洞了！！！ ",
        "でもなんというか、 ちょっと意外だな。 まさか大鳳みたいな大人しい優等生も、 \nこういう時に思いっきり学校さぼるとは。 安心安心、 内緒にするから。 ": "不过怎么说呢，有点意外。没想到像大凤这样文静的优等生，\n也会在这种时候逃课。放心，我会保密的。 ",
        "私の前で堂々と公金横領しようとするなんて、 \nこのジョージ5世がこの基地の管理を任されている限り、 絶対にさせるものか！ 次！ ！ ！ ": "居然敢在我面前公然挪用公款，\n只要我乔治五世还负责管理这个基地，就绝不会允许！下一个！！！ ",
        "やっと僕の番だ！ \n僕はグリッドレイ——": "终于轮到我了！\n我是格里德利——",
        "今度は壁に穴が開いた！ ？ \n落ち着いてくださいよ、 ジョージ5世さん！ ": "这次墙上也出现洞了！？\n乔治五世小姐，请冷静！ ",
        "なんで不合格なのよ！ 理不尽だ！ ！ \nまだ自己紹介すら終わってないよ！ ！ ": "为什么不合格！太不讲理了！！！\n我连自我介绍都还没结束呢！！！ ",
        "あなたがグリッドレイだから、 不合格です！ ": "因为你是格里德利，所以不合格！ ",
        "まあ……グリッドレイの日頃の行いを考えると……ね。 ": "嗯……考虑到格里德利平时的所作所为……也是情有可原。 ",
        "久しぶりだな、 諸君。 ": "好久不见，各位。 ",
        "（うわ、 これほど説明しづらいなのか\n説明したくないのか分からない後輩が初めてだ）": "（哇，这是我第一次遇到不知道是难以说明，还是不想说明的后辈）",
        "うん！\nこのすき焼きという鍋料理は、確かに美味だね。": "嗯！\n这道叫寿喜烧的锅料理确实美味。",
        "へへ、姉さんはきっと気に入ると思ってました。": "嘿嘿，我就知道姐姐一定会喜欢。",
        "そうだ、島風ちゃんにメールしなきゃ。\n「姉さんが帰ってきました。姉さんもすき焼きが大好きみたいです」っと。": "对了，得给岛风发邮件。\n“姐姐回来了。姐姐好像也很喜欢寿喜烧。”",
        "そういえば、パールベイで年越しパーティーが開かれているそうです。\n生中継もしているって島風ちゃんが言いました。一緒に見ましょう、姉さん。": "说起来，听说珍珠湾正在举办跨年派对。\n岛风说还有现场直播。姐姐，我们一起看吧。",
        "パールベイで年越しパーティーを？あの者たちらしいね。\nいいだろう、何をしているのか、私も気になる。": "珍珠湾的跨年派对？很有他们的风格。\n好吧，我也想看看他们在做什么。",
        "分かりました！\nえっと……ありました！これですね。": "知道了！\n呃……找到了！就是这个。",
        "しかし、 わたしの天使よ——ああ、 あなたに会えただけで、 こうも心が満たされるとは。 \nやはり、 かわいい駆逐艦は心を癒す良薬だ。 ": "不过，我的天使啊——啊，只是见到你，心灵就如此满足。\n果然，可爱的驱逐舰是治愈心灵的良药。 ",
        "えええ——！ ？ \n変態さん……じゃなくて、 アドミラル・ヒッパーさん！ どうしてここに？ ": "诶诶诶——！？\n变态……不对，希佩尔海军上将！你为什么在这里？ ",
        "もちろんです！お任せください。\nシグマさんに頼まれなくても、困っている人がいたら助けるのは当たり前です。": "当然！交给我吧。\n即使西格玛小姐不拜托我，遇到有困难的人也应该出手相助。",
        "何よりもうすぐ新年です。\nできることなら、世の中のみんなに楽しく新しい年を迎えてほしいですね。": "更何况新年马上就要到了。\n如果可以的话，希望世上的每个人都能开心迎接新年。",
        "候補者の五人は全員揃ったようですね。 ": "五位候选人似乎都到齐了。 ",
        "はいはい~まずあたし、 大人の色気と少女のかわいらしさを兼ね備えた、 \nみんな大好きな伊勢ちゃんだよ！ ": "好啦好啦～首先是兼具成熟女性魅力与少女可爱的，\n大家最喜欢的伊势酱！ ",
        "あたしがサンタクロースになりたい理由は、 それが面白そうだからです！ \n具体的にどうすればいいのか、 まだ決まっていない！ えっへん☆": "我想成为圣诞老人的理由，是因为感觉很有趣！\n具体要怎么做还没决定！哼哼☆",
        "次はあたしだけど、 もう始めていい？ ": "接下来轮到我，可以开始了吗？ ",
        "違う！ \nあと話を逸らすな！ ！ ": "不对！\n还有，别岔开话题！！！ ",
        "あんたの話を最後まで聞いた私が馬鹿だった。 むしろあんたの話を信じて、 \n本当にわざわざ人目を避けてやってきた私が大馬鹿野郎みたいだ。 ": "我居然听你把话说完，真是个笨蛋。倒不如说，相信你的话，\n特意避开人群来到这里的我才像个大笨蛋。 ",
        "……もう帰る！ ": "……我回去了！ ",
        "ちょっと待っててよゼーっち！ \nそんなに急ぐこともないでしょう？ ": "等等我啊，泽泽！\n没必要这么急吧？ ",
        "せっかくのハロウィン、 年に一度しかありませんよ？ ゼーっちもあたしに借りがある\nって言ってたでしょう？ 騙されたと思って、 参加してみてくださいよ！ ": "难得的万圣节，一年只有一次哦？泽泽不是也说过欠我人情吗？\n就当被骗了，来参加一下吧！ ",
        "断る！ そのどう聞いてもあんたみたいな年中ふざけているやつらが\n騒ぐ口実みたいなものに興味ない！ ": "拒绝！我对这种无论怎么听都像是你们这些整天胡闹的家伙\n找借口喧闹的活动没兴趣！ ",
        "ゴホン。 あたしはホーエルと申します。 \nまあみんな知ってると思うけど。 ": "咳。我叫霍埃尔。\n不过大家应该都知道。 ",
        "あたしがサンタクロースになりたい理由は、 \nみんなに夢と希望を届けたいからです。 ": "我想成为圣诞老人的理由，\n是想把梦想与希望带给大家。 ",
        "そのために……ほら！ あたしは既にサンタクロースの装備を一通り揃えてきた！ \nどうですか、 あたしのやる気が伝わりましたか？ ": "为此……看！我已经把圣诞老人的装备全都准备好了！\n怎么样，感受到我的干劲了吗？ ",
        "ホーエル……意外とまともに振舞っていますね。 ": "霍埃尔……没想到表现得还挺正常。 ",
        "どうせゲームに課金するか、 あるいはもっと悪いことをするつもりでしょ！ ": "反正你肯定是打算给游戏氪金，或者做更坏的事吧！ ",
        "しゃべるネコ？": "会说话的猫？",
        "って…………………………": "竟然…………………………",
        "にゃん—— ！ ！ ！ ": "喵——！！！ ",
        "おかしいですね。 あたしが幻覚でも見たのでしょうか。 \nなんだかしゃべる奇妙な生き物がそこを通ったような……": "真奇怪。难道我产生幻觉了吗？\n总觉得有个会说话的奇怪生物从那里经过……",
        "……と言いたいところだが、 \n指揮官たちがここにいないのは、 少々寂しさを感じるがね。 ": "……本来想这么说，\n但指挥官们不在这里，多少还是有点寂寞。 ",
        "……誰も居ないようですね。 \n今なら大丈夫だと思います。 ": "……好像没有人在。\n现在应该没问题。 ",
        "異様に静かだな。 おかしい。 ": "安静得异常。很奇怪。 ",
        "あんたらの基地は、 年中騒がしいところだと聞いてた。 \nなのに今のこの異様な静かさ……どうやら、 ただごとじゃないな。 ": "听说你们的基地一年到头都很吵。\n但现在安静得异常……看来事情不简单。 ",
        "はい、 その通りです。 もし他に方法があるなら、 \n私もゼータさんに迷惑をかけたりしません。 ": "是的，正如您所说。如果还有其他办法，\n我也不会麻烦泽塔小姐。 ",
        "しかし……パールベイは今、 大きな危機に瀕しています。 \nどうしても、 あなたの力が必要です。 ": "但是……珍珠湾现在正面临巨大危机。\n无论如何，都需要您的力量。 ",
        "あんたがこんな真面目なことを言うなんて……": "没想到你会说这么认真的话……",
        "メールが……！\nまさか姉さんから——": "邮件……！\n难道是姐姐发来的——",
        "「グナイゼナウさん、こんばんは。お姉さんはお帰りになりましたか？\nすでにあたたかい鍋料理を召し上がっているでしょうか。」": "“格奈泽瑙小姐，晚上好。您的姐姐回来了吗？\n现在应该已经在享用热腾腾的锅料理了吧。”",
        "「パールベイの年越しパーティーは既に始まりました。美味しい料理がたくさんあって、\n歌やパフォーマンスを披露する方もいらっしゃいます。とてもにぎやかです。」": "“珍珠湾的跨年派对已经开始了。有很多美味的料理，\n还有人在展示歌声和才艺。这里非常热闹。”",
        "もちろん、 わたしの可愛い天使たちに会いに来たのさ。 ": "当然是来见我可爱的天使们。 ",
        "ひっ。 \nジョージ5世さん、 は、 はやく警察を……": "咿。\n乔治五世小姐，快、快报警……",
        "冗談だ。 \n今回は人に大事な仕事を頼まれて来たんだ。 ": "开玩笑的。\n这次是受人所托来做重要工作的。 ",
        "しかし俗事は地上の凡人たちがやるべきことだ。 \n天使たちは気にせず、 ただクリスマスを満喫すればいい。 ": "不过俗事应该由地上的凡人来做。\n天使们不用在意，只要尽情享受圣诞节就好。 ",
        "かといって、 ちょうどサンタクロースを募集している知らせが目に入ったのさ。 \nせっかくだから、 凡人であるこのわたしもつい乗っかってしまった。 ": "不过，我正好看到招募圣诞老人的通知。\n难得有这个机会，身为凡人的我也顺便参加了。 ",
        "どうやら本当に大変なことになってしまったようだな。 \n安心しろ、 私が手伝うと承諾した以上、 約束を反故にすることはしない。 ": "看来事情真的变得很严重了。\n放心，既然我答应帮忙，就不会违背约定。 ",
        "……これで、 いままでのいろいろな借りを返せる。 ": "……这样就能还清至今欠下的各种人情了。 ",
        "ありがとう、 ゼータさん。 やはりあなたは、 いざというときに頼りになる者ですね。 \nあなたという友達がいて、 本当によかったです。 ": "谢谢你，泽塔小姐。关键时刻果然还是你靠得住。\n能有你这样的朋友，真是太好了。 ",
        "……うっとうしい、 そんな変なことを言ってる暇はないだろう。 ": "……真烦，没时间说这种奇怪的话吧。 ",
        "いったい何が起きているのか、 私は何をすればいいのか。 \nそれを早く教えてくれればいい。 ": "到底发生了什么，我该做什么。\n快点告诉我就行。 ",
        "わかりました。 事の始まりは昨日の深夜です。 \n何の予兆もなく—— 異界からの邪悪な気配がパールベイを襲いました！ ": "明白了。事情始于昨天深夜。\n毫无预兆地——来自异界的邪恶气息袭击了珍珠湾！",
        "その時、 空が真っ二つに割れた！ 布団から起き上がったみんなが目を凝らすと、 \n広場に三つの異形の姿があったのです。 ": "那时，天空裂成两半！大家从被窝里爬起来定睛一看，\n广场上出现了三个异形身影。 ",
        "それはなんと、 あの世からやってきた魔物でした。 どうやら今日は、 \n旧文明では夏が本格的に終わり、 厳しい冬が始まる日とされていました。 ": "那竟然是来自冥界的魔物。原来在旧文明中，今天被认为是\n夏天正式结束、严冬开始的日子。 ",
        "……ふふ、島風ちゃんは本当に優しい子ですね。": "……呵呵，岛风真是个温柔的孩子。",
        "でも、「大丈夫、姉さんはちょっとだけ遅くなるだけですから。心配しないでください。\n島風ちゃんはちゃんと年越しパーティーを楽しんでくださいね。」……": "但是，“没关系，姐姐只是会晚一点回来。请不要担心。\n岛风要好好享受跨年派对哦。”……",
        "「でも誘っていただいて、とても嬉しいです。\nお正月にまた一緒にどこかに出かけましょう。」っと、送信。": "“不过能收到您的邀请，我非常开心。\n新年我们再一起去什么地方吧。”发送。",
        "この日に死後の世界に繋がる扉が開き、 悪しき物がやって来るのです。 \nそれらは生きている者を襲い、 人間界で好き勝手にしようとします。 ": "这一天，通往死后世界的大门会打开，邪恶之物随之而来。\n它们袭击活人，企图在人间为所欲为。 ",
        "パールベイに侵入した三匹の魔物は非常に強い。 \nいずれも霊界では名の知れた者です。 ": "侵入珍珠湾的三只魔物非常强大。\n它们都是灵界赫赫有名的存在。 ",
        "残忍な吸血鬼四姉妹の中でも最も狡知に富み、 最も厄介な末妹。 \n魔王もまたいで通る、 通称デモまたの—— ブラディーレディ！ ": "残忍吸血鬼四姐妹中最狡猾、最难对付的小妹。\n连魔王都要绕道而行，外号“魔王也绕行”的——血腥女士！ ",
        "純真な顔に鬼のような実力！ 誰であろうと彼女の前でうずくまる、 \n暴虐の地獄の門番—— ケルベロススウィートハニー！ ": "拥有纯真面孔与恶鬼般实力！无论是谁都只能在她面前跪地，\n暴虐地狱的守门人——地狱三头犬甜心！ ",
        "最後は気まぐれで掴みどころのない、 人の苦しみに喜びを感じ、 \n弱い者のあがきを食いちぎる残酷な獣—— プラチナのオオカミ姫！ ": "最后是个任性难捉摸、以人类痛苦为乐，\n撕碎弱者挣扎的残酷野兽——白金狼姬！ ",
        "彼女たちはこのパールベイに呪いをかけました。 今日中に彼女たちを倒すことが\nできなければ、 この基地にいる全員の生気が吸い取られます。 ": "她们给珍珠湾下了诅咒。如果今天之内无法击败她们，\n基地里所有人的生命力都会被吸走。 ",
        "──つまり、 みんな死んでしまいます。 ": "——也就是说，大家都会死。 ",
        "勝つには、 彼女たちが提示した試練をクリアしなければなりません。 \n敗者は永遠に魂を失うが—— ": "想要获胜，就必须通过她们提出的试炼。\n失败者将永远失去灵魂——",
        "試練を全部クリアすれば、 今夜開催されるパールベイハロウィンスペシャル\nパーティーに参加できます！ どうですかゼーっち、 ぜひ挑戦してみませんか？ ": "如果全部通过试炼，就能参加今晚举行的珍珠湾万圣节特别\n派对！怎么样，泽泽，要不要务必挑战一下？ ",
        "——なるほど、 それでうちの食堂が廃墟になったのですね。 ": "——原来如此，所以我们的食堂才变成废墟。 ",
        "えっへん☆": "哼哼☆",
        "まあ~": "哎呀～",
        "あっはっは……": "啊哈哈……",
        "わたしには関係ないよ？ ": "和我没关系哦？ ",
        "すぅぅぅっ—— ": "吸—— ",
        "ふざけるな！  ！  ！ ": "别胡闹！！！ ",
        "危ないよゼーっち！ \nいきなりそれを振りまわ—— ": "很危险啊，泽泽！\n突然挥舞那个——",
        "そういえば、 前から気になっていたのですが、 ゼーっちがいつも持っているそれは一体何でしょう。 \n武器には見えませんが……孫の手？ ": "说起来，我之前就很好奇，泽泽一直拿着的那个到底是什么。\n看起来不像武器……是挠痒痒用的长柄？ ",
        "誰が受け入れるか！ ？ \nそんなのたとえゲームの仕組みが許したとしてもあたしの撫子心が許さないよ！ ": "谁会接受啊！？\n就算游戏机制允许，我的少女之心也绝不允许！ ",
        "お誘いいただきありがとうございます！ \nでもごめんなさいお断りします！ ！ ": "感谢您的邀请！\n但很抱歉，我拒绝！ ",
        "えええ、 全部あたしたちのせいにするの？ ": "诶诶诶，要把一切都怪到我们头上吗？",
        "ジョー5世さんとグローウォームちゃんがその前に\n壁や柱に食らわせたパンチは関係ないって言い切れるの？ ": "你能断言乔治五世小姐和萤火虫之前\n打在墙壁和柱子上的拳头没有关系吗？",
        "それもあなたたちがあまりにもふざけたことをするから——": "那也是因为你们做了太多胡闹的事——",
        "——そこまで。 ": "——到此为止。 ",
        "ヒッ。 ": "咿。 ",
        "今は言い争っている場合ではありません。 \nあなたたち、 今の状況をちゃんと理解しているのですか？ ": "现在不是争吵的时候。\n你们到底有没有好好理解现在的情况？ ",
        "今晩がクリスマス・イヴですよ。 基地にはもちろん早々パーティーの準備を整えました。 \nわたしを含め多くの人が、 そのためにたくさんの時間と努力を費やしました。 ": "今晚可是平安夜。基地当然早早就准备好了派对。\n包括我在内，许多人都为此付出了大量时间和努力。 ",
        "「借りがある」から何をしてもいいと思うな—— ": "别以为有“人情债”就可以为所欲为——",
        "まったく、 お嬢ちゃんは一体どこに行ったのか。 ハロウィンだからといって、 \nふわふわしたクッションに私を置いていくなんてひどいじゃないか！ ": "真是的，小姑娘到底跑哪儿去了。就算是万圣节，\n也不能把我丢在软绵绵的靠垫上啊！ ",
        "そりゃあ、 あのクッションが本当に超ふわふわだから、 \nついダメになってしまった私にもちょっとだけ責任があるかもしれないけど—— ": "那是因为那个靠垫真的超级柔软，\n害得我不小心变得懒散，我或许也有一点责任——",
        "ハウお嬢様、 ここにいるのはほとんど初心者です。 \nこんな、 いきなり第四の壁を破る行為は軽率すぎます。 ": "豪小姐，在场的大多都是新手。\n这种突然打破第四面墙的行为太轻率了。 ",
        "そうなの？ \nてっきり今時は、 みんなこういうのに慣れていると思いましたが。 ": "是吗？\n我还以为现在大家都已经习惯这种事了。 ",
        "そちらではそうかもしれませんが、 ほら、 瑞鶴さまの様子を見てくださいよ。 \nしばらく立ち直れそうにないじゃありませんか……": "你们那边或许是这样，但你看，看看瑞鹤大人的样子。\n她好像暂时都缓不过来了……",
        "……はぁ、 ここは私がしばらく代わりましょう。 ": "……唉，这里暂时由我来代替吧。 ",
        "それは……待って！ ": "那是……等等！ ",
        "ゼーっち！ ？ ": "泽泽！？ ",
        "……うん~": "……嗯～",
        "ゴホン！ えっと……というわけですね。 ": "咳咳！呃……事情就是这样。 ",
        "起きてしまったことは仕方ありません。 \n今は責任を追及するのではなく、 夜のクリスマスパーティーを成功させることが急務です。 ": "既然事情已经发生，就没有办法了。\n现在当务之急不是追究责任，而是让今晚的圣诞派对成功。 ",
        "故に！ ここはパールベイの責任者である私、 キング・ジョージ5世が指揮をとり、 \nこの目標を達成するためにみんなに指示を出します！ ": "因此！作为珍珠湾负责人，我乔治五世将在这里指挥，\n为实现这个目标向大家下达指示！ ",
        "いいですか、 先ほどはついあの女のオーラに押されてしまいましたが、 \n何と言ってもここの責任者はこの私です。 そこは忘れないでください！ ": "听好了，刚才我一时被那个女人的气场压住了，\n但无论如何这里的负责人是我。请不要忘记这一点！ ",
        "はいはい、 わかっているよ。 \nジョージ5世様～": "好啦好啦，知道了。\n乔治五世大人～",
        "なんだか面倒なことになってきたな……はあ。 \nそれで、 あたしたちは何をすればいいの？ ": "事情好像变麻烦了……唉。\n所以，我们要做什么？ ",
        "元の計画を実行するまでもなく、 ゼーっちに残ってもらえたのはいいとして、 \nさっきのあれは……？ ": "先不说不用执行原计划、泽泽能留下来是好事，\n刚才那个到底是……？ ",
        "誰かが作った、 ものすごく\nハイクオリティーのイタズラアイテム……には見えませんね。 ": "看起来不像是某人制作的、\n质量超高的恶作剧道具……",
        "ふふ、 どうやら今年のハロウィンは\nとっても面白くなりそうですね~": "呵呵，看来今年的万圣节\n会变得非常有趣呢～",
        "うん～まあいいでしょ。 ": "嗯～算了，也没关系。 ",
        "じゃ頼んだよ、 我が妹！ ": "那就拜托你了，我的妹妹！ ",
        "……えっ？ あれ？ どういうこと？ \nあたしたち、 なにしてたっけ？ ": "……诶？咦？怎么回事？\n我们刚才在做什么来着？ ",
        "ハウお嬢さま！ いくら何でもこれはやりすぎです！ \n超展開はともかく、 最後のこれはだめでしょう！ ": "豪小姐！无论如何这也太过分了！\n先不说剧情突然展开，最后这个实在不行吧！ ",
        "塩っち、 どうしてあたしに怒るの？ \nこういうやり方を始めたのは別にあたしじゃないのに。 ": "盐盐，你为什么对我发火？\n这种做法又不是我先开始的。 ",
        "人を殺すのは武器じゃなくて人自身です！ \nそんな簡単な道理もわからないはずがないでしょう！ ": "杀人的不是武器，而是人自己！\n这么简单的道理，你不可能不明白吧！ ",
        "あの、 このURLリンクをクリックしても何も反応がないんだが、 \n何か不都合でもあったのか？ ": "那个，点击这个 URL 链接没有任何反应，\n是出了什么问题吗？ ",
        "瑞鶴さまもしっかりしてください！ \n喧嘩以外のことだと、 すぐ間抜けにならないでください！ ": "瑞鹤大人也请振作一点！\n一遇到吵架以外的事情，就别马上变得糊涂啊！ ",
        "確かに伊勢よりは納得するけど……\nでもどうしていきなり日向になったんだよ！ ": "确实比伊势更让人信服……\n但为什么突然变成日向了啊！ ",
        "えっと、 ジョージ5世指揮官。 私は今回のサンタクロース選抜に応募していません。 \n何かの手違いではないでしょうか？ ": "呃，乔治五世指挥官。我没有报名参加这次圣诞老人选拔。\n是不是哪里弄错了？ ",
        "あぁ～それはですね、 実は応募した時、 \nついでに日向の名前も入れたんだよね～えっへん☆": "啊～是这样的，其实报名时，\n我顺便也填了日向的名字～哼哼☆",
        "えっ？ お姉ちゃんが……？ ": "诶？姐姐她……？ ",
        "ごめんね、 黙ってて。 \n自分だけが応募するのも面白くないな～って思ったから。 ": "对不起，一直瞒着你。\n我觉得只有自己报名也没意思～",
        "でもまさか私が不合格で日向が受かったなんて……": "但没想到我落选，日向却通过了……",
        "う~ん、 他のやつだったら、 そいつをこっそりやっつけてサンタの座を奪う！ \nと思っていたけど、 日向なら仕方ないね。 ": "嗯～如果是其他人的话，我本来打算偷偷打倒那家伙，夺走圣诞老人的位置！\n但既然是日向，那就没办法了。 ",
        "お姉ちゃんの分も頑張ってきてね、 サンタさん！ ": "也要替姐姐那份努力哦，圣诞老人！ ",
        "姉妹で……力を合わせて……": "姐妹之间……齐心协力……",
        "ジョージ5世指揮官がそこまで言うのなら、 分かりました。 \n任せてください。 ": "既然乔治五世指挥官都这么说了，我明白了。\n交给我吧。 ",
        "あたしも適当に頑張りますから、 最高のクリスマスにしよう、 我が妹よ！ ": "我也会适当努力的，让我们一起度过最棒的圣诞节吧，我的妹妹！ ",
        "はい。 あまり公にしないでくださいね。 \n実はこういう任務、 私たちはみんな適当に誤魔化しているのですよ。 ": "好的。请不要公开说出去。\n其实这种任务，我们都是随便糊弄过去的。 ",
        "……まさか、 他の方に何か言われたのですか？ ": "……难道有人对您说了什么吗？ ",
        "…信じてくれてありがとう。": "……谢谢你相信我。",
        "ふん。 \n愚かさと紙一重の賢明さだな。 ": "哼。\n这是和愚蠢仅一线之隔的聪明。 ",
        "つまり……他の候補者を全部やっつけてしまえば、 \nサンタさんの座はあたしのものになる！ ": "也就是说……只要把其他候选人全部打倒，\n圣诞老人的位置就是我的了！ ",
        "よかったね。ライザさん！": "太好了，莱莎小姐！",
        "はい……頑張って腕を振るいますね！": "好的……我会努力大显身手的！",
        "あの……本当に、 申し訳ありません。 ": "那个……真的非常抱歉。 ",
        "で、 でも、 まさか崩れるなんて……": "但、但是，没想到竟然会倒塌……",
        "全部こいつらが室内で好き勝手に発砲したせいなのよ！ \nええ、 そうに決まっています！ ": "全都是因为这些家伙在室内随意开枪！\n没错，一定是这样！ ",
        "あの、 喧嘩しないでください！ ": "那个，请不要吵架！ ",
        "全部私が不器用なせいで……": "全都是因为我笨手笨脚……",
        "きゃっ！ ？ ": "呀！？ ",
        "？ \nあの方向から何かが走ってきたような……": "？\n好像有什么东西从那个方向跑过来了……",
        "何者だ！ ": "什么人！ ",
        "こんなことさえ起きなければ、 \n誰もが満足できる完璧なパーティーになると自信を持って言えます。 ": "只要不发生这种事，我有信心说这会成为一场人人满意的完美派对。 ",
        "しかし、 用意されたパーティー会場も、 飾り物も、 食べ物や飲み物も全部……\nパーティー会場として使われる予定のこの食堂に置かれています。 ": "然而，准备好的派对场地、装饰品、食物和饮料……\n全都放在这间原本计划作为派对会场的食堂里。 ",
        "——つまりそれらすべて、 \nみなさんのふざけた喧嘩が原因で廃墟に埋もれてしまいました。 ": "——也就是说，这一切\n都因为大家胡闹般的争吵而被埋在废墟里了。 ",
        "心優しい星の魔女？": "心地善良的星之魔女？",
        "ああ！ なんということですか。 ": "啊！怎么会这样。 ",
        "我々のパールベイは、 悪に侵食されつつあります。 三匹の強大な魔物が\nそれぞれ三つの場所に巣くい、 基地全体に邪悪な呪いをかけました！ ": "我们的珍珠湾正在被邪恶侵蚀。三只强大的魔物\n分别盘踞在三个地方，给整个基地施加了邪恶诅咒！ ",
        "ですが幸い、 パールベイの戦姫たちは決して悪に屈しません！ \n三匹の魔物が出した試練に挑み、 パールベイを取り戻そうではありませんか！ ": "但幸运的是，珍珠湾的战姬绝不会向邪恶屈服！\n让我们挑战三只魔物设置的试炼，夺回珍珠湾吧！ ",
        "皆さんのお友達、 心優しい星の魔女、 \nこのノースカロライナがここで皆さんを導きましょう！ ": "大家的朋友、心地善良的星之魔女，\n北卡罗来纳将在这里引导大家！ ",
        "愛と正義を胸に、 邪悪を打ち砕くのです！ ": "怀抱爱与正义，粉碎邪恶吧！ ",
        "—— というわけで、 皆さんの手元にあるパンフレットに書かれた通り、 三つの試練は\nそれぞれ違う場所にありますので、 迷子になったら地図を確認してみてくださいね。 ": "——就是这样，正如大家手中的宣传册所写，三项试炼\n分别位于不同地点，迷路时请查看地图。 ",
        "何回挑戦しても構いませんが、 前の試練をクリアしてから次の試練に進んでください。 \nなにより重要なのは、 ぜひ夜11時のパーティーが始まる前に帰ってきてくださいね。 ": "可以挑战任意多次，但请通过前一项试炼后再前往下一项。\n最重要的是，请务必在晚上11点派对开始前回来。 ",
        "それでは、 正義の味方たちよ、 ファイトです！ 邪悪な魔物を倒し、 \n美味しいスイーツやお菓子がいっぱいのハロウィンパーティーに参加しましょう！ ": "那么，正义的伙伴们，加油！打倒邪恶魔物，\n参加有许多美味甜点和零食的万圣节派对吧！ ",
        "コホン、 以上、 心優しい星の魔女、 ノースカロライナでした！ \nそれでは次の定期放送の時間に会いましょう！ ": "咳咳，以上就是心地善良的星之魔女北卡罗来纳！\n下次定期播报时再见！ ",
        "この責任は、 しっかりとっていただきますよ？ ": "这份责任，你们可要好好承担哦？ ",
        "皆さんには二手に分かれて行動してもらいます。 ": "大家分成两队行动。 ",
        "まずは基地に残り、 新しいパーティー会場を探し、 \n予備の道具を使って会場を飾り、 パーティーの料理などを用意するチーム。 ": "首先是留在基地，寻找新的派对会场，\n用备用道具装饰会场并准备派对料理的队伍。 ",
        "担当者は……伊勢にしましょう。 ": "负责人……就定为伊势吧。 ",
        "……どこまでふざけてるんだ、 この基地は。 ": "……这个基地到底要胡闹到什么程度。 ",
        "そんなことより、 さっきのはもしかして……\nくそ！ なぜ私を見て逃げるのか……": "比起那个，刚才那个难道是……\n可恶！为什么一看到我就逃走……",
        "こっちに来ているはずだが……うん？ ": "应该是往这边来了……嗯？ ",
        "おおおお！ \nいい、 いいよ！ この調子！ ": "哦哦哦哦！\n很好，很好！保持这个劲头！ ",
        "頑張って、 島風ちゃん！ ": "加油，岛风！ ",
        "うん、 うぅ……": "嗯，呜……",
        "うんんんんん……！ ": "嗯嗯嗯嗯嗯……！ ",
        "うわ！ ": "哇！ ",
        "ああああ……！ ": "啊啊啊啊……！ ",
        "——みょうみょうちゅんちゅんコンビによる、熱い曲でした！\nありがとうございました！": "——这是妙妙啾啾组合带来的热情歌曲！\n谢谢大家！",
        "全力を尽くしました。思い残すことは何もありません！\n私たちの曲は向こうに負けることはないと信じています！": "我们竭尽全力，没有留下任何遗憾！\n我相信我们的歌曲绝不会输给对方！",
        "えええ——！ \nどうしてなのよ！ ": "诶诶诶——！\n为什么啊！ ",
        "そもそも、 最初に喧嘩を持ちかけたのは伊勢、 あなたではありませんか！ \n責任はとってもらいますよ！ ": "说到底，最先挑起争吵的不就是伊势你吗！\n你可要负责！ ",
        "……お姉ちゃんの頼みなら、 仕方がありませんね。 ": "……既然是姐姐的请求，那就没办法了。 ",
        "あっさり引き受けてくれたのかと思いきや、 \n結局やはり面倒なことを日向に押しつけるのですね！ ": "我还以为你会爽快答应，\n结果最后还是要把麻烦事推给日向吗！ ",
        "駄目です。 \n今回は「日向に丸投げ」は禁止です！ ": "不行。\n这次禁止“全都丢给日向”！ ",
        "それはまたどうしてなのよ！ \n日向も別に嫌がっていないし、いいじゃないか！": "那又是为什么啊！\n日向也没有不情愿，这样不就好了吗！",
        "安心してください、 私はあなたたちの姉妹愛のあり方に口出しする興味がありません。 ": "请放心，我没有兴趣干涉你们姐妹相爱的方式。 ",
        "今回日向に押し付けるのを禁止したのは、 日向には他に頼みたい仕事があるからです。 \nパーティーに必要な他の材料と一番肝心な「クリスマスプレゼント」を買ってきてもらわないと。 ": "这次禁止把事情推给日向，是因为我还有别的工作要拜托日向。\n需要她买来派对所需的其他材料，以及最重要的“圣诞礼物”。 ",
        "あとすこしなのに……！ ": "明明只差一点了……！ ",
        "<color=#f14949>ブラディーレディ？</color>": "<color=#f14949>血腥女士？</color>",
        "残念だ。 ": "真遗憾。 ",
        "制限時間内にこの水が入っているボウルから、浮いているリンゴを\nくわえて取ってもらえないと、ここを通すわけにはいかない。 ": "如果不能在时限内从这个装水的碗里\n叼出漂浮的苹果，就不能通过这里。 ",
        "そこまで厳しくしなくてもいいんじゃないですか、 羽黒！ \nさっきはもう水から離れたでしょう！ ": "没必要这么严格吧，羽黑！\n刚才苹果已经离开水面了吧！ ",
        "また落ちたじゃないか。 \nアウトだ。 ": "不是又掉下去了吗。\n淘汰。 ",
        "本当に顽固ですね。 すこしだけ融通を利かせてくださいよ。 \n島风はもう何度もやってたのに、 可哀想とか思わないのですか？ ": "真是顽固。稍微通融一下吧。\n岛风已经试了很多次了，你不觉得她很可怜吗？ ",
        "制限時間内にこの水が入っているボウルから、 浮いているリンゴを\nくわえて取ってもらえないと、 ここを通すわけにはいかない。 ": "如果不能在时限内从这个装水的碗里\n叼出漂浮的苹果，就不能通过这里。 ",
        "制限時間内にこの水が入っているボウルから、 浮いているリンゴを\nくわえて取ってもらえないと、 ここを通すわけには行かない。 ": "如果不能在时限内从这个装水的碗里\n叼出漂浮的苹果，就不能通过这里。 ",
        "……やっぱり、大晦日にケーキはおかしいか？": "……果然，除夕吃蛋糕很奇怪吗？",
        "いいえ、何でもない。さあ、お前のおすすめの鍋料理をいただこうか。\nそしてこのケーキは、夜のデザートにしよう。": "不，没什么。来尝尝你推荐的火锅吧。\n至于这个蛋糕，就当作晚上的甜点。 ",
        "あ、 いいえ、 あの、 伊勢さんの悪口ではなくて、 これはあの……": "啊，不，那个，我不是在说伊势小姐的坏话，只是这个……",
        "伊勢さんが日向さんより劣っているという意味ではない。 \nもちろんその逆でもない。 「リスクとリターン」の問題だろう。 ": "我不是说伊势小姐不如日向小姐。\n当然也不是相反。“风险与回报”的问题罢了。 ",
        "日向さんやあなたの姉と知り合って間もないが、 日向さんは冷静で穏やかなひとだと思う。 \nそれに対して、伊勢さんは明るい性格を持ち、 大胆で、 冒険心のあるタイプだ。 ": "我认识日向小姐和你姐姐没多久，但我觉得日向小姐冷静又温和。\n相较之下，伊势小姐性格开朗、大胆，是富有冒险精神的类型。 ",
        "っぐ！ \n酷い言われようなのに、 反論できないかも！ ": "呜！\n明明被说得这么难听，我却可能无法反驳！ ",
        "しっかりしてヒーアマン！ \n私たちはまだ子供だから、 そこに入っていませんから！ ": "振作点，希尔曼！\n我们还是孩子，所以不在那个范围内！ ",
        "あんたらの土地に足を踏み入れたのは、 悪意があったからではない。 \nただ個人的に解決しなければならないことがあるからだ。 ": "我踏入你们的土地并没有恶意。\n只是有些必须由我个人解决的事情。 ",
        "騒ぎにするつもりはない。 \n私のことを見なかったことにしてもらえないかな。 ": "我无意闹出骚动。\n能不能当作没看见我？ ",
        "……そんなことを言えば、 あたしたちが「はいわかった」って\n簡単にうなずくと思っていないでしょうね。 ": "……你不会以为你这么说，我们就会轻易地点头说“好，知道了”吧。 ",
        "その「個人的に解決しなければならないこと」とは、 何のこと？ ": "你说的那件“必须由你个人解决的事情”，究竟是什么？ ",
        "あんたらに言うつもりはない。 ": "我没打算告诉你们。 ",
        "なるほど、 よく分かった。 \nあんたは……喧嘩を売りに来たのね。 ": "原来如此，我明白了。\n你是来挑衅的吧。 ",
        "ちょ、 ちょっと待ってください、 羽黒さん！ \nこの方のことは資料で見たことがあります。 敵ではないと思いますが……": "等、等一下，羽黑小姐！\n我在资料里见过这位。我想她不是敌人……",
        "でも、 どうしてここに？ この空気、 まずくないですか？ \n何か収拾のつかない事態になるんじゃ……": "但是，为什么会在这里？气氛不太对吧？\n不会演变成无法收拾的事态吧……",
        "はいそこまで~ ！ ": "好，到此为止～！ ",
        "伊勢さんをサンタクロースに選べば、 \nいい意味で思いがけないサプライズをしてくれる可能性が高い。 ": "如果选伊势小姐当圣诞老人，\n她很可能会带来意想不到的惊喜，而且是好的方面。 ",
        "しかし同時に、 その驚きが悪い方向に傾けてしまう「リスク」もある。 ": "但同时，也存在这份惊喜朝坏的方向发展的“风险”。 ",
        "どうした、 私の天使？ \nそんな純粋で熱い目で見られると、 あなたを可愛がりたい衝動を抑えられなくなるのだが。 ": "怎么了，我的天使？\n被你用如此纯真而炽热的目光注视，我就控制不住想疼爱你的冲动了。 ",
        "きゃっ！ ？ \n見ていません、 見ていませんからこっちに来ないでください！ ": "呀！？\n我没看，我真的没看，请不要过来！ ",
        "ですから～今年のハロウィンイベントを取り仕切るロドニーさんと話し合ったら、 \nいい考えを思いつきました。 それが今回の—— ": "所以～我和负责今年万圣节活动的罗德尼小姐商量后，\n想到了一个好主意。那就是这次的——",
        "ジャンジャン！ \n悪霊退散！ ハロウィン大冒険イベント—— でした！ ": "锵锵！\n恶灵退散！万圣节大冒险活动——！ ",
        "もう、 二人とも、 そんなにイライラピリピリしないでくださいよ。 ゼーっちはあたしが誘ったの。 \nみんなと同じ、 今回のハロウィン大冒険イベントの参加者ですよ！ ": "真是的，你们两个别这么焦躁紧张嘛。泽泽是我邀请来的。\n和大家一样，也是这次万圣节大冒险活动的参加者！ ",
        "ハウ……！ \n結局あんたの仕業か。 ": "豪……！\n结果还是你搞的鬼吗。 ",
        "ちょっと待って、 適当なことを言うな。 \n私はその大冒険とやらに参加するとは一言も……": "等一下，别随便胡说。\n我从没说过要参加那个所谓的大冒险……",
        "チッチッチッ～だめですよ、 ゼーっち。 知っていますか、 ブルースフィアには\n「郷に入れば郷に従え」ということわざがあります。 ": "啧啧啧～不行哦，泽泽。你知道吗，蓝星有句谚语叫\n“入乡随俗”。 ",
        "ゼーっちがここで「何を」探したいのかは分かりませんが、 \n今日はパールベイ基地にとって、 年に一度しかないハロウィンなのです。 ": "我不知道泽泽想在这里寻找“什么”，\n但今天可是珍珠湾基地一年一度的万圣节。 ",
        "今日のためにたくさんの努力をしてきた者もいます。 \nみんな、 この日を思う存分楽しみたい、 最高のハロウィンにしたいと思っているのです。 ": "有人为今天付出了许多努力。\n大家都想尽情享受这一天，度过最棒的万圣节。 ",
        "そんな中で、 ゼーっちだけがひたすらに自分のことをして、 せっかくの雰囲気をぶち壊すのは、 \nちょっとだけここのみんなに失礼じゃないかしら？ ": "在这种情况下，只有泽泽一味做自己的事，破坏好不容易营造的气氛，\n是不是有点对不起这里的大家？ ",
        "くろっちはあなたが「喧嘩を売りに来た」と言ったのも、 \nあながち間違いではないでしょう。 ": "黑黑说你“是来挑衅的”，\n也不能说完全错了。 ",
        "そもそも私を無理矢理巻き込んだのはあんたじゃないか！\nなぜ偉そうに説教するみたいな口ぶりになっているんだ。": "说到底，强行把我卷进来的不就是你吗！\n为什么还摆出一副高高在上训人的口吻？",
        "……でも、あんたの言っていることも一理あるか。 \nここはあんたらの土地だ。 私もそれなりの礼儀を払うべきだった。 ": "……不过，你说的也有道理。\n这里是你们的土地，我也应该遵守相应的礼节。 ",
        "（お、 おお……これは、素直ですね。 ）": "（哦、哦哦……这还真是坦率。 ）",
        "（あのヒトは確か、 ゼータさんでしたよね。 \nもしかしたら、 とてもまっすぐなヒトかもしれません。 ）": "（那个人应该是泽塔小姐吧。\n说不定是个非常率直的人。 ）",
        "あんたの話によれば、 つまり私がそのイベントとやらに参加すれば、 \nここで堂々と行動していいんだな？ ": "照你所说，也就是说只要我参加那个所谓的活动，\n就可以在这里光明正大地行动了？ ",
        "その通り。 \nでもまあ、 今日だけの話ですけどね~": "没错。\n不过嘛，只限今天哦～",
        "ジョージ姉に話を通しておいたけど、 \n今日限りの特別招待券みたいなものですよ。 ": "我已经和乔治姐姐打过招呼了，\n这就像是仅限今天的特别邀请券。 ",
        "なにしろハロウィンというのは、 妖怪や霊など「出て来てはいけないものが街中を歩くのが当たり前」\nのような日ですから。 ": "毕竟万圣节是一个“妖怪和幽灵等不该出现的东西在街上行走很正常”\n的日子。 ",
        "何を言っているのかよく分からないが……いいだろう。 \n参加しょう。 ": "虽然不太明白你在说什么……好吧。\n我参加。 ",
        "……それで、 そのはろうい？ \nというのは何だ？ ": "……那么，那个万圣节？\n究竟是什么？ ",
        "（そこから！ ？ ）": "（从这里开始！？）",
        "……なるほど、 大体分かった。 ": "……原来如此，大致明白了。 ",
        "このようなつまらないプレゼントは、 せいぜい及第点と言ったところでしょう。 \nお姉ちゃんなら、 ジョージ5世指揮官にもっと素敵な贈り物をするはずです。 たとえば……": "这种无聊的礼物，顶多只能算及格吧。\n如果是姐姐，一定会送乔治五世指挥官更棒的礼物。比如……",
        "「1日サボり券」？ 飲めば嫌なことを忘れられる奇妙な錠剤？ \nあるいは、 あのムーバーの駆逐艦の等身大ぬいぐるみとか……": "“一日偷懒券”？吃下去就能忘记烦恼的奇怪药片？\n或者那艘搬运者驱逐舰的等身大玩偶……",
        "どうも～！ お疲れさん！ ": "嗨～！辛苦啦！ ",
        "エッセンシャルオイルがいいです！ \nジョージ5世さんに変なものを贈ろうとしないでください！ ": "精油就很好！\n请不要想着送乔治五世小姐奇怪的东西！ ",
        "なあ、その「ムーバーの駆逐艦の等身大ぬいぐるみ」について、 詳しく話してくれないか？ \nとても興味があるんだ。 ": "那个，关于“搬运者驱逐舰的等身大玩偶”，能详细说说吗？\n我非常感兴趣。 ",
        "駄目です！ ！ そんなことより早く次の人へのプレゼントを探しましょう！ \n行きましょう日向さん！ ねえ！ ？ ": "不行！！比起这个，快点寻找给下一个人的礼物吧！\n我们走吧，日向小姐！喂！？ ",
        "これで全員へのプレゼントがそろいましたね！ \nお疲れ様です、 日向さん。 ヒッパーさんもお疲れ様でした。 ": "这样所有人的礼物都准备好了！\n辛苦了，日向小姐。希佩尔小姐也辛苦了。 ",
        "えへへ！ もっと褒めてくれてもいいんだよ！ ": "嘿嘿！你可以再多夸我几句哦！ ",
        "このブルースフィアでは、 今日が秋と冬の変わり目で、 \nこの日に死者の魂が家に帰ってくると考える文化がある。 ": "在这颗蓝星上，人们认为今天是秋冬交替之日，\n死者的灵魂会在这天回到家中。 ",
        "その死者の魂を怒らせないために、 家に食べ物や飲み物を用意しておくのが伝統的なやり方だ。 \n悪霊に見つからないように、 子供たちも魔女やお化けの格好をする。 ": "为了不激怒死者的灵魂，传统做法是在家里准备食物和饮料。\n为了不被恶灵发现，孩子们也会打扮成魔女或幽灵。 ",
        "しかし今になっては、 ただまともじゃない大人も子供に混じって、 \n妙か格好をして騒いだりおやつをねだったりする日だ。 ": "但到了现在，这已经变成连不正常的大人也混在孩子中间，\n穿着奇怪服装吵闹、索要零食的日子。 ",
        "……ブルースフィアのやつらは、 \n本当に救いようがないな。 ": "……蓝星上的家伙们，\n真是无可救药。 ",
        "ごめんなさい通してください命がかかっているの！ 主にあたしの命が！ ": "对不起，请让我过去，事关性命！主要是我的性命！ ",
        "なに！ ？ \nどういうこと！ ？ ": "什么！？\n怎么回事！？ ",
        "さっき走っていた子……\n年から見ると、 パールベイ学園の生徒なのでは？ ": "刚才跑过去的孩子……\n从年龄来看，应该是珍珠湾学园的学生吧？ ",
        "授業中のはずなのに、 さっきから何人もの学生を見かけるなんて……\nどうやら噂通り、 学園の状況はあまりよくないのね。 ": "明明应该在上课，却从刚才起就看到好几个学生……\n看来和传闻一样，学园的状况不太好。 ",
        "すこし急ぎましょう。 ": "我们稍微加快脚步吧。 ",
        "それともなに？ 羽黒さんは可哀想な女の子の無様で恥ずかしがる姿を見て\n興奮するようなタイプですか？ ブラディーレディ様？ ": "还是说，羽黑小姐是那种看到可怜女孩狼狈又害羞的样子\n就会兴奋的类型吗？血腥女士大人？ ",
        "うるさい、 そんなわけないでしょう！ \nあとその名で呼ぶな、 恥ずかしい！ ！ ": "吵死了，怎么可能！\n还有别用那个名字叫我，太羞耻了！！！ ",
        "あたしだってこんなことをしたくてしたんじゃない！ \nでもあのふたりに頼まれたら、 断れないじゃないか！ ": "我也不是想做这种事才做的！\n可是那两个人拜托我，我怎么能拒绝！ ",
        "あのふたりって……そういえば、 確か今回のイベントはロドニーさんとジョージ5世さんの妹、 \nハウさんが合同で主催したものでしたっけ。 それなら納得ですね。 ": "那两个人……说起来，这次活动确实是罗德尼小姐和乔治五世小姐的妹妹，\n豪小姐共同主办的吧。那就说得通了。 ",
        "純粋に耳を痛めつけるような曲も何曲あったけどね……": "也有好几首纯粹折磨耳朵的歌曲就是了……",
        "ところで、この中継は指揮官さまも見ているかもしれないって本当？\nおーい！指揮官さま、一体いつになったら帰ってくるつもりなの？": "话说回来，指挥官大人真的可能也在看这场直播吗？\n喂——！指挥官大人，你到底打算什么时候回来？",
        "うん？ あれれ？ \nクリスマスプレゼントを買うって、 それってつまり……！ ": "嗯？咦咦？\n要买圣诞礼物，也就是说……！ ",
        "そう、 日向——今年のパールベイの「サンタクロース」はあなたに決めました。 \nおめでとうございます。 みんなのために、 良いプレゼントを選んできなさい。 ": "没错，日向——今年珍珠湾的“圣诞老人”就决定是你了。\n恭喜。去为大家挑选一份好礼物吧。 ",
        "えっ！ ？ 日向さんがサンタになるの！ ？ ": "诶！？日向小姐要当圣诞老人！？ ",
        "でもやはり、 結局はお姉ちゃんではなく、 私をサンタクロースに選んだなんて……": "但没想到最后没有选姐姐，而是选我当圣诞老人……",
        "ジョージ5世指揮官は、 もっと慧眼の持ち主かと思っていましたが。 ": "我还以为乔治五世指挥官会更有慧眼呢。 ",
        "おや、 すこしは喜ぶかと思ったが、 まさか本気で不満を感じているのか。 ": "哦，我还以为你会稍微高兴一点，难道你真的感到不满吗。 ",
        "なにしろ日向さんは、 本当にお姉ちゃんの伊勢さんのことが大好きなんですからね……": "毕竟日向小姐真的非常喜欢她的姐姐伊势小姐……",
        "でも、 「面白いから」って何をするかわからない伊勢さんより、 \n私も日向さんのほうが断然いいサンタさんになれると思います。 ": "不过，比起因为“觉得有趣”就不知道会做出什么的伊势小姐，\n我也觉得日向小姐绝对能成为更好的圣诞老人。 ",
        "ハロウィンだから、 絶対ふざけたことを企むやつが出てくると思っていたけど、 \nあの二人が主催者なら安心……かな？ あれ？ むしろこのイベント自体がふざけたものでは……？ ": "因为是万圣节，我本以为一定会有人策划胡闹的事情，\n如果是那两个人主办就放心了……吧？咦？倒不如说这个活动本身就很胡闹……？ ",
        "とにかく、 あんたたちはさっさと次のところに行ってよ。 \nあんたたちがずっとここで騒ぐから、 この子がうまくできないのかもしれないし。 ": "总之，你们赶紧去下一个地方。\n也许就是因为你们一直在这里吵闹，这孩子才做不好。 ",
        "あれれ？ それって、 もしかして羽黒さんなりに\n島風ちゃんに気を配っているのですか～？ ": "咦咦？难道说，羽黑小姐是在用自己的方式\n关心岛风吗～？ ",
        "ち、 ちがう。 \n適当なことを言わないでよね！ ": "不、不是。\n别胡说八道！ ",
        "いや～お誘いいただき、 ありがとうございます。 \nでも、 あたしはただの一般人ですから、 こういう目立つことはあまり……あはは～": "哎呀～谢谢你的邀请。\n不过我只是普通人，不太适合这种引人注目的事情……啊哈哈～",
        "そう？ そんなに急いで返事しなくても大丈夫よ。 \nまずはこの名刺を受け取ってから、 ゆっくり考えよう。 ": "是吗？不用这么急着回答也没关系。\n先收下这张名片，再慢慢考虑吧。 ",
        "いいえ、 いええ、 結構です。 \n（たとえ世界の外側から来た超次元的な力でもこの瑞鶴さんを頷かせないからね！ ）": "不、不用了，谢谢。\n（就算是来自世界之外的超次元力量，也别想让瑞鹤小姐点头！）",
        "そ、 そうだ！ 今朝出かけた時に片方のスリッパが７度くらいずれていたのを思い出した！ \nそれはいけない！ すぐ戻って直さないと！ ": "对、对了！我想起今天早上出门时有一只拖鞋偏了大约七度！\n这可不行！得马上回去修正！ ",
        "あっ、 先輩。 ": "啊，前辈。 ",
        "もっとまともな言い訳を考えないと、 また狙われますよ。 ": "不想个更像样的借口，还会再次被盯上的。 ",
        "やはり面白い子ね。 \n舞台に上がらせたいわ。 ": "果然是个有趣的孩子。\n真想让她登上舞台。 ",
        "ほら……": "你看……",
        "おや、申し訳ありません。こちらのお客様は顔出しNGの謎のゲストでした。\nシカゴさん、お客様たちの顔が見えないように、モザイクをかけましょう。": "哎呀，实在抱歉。这位客人是拒绝露脸的神秘嘉宾。\n芝加哥小姐，请给客人的脸打上马赛克。 ",
        "では、もう一度。パールベイの年越しパーティーに参加して、いかがでしたか？\n楽しかったですか？": "那么，再问一次。参加珍珠湾的跨年派对，感觉如何？\n玩得开心吗？",
        "モザイクって……それもまずいではありませんか？\nまあ、あなたが言い出したのですから、大丈夫でしょうが……": "马赛克……那样也不太妥当吧？\n不过既然是你提议的，应该没问题吧……",
        "ちょっと待ってください、 お姉ちゃん。 \nそんな目立つようなこと、 私はあまり……": "请等一下，姐姐。\n这种引人注目的事情，我不太……",
        "ほかの候補者はどいつもこいつもろくな人間ではありません。 \nサンタクロースという大事な使命は、 日向しか頼めません。 ": "其他候选人一个个都不是什么正经人。\n圣诞老人这项重要使命，只能拜托日向。 ",
        "持ち場は違いますが、 姉妹二人で力を合わせようではありませんか。 \nみんなに楽しんでもらえるクリスマスを作りましょう。 ": "虽然负责的岗位不同，但让我们姐妹二人齐心协力吧。\n一起创造一个让大家开心的圣诞节。 ",
        "「クリスマスパーティーを一から準備しなければならない」アクシデントが起きた今、 \nジョージ5世が日向さんを選んだ理由の一つは、 日向さんの安定性に目をつけたからだろう。 ": "如今发生了“必须从头准备圣诞派对”的意外，\n乔治五世选择日向小姐的理由之一，大概是看中了她的稳定性。 ",
        "日向さんなら、 伊勢さんのようにみんなを驚かせることはできないかもしれないが、 \nそれでも必ずみんなに満足してもらえる結果を出してくれると信じているだろう。 ": "如果是日向小姐，或许无法像伊势小姐那样给大家带来惊喜，\n但乔治五世一定相信她能交出让所有人满意的结果。 ",
        "そう……でしょうか。 \nそれでしたら、 ジョージ5世指揮官の期待を裏切るわけにはいきませんね。 ": "是……这样吗？\n既然如此，就不能辜负乔治五世指挥官的期待了。 ",
        "（日向さんの機嫌を直しました！ \n特に付き合いが長いわけでもないのに、 日向さんの心理を正確に把握できるなんて……）": "（让日向小姐恢复心情了！\n明明交往时间并不长，却能准确把握日向小姐的心理……）",
        "（やはり変態さんはちょっと軽薄で不真面目なところを除けば、 すごい人なんですね……）": "（变态先生果然是个厉害的人，只是稍微有些轻浮和不认真……）",
        "……ふん。 \n気づかれるつもりはなかったが……仕方がない。 ": "……哼。\n本来不打算被发现的……没办法。 ",
        "あんた、 なかなか鋭いじゃないか。 ": "你还挺敏锐的嘛。 ",
        "あ、 あの、 日向さん！ 早速みんなへのクリスマスプレゼントを探しに行きましょう！ \n手伝いますから、 是非日向さんの隣にいさせてください！ ": "啊、那个，日向小姐！我们马上去寻找给大家的圣诞礼物吧！\n我会帮忙的，请务必让我待在日向小姐身边！ ",
        "フッドもラフィーも、完全にあいつらと打ち解けたようで……\nまあ、せっかくの新年だ。大目に見てやるか。": "胡德和拉菲似乎都完全和那群人打成一片了……\n算了，难得是新年，就睁一只眼闭一只眼吧。",
        "しかし僕たちはあいつらみたいに、新年だからといって\n堂々とはしゃいだりサボったりしない。そうだろう、ツェッペリン——": "但我们不会像那群人一样，因为是新年就大肆玩闹或偷懒。\n对吧，齐柏林——",
        "もちろんです。 むしろこちらからお願いしたいくらいです。 \n私と姉は何と言っても、 皆さんよりもずっと後からパールベイに配属されましたから。 ": "当然。倒不如说是我们想拜托您。\n毕竟我和姐姐比大家晚得多才被分配到珍珠湾。 ",
        "皆さんがどんなプレゼントをもらうと喜ぶのか、 \n先輩であるグローウォームさんやヒッパーさんのご意見をお聞かせください。 ": "大家收到什么礼物会开心呢？\n请萤火虫前辈和希佩尔前辈告诉我你们的意见。 ",
        "そんなにかしこまらなくてもいい。 \nせっかくパールベイの方に来たのだから、 もともと天使と一緒にショッピングモールでも回るつもりだ。 ": "不用这么拘谨。\n难得来到珍珠湾，我本来就打算和天使一起逛逛购物中心。 ",
        "私のアドバイスでよければ、 必要な時に遠慮なくいつでも言えばいいさ。 ": "如果我的建议有用，需要时尽管随时告诉我。 ",
        "では、 まずは……ジョージ5世さんへのプレゼントから始めましょうか？ \nジョージ5世さんが喜ぶプレゼントと言えば……": "那么，先从……给乔治五世小姐的礼物开始吧？\n说到乔治五世小姐会喜欢的礼物……",
        "……このエッセンシャルオイルはどうでしょうか。 \nストレスを癒し、 緊張をほぐす効果があると書かれています。 ": "……这瓶精油怎么样？\n上面写着有舒缓压力、放松紧张的效果。 ",
        "ジョージ5世指揮官はいつも基地のことで苦労していますから、 \nこれで仕事のストレスや疲れを和らげると思います。 ": "乔治五世指挥官总是为基地的事务操劳，\n我想这个能缓解她工作上的压力和疲劳。 ",
        "首マッサージャーや栄養ドリンクも考えられますが、 \nジョージ5世指揮官がすでに持っていると記憶しています。 ": "颈部按摩器和营养饮料也可以考虑，\n但我记得乔治五世指挥官已经有了。 ",
        "消耗品ですから、 たまたま他の方から似たようなものを貰っても困らないはずです。 ": "这是消耗品，就算碰巧从别人那里收到类似的东西，也不会为难。 ",
        "ジョージ5世指揮官が使っているハンドクリームはこの香りに近いですから、 \nこのエッセンシャルオイルの香りが苦手ということもないでしょう。 ": "乔治五世指挥官使用的护手霜香味和这个很接近，\n所以她应该不会讨厌这瓶精油的香味。 ",
        "それに、 ジョージ5世指揮官の部屋に加湿器があったのを見たことがあります。 \nこれは加湿器対応のものですから、 加湿器に垂らせば簡単に使います。 ": "而且，我见过乔治五世指挥官房间里有加湿器。\n这个精油适用于加湿器，滴进去就能轻松使用。 ",
        "それは……とてもいいと思います。 ": "这个……我觉得非常好。 ",
        "よく考えているじゃないか。 どうやらジョージ5世の目に狂いはないようだ。 ": "考虑得很周到嘛。看来乔治五世的眼光没有错。 ",
        "？ 恐縮です。 これはたいしたことではありません。 \nあくまでも普段目にしたものを「情報」として覚えているだけですから。 ": "？过奖了。这没什么大不了的。\n我只是把平时看到的东西当作“信息”记住而已。 ",
        "ふぅ……一応、 ここまでか。 ": "呼……暂时到这里吗。 ",
        "ゼーっちお疲れ～いや～予想よりずっといい反応ですね。 \nこれであたしも一安心です。 ": "泽泽辛苦啦～哎呀～反应比预想中好得多呢。\n这样我也放心了。 ",
        "正直、 ここに座って選択肢をクリックするだけなのに、 \nこんなに疲れるとは思わなかった……": "说实话，明明只要坐在这里点击选项，\n没想到会这么累……",
        "瑞鶴さま、 精神的な疲労は肉体的なものとは異なります。 ": "瑞鹤大人，精神上的疲劳和肉体上的疲劳不同。 ",
        "そうか……よし、 わかった。 \nすぐにでもそれを鍛える方法を考えないと——": "这样啊……好，我明白了。\n必须马上想办法锻炼它——",
        "真面目スイッチ、 オフ～\nゼーっち、 前にも言ったでしょう。 ": "认真开关，关闭～\n泽泽，我之前也说过吧。 ",
        "心の余裕を作らないとって。 \n事あるごとにすぐ突っ走るのはよくないよ。 ": "要给心里留出余裕。\n一有事情就立刻冲出去可不好。 ",
        "あなたが抱えている 「難問」 は、 \nとても困難で、 解決の糸口が見かりそうにないことです。 ": "你所面对的“难题”，\n是非常困难、看不到解决头绪的问题。 ",
        "だからこそ、 向こう見ずの無理やりなやり方では、 \nいい結果を得られません。 ": "正因如此，鲁莽而强硬的做法\n无法得到好的结果。 ",
        "どのプレゼントも、 送る相手のニーズや好みに合うものだな。 \nきっと喜んでくれるよ。 ": "每件礼物都符合收礼人的需求和喜好。\n他们一定会喜欢的。 ",
        "よかったですね！ \nさすが日向さんです。 ": "太好了！\n不愧是日向小姐。 ",
        "もしかして、 なにかアドバイスできるかと思いましたが……\nでも、 その必要はまったくありませんでしたね。 ": "我本来还想或许能给些建议……\n不过看来完全没有这个必要。 ",
        "少なくともあたしからすると、 ここで猪突猛進するより、 \n意地を見せるほうがよっぽど勇気のあることですよ。 ": "至少在我看来，比起在这里一味横冲直撞，\n展现自己的坚持才更需要勇气。 ",
        "……つまり、 やはりあんたはただの遊び心で\nこれを作ったわけではないのか。 ": "……也就是说，你果然不是单纯出于玩心\n才制作这个的吗。 ",
        "言ったでしょう？ \nこれは 「みんな」 に楽しく年末を過ごしてもらうために用意したものだって。 ": "我说过吧？\n这是为了让“大家”愉快地度过年末而准备的。 ",
        "あぁ。 確かに。 \n少しは笑えた。 ": "啊。确实。\n至少让我笑了一下。 ",
        "イエーイ～！ \n大 · 成 · 功！ ": "耶～！\n大·成·功！ ",
        "じゃあお祝いのために、 今晩はいいもの食べようよ！ \nいつの間にこんな時間だし。 ": "那今晚吃点好东西庆祝吧！\n不知不觉已经这么晚了。 ",
        "ここがあなたのせいで食料不足になりそうってことを、 もう忘れたのか！ ": "你已经忘了这里差点因为你而食物短缺吗！ ",
        "\n\n その後、 ハウが最初から宣言した通り、 \n  『ときめき♥パールベイ学園BOR』 は全員に配られ、 \n 多くの好評を得た。 ": "\n\n之后，正如豪一开始所说，\n《心动♥珍珠湾学园 BOR》分发给了所有人，\n并获得了许多好评。 ",
        "\n\n しかし一番人気なのは、 \n それを 「テスト記録撮影」 という名義で撮れた映像を\n 編集してアップロードした数々のゲーム実況である。 ": "\n\n不过最受欢迎的，\n还是以“测试记录拍摄”名义拍下的影像\n剪辑上传后形成的众多游戏实况。 ",
        "っく！ 敵の火力はますます強くなったな。 \nついにこのクソみたいな世界とはさよならか……": "可恶！敌人的火力越来越强了。\n终于要和这个糟糕透顶的世界告别了吗……",
        "しっかりして！ 明日が 『瑞鶴さまがゲームをやる12』 の配信日よ！ \n死んだらもう見られないでしょう！ ": "振作一点！明天是《瑞鹤大人玩游戏12》的播出日！\n死了就再也看不到了！ ",
        "……おっおぉ……おおおぉぉぉおおお！ ": "……哦、哦哦……哦哦哦哦哦哦！ ",
        "こちとら瑞ちゃんの単推しなんだ！ \nここでお前らみたいなクズに殺されて……たまるか！ ！ ！ ": "我可是瑞酱的唯一推！\n怎么能在这里被你们这种人渣杀掉……我不答应！！！ ",
        "こんな理由であんなに生還率が上がるなんて……": "没想到这种理由竟然能让生还率提高那么多……",
        "いいことでしょう？ ": "这不是好事吗？ ",
        "……死を救済とするよりは。 ": "……总比把死亡当成救赎好。 ",
        "制限時間内にこの水が入っているボウルから、 浮いているリンゴを\nくわえて取ってもらえないと、 ここを通すわけにはいかない。 ": "如果不能在时限内从这个装水的碗里\n叼出漂浮的苹果，就不能通过这里。 ",
        "よし、 ではそろそろ計画の第二段階に入りましょうか。 \n次は生放送でするつもりよ、 それで視聴者たちにも参加してもらって——": "好，那么差不多该进入计划的第二阶段了。\n接下来打算进行直播，让观众们也参与进来——",
        "お願いだから勘弁してくださいよ、 ハウお嬢さま！ ": "求求你放过我吧，豪小姐！ ",
        "でも……まあ、食べ物に罪はない。\nいただくとするか。": "不过……食物总没有罪。\n那我就吃了吧。",
        "なんか、古い冒険小説の展開っぽいなあ……": "总觉得像老冒险小说里的展开啊……",
        "ふん、 もうすぐクリスマスって時期にこそこそしているということは……\n絶対雑誌に書いてある 「男ができた」 から、 人を邪魔者扱いしているんだ！ 絶対！ ": "哼，都快到圣诞节了还偷偷摸摸的……\n绝对是因为杂志上写的“交了男朋友”，才把别人当成碍事的人！绝对是！ ",
        "あ、 それはないね。 ": "啊，那倒不至于。 ",
        "（どこから出てきた！ ？ ）": "（你是从哪里冒出来的！？）",
        "あっ、 まだ瑞鶴さんに紹介していませんね。 \nこちらは私の姉の秘書であるカールスルーエさんです。 ": "啊，还没向瑞鹤小姐介绍呢。\n这位是我姐姐的秘书卡尔斯鲁厄小姐。 ",
        "ひどい！ じゃあたしがどうなってもいいっていうの！ ？ \nあたしだけ正月太りになっちゃう！ ？ ": "太过分了！那你是说我怎么样都无所谓吗！？\n难道只有我会新年发胖！？ ",
        "いまさらそんなこと言っても無駄でしょう。 \n幸いあの二人が使っているのは模擬弾だ。 本当に大怪我することはないでしょう。 ": "事到如今再说这些也没用了。\n幸好那两个人用的是演习弹，不会真的造成重伤。 ",
        "これだけ派手にやっているのだ、 誰か様子を見に来るかもしれない。 \n見つからないように、 どこかで適当にぶらついておくわ。 ": "闹出这么大动静，说不定会有人过来查看。\n为了不被发现，我先到某处随便逛逛。 ",
        "自己紹介は要らないだろう。 わたしが誰かは重要じゃない。 重要なのは、 わたしは天使たちに\n最高のクリスマスプレゼントを送るだけの情報、 財力そして行動力を持っていることだ。 ": "不用自我介绍了。我是谁并不重要。重要的是，我拥有\n为天使们送上最棒圣诞礼物所需的信息、财力和行动力。 ",
        "『！ ビスマルク先輩にイータさん、 お久しぶりです。 』": "『！俾斯麦前辈、伊塔小姐，好久不见。』",
        "それなら私に話しかけないで放って置いてくださいよ？ ！ ": "那就别和我说话，放着我不管啊！？ ",
        "ち、 違います！ ロドニー様はイータ様と親しい方なので、 \nこれは友達の間の冗談……みたいなものです。 ": "不、不是的！罗德尼大人和伊塔大人关系很好，\n这只是朋友之间的玩笑……之类的。 ",
        "お願いだからそんな勘違いされやすい言い方しないでください！  \nたまにあの人に相談に乗ってもらっているだけで、 別にプライベートで仲良くしているわけではありません！ ": "求求你别用这种容易让人误会的说法！\n我只是偶尔请她帮忙出主意，并不是私下关系亲密！ ",
        "どうでもいい。 ": "无所谓。 ",
        "覚悟を決める勇気があるかどうか、 その瞬間に試される。 \nその前のことはカウントされない。 ": "那一刻会考验你是否有下定决心的勇气。\n在那之前的事情不算数。 ",
        "『私はまだ手伝うかどうか決めていませんが、 今ここでこの話をされると……』": "『我还没决定要不要帮忙，但现在在这里被这样说……』",
        "そのぬいぐるみのことなら、 既にイータさんから話を聞いています。 \nご心配には及びません。 ": "如果是那只玩偶的事，我已经听伊塔小姐说过了。\n无需担心。 ",
        "つまり「まともじゃない大人」ってあたしのことなの？ ": "也就是说，“不正常的大人”指的是我吗？ ",
        "みんなが楽しく、 かつ安全にハロウィンを楽しめるために\n一生懸命頑張ってきたのに……傷つきますよ。 しくしく。 ": "为了让大家开心又安全地享受万圣节，\n我一直拼命努力……这样很伤人啊。呜呜。 ",
        "三流芝居はやめろ。 それにあんたの話によると、 その飴をくれないとぶっ飛ばす……？ \n風習と、 この基地で今やっている茶番とあまり関係ないじゃないか。 一体どういうことだ？ ": "别演三流戏了。而且按你说的，不给糖就要把人打飞……？\n这和习俗、以及基地现在上演的闹剧没什么关系吧。到底是怎么回事？ ",
        "それには理由があるのですよ。 知っていますか、 ゼーっち。 \nこの基地はね、 毎年ハロウィンになると必ず何かの騒ぎが起こるのですよ。 ": "这其中是有原因的。你知道吗，泽泽。\n这个基地每年一到万圣节，就一定会发生什么骚动。 ",
        "……ふざけているにしか見えない。 そもそもなぜ「悪霊」なんだ？ \nさっきは「魔物」って言っていたじゃないか。 一体どっちだ？ ": "……怎么看都像是在胡闹。说到底为什么是“恶灵”？\n刚才不是说“魔物”吗？到底是哪一个？ ",
        "細かいこと気にしないで～ほら、 悪い子たちが仕掛けるのをただ待つより、 \n他のことで彼女たちの注意を引いて、 状況を最初から自分の思うままに動かせた方がいいでしょう？ ": "别在意细节啦～与其等着坏孩子们出招，\n不如用其他事情吸引她们的注意，从一开始就让局势按自己的想法发展，不是更好吗？ ",
        "はい、 とにかく~！ ゼーっちはもう参加するって言っていましたし、 ではさっそく！ \n手始めに最初の試練—— アップルボビングから挑みましょう！ ": "好，总之～！泽泽已经说要参加了，那就马上开始！\n先从第一个试炼——咬苹果开始挑战吧！ ",
        "あれ？ ハウ？ \nどうしたの？ 変な顔して。 ": "咦？豪？\n怎么了？表情真奇怪。",
        "……いいえ、 ただどこか調整できる点がないかって考えているだけよ。 ": "……不，我只是在想有没有什么地方可以调整。",
        "（この黒猫の素材、 いつ入れたの？ \n記憶にないなんて……）": "（这只黑猫的素材是什么时候加入的？\n我怎么完全不记得……）",
        "はい、 何ですか。 ": "是，有什么事？ ",
        "猫がしゃべるなんて、 非現実的とは思わないか？ ": "猫会说话，你不觉得太不现实了吗？ ",
        "……本当のことを言えば、 今日瑞鶴さまの一部の言動と比べると、 \n大したことではないと思いますが。 ": "……说实话，和瑞鹤大人今天的某些言行相比，\n这根本不算什么。 ",
        "わたくしは素人なので、 アイドルには詳しくありませんが……\n 「変人」 が選抜の基準になっているのではありませんよね。 ": "我是外行，不太了解偶像……\n选拔标准不会是“怪人”吧。 ",
        "私にとってはそうよ？ ほら、 平々凡々な一般人なら、 \n大勢の視線と喝采を浴びされると、 すぐ気圧されてしまうでしょう？ ": "对我来说就是啊？普通平凡的人，\n受到众人的注视与喝彩，很快就会被压倒吧？ ",
        "はいはい、終わったらさっさと降りて舞台を譲りなさい。\nこれからはこのわたくしの出番だわ。": "好啦好啦，结束后赶紧下台，把舞台让给我。\n接下来轮到我了。",
        "現場の凡人どもよ！そして生中継を見ている凡人どもよ！\n骨の髄までメロメロに魅せてあげるから、覚悟しなさい！": "现场的凡人们！还有正在看直播的凡人们！\n我会让你们从骨子里为我倾倒，做好觉悟吧！",
        "えっと……ヴェネトさん、実はここからは観客のインタビューに入る予定なので、\n一旦楽屋のほうにお戻りになって、しばらくお待ちいただけますか？": "呃……维内托小姐，接下来计划采访观众，\n能否先回后台稍等一会儿？",
        "なっ……！？そういうことなら……先に言いなさいよ！\nうええええ恥ずかしい……！！": "什……！？既然这样……你倒是早点说啊！\n呜呜呜，好丢脸……！！",
        "みなさん、こんばんは。ロドニーです。\nわたしは今客席のほうにいます。": "大家晚上好，我是罗德尼。\n我现在在观众席。",
        "大晦日の夜に行われる、海辺の年越しパーティー。皆さんは楽しんでいるのでしょうか？\n実際に観客の方々から話を聞いてみましょう。": "除夕夜举行的海边跨年派对，大家玩得开心吗？\n让我们实际采访一下现场观众。",
        "どの曲も出演者の個性があふれていて、\nとても素敵な年越しパーティーだと思います。": "每首歌都充满出演者的个性，\n我觉得这是场非常棒的跨年派对。",
        "ああ。 聖なる夜に、 純真が飾られた寝室の中で、 最高の贈り物をそっと天使たちの隣に置く。 \nそして、 天使たちのその無防備な寝顔に——": "啊。在神圣的夜晚，在装饰着纯真的卧室里，把最棒的礼物轻轻放在天使身边。\n然后对着天使毫无防备的睡脸——",
        "はい、 アウトです！ ！ ！ ": "好，淘汰！！！ ",
        "こんなロリコン傾向のある危険な大人を、 サンタクロースにしてはいけません！ \nですよねジョージ5世さん！ ": "不能让这种有恋童倾向的危险大人成为圣诞老人！\n对吧，乔治五世小姐！ ",
        "そ、 そうですね……あなたの言う通りです。 \nしかしその前に、 すこし落ち着きなさい、 グローウォームちゃん。 ": "是、是啊……你说得对。\n不过在此之前，请稍微冷静一点，萤火虫。 ",
        "柱にこんな大きな亀裂が……あなたのその小さな体に、 そんな力が秘められているとは……": "柱子上竟然有这么大的裂缝……没想到你小小的身体里蕴藏着那样的力量……",
        "というか、 これで全員脱落じゃないか。 \nじゃあいったい誰がサンタさんになるの？ ": "话说回来，这样不就全员淘汰了吗。\n那到底谁来当圣诞老人？ ",
        "──そうか、 分かったぞ！ ": "——原来如此，我明白了！ ",
        "え？ なに急に真顔になって……": "诶？怎么突然变得这么严肃……",
        "本当に欲しいものは、 \n誰かにねだるのではなく、 自分の力で勝ち取るのだよ！ ": "真正想要的东西，\n不是向别人索取，而是靠自己的力量争取！ ",
        "どう？ 挑戦してみないか？ \n舞台に上がれば、 あとは頭を空っぽにして、 好きなようにすればいいわ。 ": "怎么样？要不要挑战一下？\n只要登上舞台，之后放空脑袋，随心所欲就好。 ",
        "……先輩。 先輩の人生の選択に干渉するつもりはありません。 \nしかしどうか、 ちゃんと考えてから慎重に決めてください。 ": "……前辈。我无意干涉您的人生选择。\n但请您仔细考虑后再慎重决定。 ",
        "いやどうしてあたしがオーケー出すと思うの！ ？ \n恥ずかしがり屋の純粋な乙女瑞鶴さんには、 どう考えても不可能でしょう！ ": "不，你为什么觉得我会答应！？\n对害羞纯真的少女瑞鹤来说，无论怎么看都不可能吧！ ",
        "指揮官さま、お変わりございませんか。よいお正月をお過ごしください。\nこのような形ではなく、はやく指揮官さまのお顔を見ながらお話ししたいですね。": "指挥官大人，您一切还好吗？祝您新年愉快。\n真希望能早点面对面看着指挥官大人的脸聊天，而不是用这种方式。",
        "確かに、上官殿はどうしているのか気になるね。\n外ではいろいろと不便でしょう。": "确实，我也很在意长官现在怎么样。\n在外面一定有很多不便吧。",
        "あの連中のことですから、どうせ通常運転でしょう。朝日さんが祝おうと言い出したら、\nエリザベスさんや神通さんたちはきっと手伝うでしょう。": "以那群人的性格，肯定和平时一样。朝日小姐要是提议庆祝，\n伊丽莎白小姐和神通小姐她们一定会帮忙。",
        "ポートランドさんはグロイ見た目をしているけど、味は案外悪くない料理を作る。\nそれでオークランドと夕立あたりがわいわい騒いで、最後は茶番か修羅場になるでしょう。": "波特兰小姐做的料理外表虽然吓人，味道意外地还不错。\n然后奥克兰和夕立之类的人会吵吵闹闹，最后变成闹剧或修罗场。",
        "そんなことより、お姉さま！そばを持ってきました！\n一緒に食べましょうか？": "比起那些，姐姐！我带荞麦面来了！\n一起吃吧？",
        "ふふ、モロトフさんの発言には同意せざるを得ませんね。\nでは、こちらの方々にもお話を——": "呵呵，我不得不同意莫洛托夫小姐的话。\n那么，也和这边的各位聊聊——",
        "ちょ、ちょっと待ってください！困ります……！": "等、等一下！这可不行……！",
        "どこかの少年漫画のセリフみたいに言ってるけど、 \nそれは単に脱落したから駄々をこねているだけじゃないか！ ": "虽然说得像某部少年漫画里的台词，\n但你不就是因为被淘汰才在耍赖吗！ ",
        "でも……\nふんふん～いいアイデアかもね。 あたしは乗るよ！ ": "但是……\n嗯嗯～或许是个好主意。我参加！ ",
        "おおおお！ そうこなくっちゃ！ ": "哦哦哦哦！这才对嘛！ ",
        "出てこい、 僕の仲間たちよ！ 戦争だ戦争！ ": "出来吧，我的伙伴们！战争，开战了！ ",
        "グリッドレイ級に勝利を！ ": "为格里德利级的胜利！ ",
        "うまく収まって何よりだな。 めでたしめでたし。 ": "事情顺利解决就好。皆大欢喜。 ",
        "……意外と起こらないね。 \n面白いこと。 ": "……意外地什么都没发生。\n真没意思。 ",
        "あの、実はですね……": "那个，其实是……",
        "ほら、 面接に参加するなら、 服装にも力を入れておかないといけないでしょう？ ": "你看，既然要参加面试，服装也得用心准备吧？ ",
        "何を言ってるの、 ホーエル？ サンタさんの格好で面接？ \nそれは一体なんの面接——": "你在说什么，霍埃尔？穿成圣诞老人去面试？\n那到底是什么面试——",
        "えっ。 \nまさか……？ ": "诶。\n难道……？ ",
        "そのとおり！ \n今年のサンタクロースになるのは、 このホーエル以外考えられない！ ": "没错！\n今年的圣诞老人，除了我霍埃尔不作他想！ ",
        "おやおや、 自信満々なようだね、 ホーエルちゃん。 ": "哎呀呀，看来你信心十足啊，霍埃尔。 ",
        "でもね、 外見をいくら工夫したところで、 \n大切なのは日頃から積み重ねてきた信頼なのよ。 ": "不过，无论外表下多少功夫，\\n重要的还是平日积累的信任。 ",
        "サンタクロースはね、 クリスマス・イブという年に一度の特別な日に、 \n子供たちに夢と希望を与える重要な役割だ。 ": "圣诞老人啊，在一年一度的特殊日子——平安夜，\\n肩负着给孩子们带来梦想与希望的重要职责。 ",
        "そういう重要な役割を担うのは、 優しくて大人で、 人間味があって、 \n基地のみんなを大切にしている、 有能で信頼できる人物でなければならない。 ": "承担如此重要职责的人，必须温柔成熟、富有人情味，\\n珍惜基地里的每个人，并且能力出众、值得信赖。 ",
        "──たとえばあたしや、 あたしのような子や、 あたしみたいな子とか！ ": "——比如我，或者像我这样的孩子，又或者和我一样的孩子！ ",
        "いいえ違うわ。 実は、 期末のテストの成績があまりよくなくって、 \nついさっきまでアラスカ先生に職員室で説教されてただけよ。 ": "不，不是。其实只是因为期末考试成绩不太好，\\n刚才一直在办公室被阿拉斯加老师训话而已。 ",
        "いやいやいや、 伊勢さんが言っていた例は全部自分のことじゃないか！ ": "不不不，伊势小姐举的例子全都是在说自己吧！ ",
        "日向さんも、 どうして伊勢さんを止めてくれないのよ？ ": "日向小姐，你为什么也不阻止伊势小姐？ ",
        "（あれ？この人、食べることしか考えていない？）": "（咦？这个人脑子里只有吃吗？）",
        "まあまあ、食べ物じゃなくても、縁起物だから。\nきっとお姉さんも喜ぶと思うよ。": "好啦好啦，就算不是食物，也是吉祥物。\n姐姐一定也会高兴的。",
        "本当ですか？それなら、これを買って帰りましょう。": "真的吗？那我们买这个带回去吧。",
        "そうですね。お姉さんへの贈り物にしましょう。": "好啊，就当作送给姐姐的礼物吧。",
        "それでは、次はこちらを見てみましょう。": "那么，接下来看看这边吧。",
        "まあ、おしゃべりコーナーは\n少し後に回したほうがいいかもよ。": "聊天环节\n或许可以稍后再进行。",
        "ヘビー級選手のお出ましだね～": "重量级选手登场啦～",
        "む。\nラフィー、一人でもやれるの。": "嗯。\n拉菲一个人也能做到。",
        "はいはい～子供は意地張らないの。\nこっちが勝手に加わらせてもらっているんだから。": "好啦好啦～小孩子别逞强。\n毕竟是我们擅自加入进来的。",
        "そうだね……朝日たちのことは、\nゲームでよくある支援キャラとでも思えばいいよ～": "说得对……把朝日她们\n当成游戏里常见的支援角色就好啦～",
        "お、 おお……おはよう……": "哦、哦……早上好……",
        "……って、 ちょっと待って！ ？ \nまさかあたし、 一晩中気を失っていたのか！ ？ ": "……等等，等一下！？\n难道我昏迷了一整晚！？ ",
        "いいえ、 今は夕方です。 \n瑞鶴さんが気絶したのは三時間ほどです。 ": "不，现在是傍晚。\n瑞鹤小姐只昏迷了大约三个小时。 ",
        "これは新しい年を迎える、\nすこし前の物語である——": "这是迎接新年之前不久发生的故事——",
        "わあ……とてもにぎやかですね！": "哇……真热闹啊！",
        "ほほぉ~これがお嬢ちゃんの言っていたセールというやつか。\n確かににぎやかだね。": "哦哦～这就是小姑娘说的促销吗。\n确实很热闹。",
        "そういえば、こっちのデパートに来るのは初めてだ。\n普段もこんなに混んでいるのか？": "说起来，我还是第一次来这里的百货商店。\n平时也这么拥挤吗？",
        "今はちょうど歳末セールの時期ですから、デパートに来る人もいつもより多いです。\n私もあまりこのような場所に来ませんから、わくわくしますね。": "现在正好是年末促销期，所以来百货商店的人比平时多。\n我也很少来这种地方，感觉很兴奋。",
        "お正月に使うものを買いに来たのでしょう。\n行き交う方々のお顔は、新年への期待と喜びに満ちているように見えます。": "大家应该是来买过年要用的东西。\n来往行人的脸上，似乎都洋溢着对新年的期待与喜悦。",
        "人の幸せそうな姿を見ているだけで、自分まで楽しくなるものですね。": "光是看着别人幸福的样子，自己也会开心起来呢。",
        "確かに、あちこちきれいに飾られているから、見ているだけでも気分が上がる。": "确实，到处都装饰得很漂亮，光是看着心情就会变好。",
        "うちも新年を祝うけど、大晦日にみんなで集まって簡単に祝うだけだ。\nこっちみたいに派手に祝うことはできないわ。": "我们也会庆祝新年，但只是除夕大家聚在一起简单庆祝。\n没法像这里一样热闹地庆祝。",
        "んん、私も何か買いたくなってきた！\nどんなものが売っているの？": "嗯嗯，我也想买点东西了！\n这里都卖些什么？",
        "こちらのお正月商品コーナーにはお正月に必要なものや、\nお正月に食べるものが揃っていますね。": "这里的新年商品区有过年需要的东西，\n也有过年要吃的食物。",
        "見てください、シグマさん。正月飾りはこんなにたくさんあります。\n玄関先に置く門松に、玄関の軒下などに飾る玉飾り、綺麗な輪飾りにしめ縄……": "请看，西格玛小姐。新年装饰有这么多。\n门口摆放的门松、挂在门檐下的玉饰、漂亮的环饰，还有注连绳……",
        "シグマさん、どうしてそれを食べることばかり考えているのですか……": "西格玛小姐，您为什么总想着吃呢……",
        "まあ、確かに最後は食べます。一般的には12月28日に鏡餅を飾って、\n翌年の鏡開きの日に鏡餅を下げていただくのです。": "嗯，确实最后是要吃的。一般会在12月28日摆上镜饼，\n第二年镜开日取下镜饼食用。",
        "木槌で食べやすい大きさに割って、自分の好きなように召し上がります。\nおしるこにしてもいいですし、もち巾着にして、鍋に入れても美味しいですね！": "用木槌敲成方便食用的大小，按自己喜欢的方式享用。\n可以做成红豆年糕汤，也可以做成糯米福袋放进锅里，都很好吃！",
        "おおお！ちゃんとお餅があるじゃないか！\n最高だ！これ買いましょうよ！": "哦哦哦！这里真的有年糕！\n太棒了！买这个吧！",
        "はい！では、カートに入れますね。": "好的！那我放进购物车。",
        "えへへ、私は今まで伝統的な鏡餅しか見たことがありませんから、\nこういうのを見るのは初めてです。なんだか新鮮ですね。": "嘿嘿，我以前只见过传统镜饼，\n还是第一次看到这种，感觉很新鲜。",
        "でもお嬢ちゃんは追い込まれると、いざというときに牙を剝いてガブッ！\nと噛み付くタイプだね。それに比べて、そっちの方は……": "不过小姑娘被逼到绝境时，会在关键时刻露出獠牙狠狠咬下去！\n是这种类型。相比之下，那边的你……",
        "誰か話しかけてやらないと、悪いことを死ぬまでひとりで抱え込んじゃうかもしれない。\nほんと、どこまでも不器用な子だ。": "如果没人主动和她说话，她可能会把烦恼独自藏到死。\n真是个笨拙到极点的孩子。",
        "……えっと。\n私は誰かにガブッ！と噛み付くつもりはありませんが……": "……那个。\n我没有打算狠狠咬谁……",
        "………………はあ。": "………………唉。",
        "すみません。\nすこしよろしいでしょうか？": "抱歉。\n可以打扰一下吗？",
        "えっ？\n私……ですか？何でしょうか？": "诶？\n我……吗？有什么事？",
        "敵襲！敵襲だ！\n全員、戦闘用意！！！": "敌袭！敌袭！\n全员准备战斗！！！",
        "なんだ！？\nどこの敵だ！？": "什么！？\n哪里的敌人！？",
        "よくわからんが、\nあのオークランドさんもいるって…！": "虽然不太清楚，\n听说那位奥克兰小姐也在……！",
        "…なあに？\n外がうるさいの。": "……什么？\n外面好吵。",
        "「オークランド」って…": "“奥克兰”……",
        "よくわからないけど、\nここで油を売ってる場合じゃないみたいね。": "虽然不太清楚，\n但现在不是在这里闲逛的时候。",
        "…中のやつがもっとうるさいの！！！": "……里面那个家伙更吵！！！",
        "ムーバーの包囲網を突破した！": "突破搬运者包围网！",
        "しかし、奴らは諦める気がないようね。\n対応策はもちろん用意してあるのね、クイーン・エリザベス？": "但是，他们似乎没有放弃的打算。\n应对方案当然已经准备好了吧，伊丽莎白女王？",
        "ないわ。": "没有。",
        "…笑えない冗談を——": "……真是不好笑的玩笑——",
        "まあそう焦らないで。\nここからは、私の担当ではなくなるだけよ。": "别这么着急。\n从这里开始就不归我负责了。",
        "こちらエリザベス、任務完了よ。そろそろ主役交代の時間かしら？\n——敬愛なる指揮官様？": "这里是伊丽莎白，任务完成。差不多该换主角了吧？\n——敬爱的指挥官大人？",
        "……げ、激射！！！": "……激、激射！！！",
        "うああああああ——！！": "呜啊啊啊啊啊——！！",
        "あらあら、あんな少人数でムーバーの大部隊に攻勢を仕掛けるとは、\n実に勇猛果敢な姿ですね！": "哎呀呀，居然用这么少的人向搬运者大部队发起攻势，\n真是勇猛果敢！",
        "本気でその人数で大軍に挑むつもりなら、勇猛どころか、向こうみずとしか言えないわね。\nでも…あの様子だと、おそらく真の目的は他にあるのでしょう。": "如果真打算靠这点人挑战大军，那就不是勇猛，而是鲁莽。\n不过……看那个样子，她们真正的目的恐怕另有其事。",
        "うむ、どうすればいいでしょうか？\n人が戦っている時に、ただ見ているだけというも…": "嗯，该怎么办才好？\n别人正在战斗，我们却只在旁边看着……",
        "いいや、もう少し様子を見ておきましょう。軽率に動いたら、\nサンディエゴを救出するどころか、私たちまで捕まえるかもしれないし。": "不，再观察一会儿吧。贸然行动的话，\n别说救出圣地亚哥，连我们也可能被抓。",
        "我慢は苦手ですけど、\nボルチモアがそういうなら、頑張ってみますね。": "虽然我不擅长忍耐，\n但既然巴尔的摩这么说，我会努力的。",
        "後はご褒美として、\nちゃんと褒めてくださいね？": "之后作为奖励，\n要好好夸奖我哦？",
        "いつの間にか周りの人を自分のペースに飲み込んじゃうよね。\nサンディエゴ姉さんは。": "圣地亚哥姐姐总是不知不觉间把周围的人带进自己的节奏。",
        "…はは。\n本当に、昔のまんま。": "……哈哈。\n真的和以前一模一样。",
        "うん？よく聞こえなかったけど、褒めてるってことよね？\nよ～し！ならもうひと頑張りしなきゃね！": "嗯？虽然没听清，但你是在夸我吧？\n好～！那我得再加把劲了！",
        "窮地に立たされる時こそ、\n本当の激射戦士は、希望を諦めるわけにはいかんのだ！": "越是身处困境，\n真正的激射战士越不能放弃希望！",
        "この究極な激射で、空に穴を開けてやる——って、": "我要用这究极的激射，在天空开个洞——等等，",
        "あらら？": "哎呀？",
        "！！\nあれは…！": "！！\n那是……！",
        "…出番がなくなったようだね、残念。\n激射の道、障害が多くてしょうがないよ〜": "……看来没我们的出场机会了，真遗憾。\n激射之道障碍太多了，真没办法～",
        "まあもちろん、 もしなにか親切なお得意さんだけが対象の特別サービスがあるなら……\nえへへへへ～ ": "当然，如果有只面向亲切老主顾的特别服务……\n嘿嘿嘿嘿～ ",
        "それでも何かわからないことがあれば、いつでも私に聞いて頂戴。\nただし…": "如果还有什么不明白的，随时来问我。\n不过……",
        "ただし…？": "不过……？",
        "ギブアンドテイクとして、お嬢さんのアトリエやらを見学したいのよね～\n異世界の技術には私たちも非常に興味があるの。": "作为等价交换，我想参观一下小姐的工作室之类的地方～\n我们对异世界的技术也非常感兴趣。",
        "Hello～聞こえる？\nムーバーの駆逐艦ちゃん～": "Hello～听得到吗？\n搬运者的小驱逐舰～",
        "……悪いお姉ちゃんたち。": "……坏姐姐们。",
        "どうして、ラフィー、助けるの。": "为什么要帮助拉菲？",
        "今朝日たちはあなたと敵対するつもりは無いわ。": "我们朝日并没有与你为敌的打算。",
        "お時間がありましたら、 \n食堂でお茶の準備を手伝っていただけませんか？ ": "如果您有时间，\n能来食堂帮忙准备茶点吗？ ",
        "せっかく生徒たちがこんなにも情熱を見せているのです。 \n少し何かを飲んでからのほうが、 話し合いもうまく進むと思いますよ。 ": "学生们难得表现出这么大的热情。\\n我觉得先喝点东西，讨论也会进行得更顺利。 ",
        "本当にそれで大丈夫ですか……": "这样真的没问题吗……",
        "こそこそこそこそ……瑞鶴さんもう逃げる！ \nあんな怖いお姉さんの計画なんかに巻き込まれたら絶対——": "嘀嘀咕咕……瑞鹤小姐快逃！\\n要是被卷进那种可怕姐姐的计划，肯定——",
        "あぁ、 瑞鶴さん。 \nちょうど人手が足りないんです、 瑞鶴さんも手伝ってくれますね？ ": "啊，瑞鹤小姐。\\n正好人手不够，瑞鹤小姐也来帮忙吧？ ",
        "でもせっかく来てくれたことだし、 あがって頂戴。 ": "不过既然难得来了，请进吧。 ",
        "あれ？ ": "咦？ ",
        "とりあえず早く横になってください！ ": "总之请快躺下！ ",
        "そ、 そうね、 わたくしは確かにまだ病人だしね。 すみませんが、 お好きなところに掛けて頂戴ね。 \n（ ああ、 さっきのはちょっと無茶をしすぎたかも…… ）": "这、这么说也对，我确实还是病人。抱歉，请随意坐吧。\\n（啊，刚才可能有点太勉强了……）",
        "わかりました、 もう私のことで気を遣わないでください……\n医務室に電話をして、 誰かに来てもらいましょうか？ ": "知道了，不用再为我操心……\\n要不要给医务室打电话，让人过来？ ",
        "そんな大げさだわ。 \nたかが熱、 すこし寝ていれば治るわ。 ": "没那么严重。\\n只是发烧，睡一会儿就好了。 ",
        "本当を言うと、 このくらいなんともないわ。 わたくしの日常生活には、 何の支障もない。 \nただあなたたちに伝染したくないから、 こうして部屋におとなしくこもっているのよ。 ": "说实话，这点程度根本不算什么，对我的日常生活也没有影响。\\n只是我不想传染给你们，所以才老实待在房间里。 ",
        "心配いりません。 ただ先生の言う通りにすればいいんですよ。 \nふふふふ～": "不用担心，只要听老师的话就好。\\n呵呵呵呵～",
        "い、 いやだ！ あたしまだ若いの！ \n犯罪で刑務所行きなんていやよおおおお！ ！ ！ ": "不、不要！我还年轻！\\n我不要因为犯罪进监狱啊啊啊啊！！！ ",
        "\n\n その後、 反対運動に参加した生徒たちは、 \n 何者かによって送られた差し入れの飲み物を飲んだ後、 \n 全員意識不明の状態に陥り、 夕方になってやっと目を覚ました。 \n しかし、 その時にはすでに投票が終わっていた。 ": "\n\n之后，参加反对运动的学生喝下某人送来的慰问饮料后，\\n全员陷入昏迷，直到傍晚才醒来。\\n然而那时投票已经结束。 ",
        "\n\n 結果は同票。 \n 生徒たちはしばらくこの結果に戸惑ったが、 \n すぐ 「それでもこれはみんなで出した答えだ」 という声があがった。 \n この投票結果の意味を理解した生徒たちの中には、 \n もう反対の声を上げる人はいない。 ": "\n\n结果是平票。\\n学生们一度对这个结果感到困惑，\\n但很快有人说道：“即便如此，这也是大家共同给出的答案。”\\n理解了投票结果意义的学生中，\\n再也没有人发出反对声音。 ",
        "\n\n  「生徒たちの投票結果により両方開催する」 \n その結果を知ったジョージ5世教頭先生はその場でもう一度気絶したが、 \n すぐ立ち直り、 それを実現できる計画書の用意に取り掛かった。 ": "\n\n“根据学生投票结果，两项活动都举办。”\\n得知结果的乔治五世副校长老师当场再次晕倒，\\n但很快振作起来，开始准备能够实现这一结果的计划书。 ",
        "\n\n  「協力的なスポンサーたち」 の支援の下、 \n パールベイ学園に平和で愉快な年末が訪れた。 ": "\n\n在“热心合作的赞助商”支持下，\\n珍珠湾学院迎来了和平愉快的年末。 ",
        "\n 細かいことは一部省いていますが……ふふ、 \n それらのことは、 特に気にする必要がないと思います。 そうでしょう？ ": "\n细节部分省略了一些……呵呵，\\n我觉得那些事没必要特别在意。对吧？ ",
        "\n\n END 2\n 語られることのない物語": "\n\n END 2\n无人讲述的故事",
        "……わかりました。 でも、 くれぐれも無理をしないでください。 \n体調が悪化でもしたら、 すぐ私たちに言うのですよ。 ": "……知道了。不过请千万不要勉强自己。\\n如果身体状况恶化，要马上告诉我们。 ",
        "そんなはずがないでしょう。 \nあなたの心無い一言で倒れるほど、 わたくしは弱くないわ。 ": "怎么可能。\\n我还不至于因为你一句无心的话就倒下。 ",
        "それはさておき、 \nあなたがさきほど入ってからずっと抱えているあのタブレットはなに？ ": "先不说这个，\\n你从刚才进来后一直抱着的那台平板是什么？ ",
        "これは……すみません。 部屋で体を休ませている間、 もしかする退屈と感じる\nかもしれないと思って、 時間つぶしに風景写真をお持ちしましたが……": "这是……抱歉。我想着您在房间休息时可能会觉得无聊，\\n所以带了些风景照片来打发时间……",
        "おぉ！ それは風流だね、 まさにわたくしの品格にふさしいことだわ。 \nやはり品がある人ね。 早速見せて頂戴。 ": "哦！真有雅趣，确实符合我的品格。\\n你果然是个有品位的人。快给我看看。 ",
        "しかし、 ヴェネトさんは今体がとても弱っているように見えます。 \nこれ以上体力を使わせるのは……": "可是，维内托小姐现在看起来身体很虚弱。\\n再让您消耗体力……",
        "大丈夫と言ったでしょう。 安心しなさい。 それに見舞いに来てくれた人に持ってきた\n手土産をそのまま持ち帰らせるなんてこと、 わたくしにはできないわ。 ": "不是说没关系了吗，放心吧。而且我不能让来探望我的人\\n把带来的礼物原封不动地带回去。 ",
        "……わかりました。 \nでは、 どうぞ。 ": "……知道了。\\n那么，请看吧。 ",
        "これは素敵だわ。 \nどれも心をこめて選び出された秀作だと、 見れば分かるわ。 ": "真漂亮。\\n一看就知道每一张都是用心挑选出的佳作。 ",
        "この街の家屋、 どれも色鮮やかだね。 \nこれは加工で作り出したものなのか？ ": "这座城市的房屋都色彩鲜艳。\\n这是后期加工出来的吗？ ",
        "いいえ、 これはCepa Townの実際の景色です。 \nここに住む人たちはみんな、 自分の家にカラフルな色を塗っているのです。 ": "不，这是Cepa Town的真实景色。\\n住在这里的人都会把自己的房子涂成鲜艳的颜色。 ",
        "この写真に写っている砂浜もとてもきれいだわ。 \nあたりの植物から見ると、 南の街なのか？ ": "照片里的沙滩也很漂亮。\\n从周围的植物来看，是南方的城市吗？ ",
        "そうです。 そこはZenziberというところです。 \n景色がとてもきれいですし、 そこの人々もとても親切です。 ": "是的，那里叫Zenziber。\\n风景非常美丽，那里的人们也很亲切。 ",
        "ふーん……詳しいのね。 \nもしかして、 こっそり行ったことがあるの？ ": "嗯……你知道得很详细。\\n难道你偷偷去过那里？ ",
        "そ、 それは……！ \nそういうわけでは……": "这、这个……！\\n不是那样……",
        "なになに？ まさか何かある？ \n言ってみなさいよ。 ": "什么什么？难道有什么隐情？\\n说来听听。 ",
        "実は……私もどうしてなのか分かりませんが、 あそこに行ったことなかったのに、 \nなぜか最初からあそこを知っているような、 おぼろげなイメージが記憶にあって……": "其实……我也不知道为什么，明明从没去过那里，\\n记忆中却有一种仿佛从一开始就知道那里的模糊印象……",
        "ただ写真の見すぎで、 現実と混同してしまったかもしれません。 \nすみません、 こんなおかしな話を聞かせてしまって。 ": "也许只是看了太多照片，把它和现实混淆了。\\n抱歉，让您听了这么奇怪的故事。 ",
        "そうなの？ まあ、 わたくしからすれば、 \nフッドさんの記憶違いには聞こえないから、 信じるわ。 ": "是吗？在我看来，\\n这不像是胡德小姐记错了，所以我相信你。 ",
        "え？ ": "诶？ ",
        "ちょっと待ってください！ 私はおそらく確かにあそこに行ったことがありません。 \nですから、 先ほどの話はどうかあまり本気にしないでください。 ": "请等一下！我确实应该从没去过那里。\\n所以，刚才的话请不要太当真。 ",
        "なによ、 そんなに慌てて。 まさか、 さっきの話はわざとわたくしをからかうための嘘？ \n言い出した後引け目を感じて、 言い繕おうとしているの？ ": "怎么了，慌什么？难道刚才的话是为了故意逗我而撒的谎？\\n说出口后觉得心虚，所以想找借口圆回来？ ",
        "いいえ、 そんな、 \nもちろん嘘などでは……": "不，不是的，\\n当然不是谎话……",
        "それじゃあ問題ないでしょう。 \nあなたが教えたのはあくまでもあなたが知っていることでしょう。 ": "那就没问题了。\\n你告诉我的只是你所知道的事情。 ",
        "それが実際の状況と噛み合わないとしても、 あなたがすこし間違ってしまっただけだから。 \nそれくらい、 笑い飛ばせばいいことだわ。 ": "即使和实际情况对不上，也只是你稍微弄错了。\\n这种小事笑一笑就过去了。 ",
        "うぅ、 なんだかすみません……": "呜，总觉得很抱歉……",
        "気にしなくていいわ。 \nまあ、 これもわたくしは心が広いおかげよ、 ありがたく思いなさい。 ": "不用在意。\\n这也多亏我心胸宽广，好好感谢我吧。 ",
        "……はい、 誠にありがとうございます。 ": "……是，真的非常感谢。 ",
        "ふふ、 ヴェネトさんのような自信と元気の溢れる方が一体どんなダンスを見せてくれるのか、 \n今からとても楽しみです。 ": "呵呵，像维内托小姐这样充满自信与活力的人，究竟会展示怎样的舞蹈，\\n我现在就很期待。 ",
        "それはもちろん、 喝采の嵐と万雷の拍手を呼ぶ最高なパフォーマンスだわ。 ": "当然是能引来喝彩如潮、掌声雷动的最佳表演。 ",
        "そんなに待ちきれないのなら、 明日の午前中にあなたたちのところでお披露目でもしてあげようか。\n二ヶ月も待たせるのは忍びないからね。 いいお茶を用意しておきなさい。 ": "既然你这么等不及，明天上午要不要就在你们那里先展示一下？\\n让你等两个月实在过意不去。准备好上等的茶。 ",
        "それは……もう少し待ってからにしたほういいでは？ \nヴェネトさんの具合はまだ……": "那……是不是再等一会儿比较好？\\n维内托小姐的身体还……",
        "わたくしを誰だと思っている？ ヴィットリオ・ヴェネトよ。 \n今日のようなベッドに横になっていながら客と対話するしかない失態なんて一度で十分だわ。 ": "你以为我是谁？我是维托里奥·维内托。\\n像今天这样只能躺在床上和客人交谈的失态，一次就够了。 ",
        "この一晩中をかけて練習したダンスのステップも、 \nただ普通のお返しみたいなものよ！ そうよ！ ": "我练习了一整晚的舞步，\\n也不过是普通的回礼而已！没错！ ",
        "きゃああああああああ！ ！ ！": "呀啊啊啊啊啊啊啊！！！",
        "……っえ？ ": "……诶？ ",
        "おやおや、 そのようなはしたない奇声をあげるとは、 \n淑女として実に大きな失態ですね。 ": "哎呀呀，居然发出如此不雅的尖叫，\\n作为淑女真是巨大的失态。 ",
        "この悪女！ 絶対わざとでしょう！ ？ \nあの見るからに怪しい黒猫とグルだよね！ ？ ": "这个坏女人！绝对是故意的吧！？\\n你和那只一看就很可疑的黑猫是一伙的吧！？ ",
        "それはまたどうして？ そろそろ時間かと思い、 ヴェネトさんをお迎えに参りましただけですよ。 \n何かおかしなことでも起こったのですか？ ": "为什么这么说？我想着差不多到时间了，所以只是来接维内托小姐。\\n发生什么奇怪的事了吗？ ",
        "くっ……な、 なんでもないわ。 まったく余計なお世話。 このわたくしが迷子するとでも\n思っているの？ わざわざ迎えに来るなんて、 ふん、 何を企んでいるのか。 ": "可恶……没、没什么。真是多管闲事。你以为我会迷路吗？\\n居然特意来接我，哼，不知道在打什么主意。 ",
        "まさか。 わたしはただヴェネトさんが今日ダンスを披露すると知り、 前からダンスが得意と\n聞いておりますから、 せっかくの機会に共演をどうかとお誘いに参りましただけですよ。 ": "怎么会。我只是听说维内托小姐今天要表演舞蹈，\\n也早就听说您擅长跳舞，所以来邀请您趁此机会合作演出。 ",
        "いやよ！ ソロダンスの意味分かっているの？ それにあなたが踊っているとき、 いつもいきなり\n地面をすごい力で叩いたりしているでしょう？ そんなのこわ——品がないわ！ ": "不要！你明白什么叫单人舞蹈吗？而且你跳舞时总是突然\\n用很大力气踩地吧？那太可怕——太没品了！ ",
        "あのように床を踏み鳴らせば、 観客を圧倒できるわ。 \n試しにやってみませんか？ ": "那样踩响地板，就能震撼观众。\\n要不要试试看？ ",
        "観客を脅してどうするのよ！ この野蛮人！ \nせっかくの優雅な芸術が、 あなたにされると路上の喧嘩みたいになっているじゃないか！ ": "吓唬观众算什么！你这个野蛮人！\\n难得的优雅艺术，被你弄得像街头斗殴一样！ ",
        "ふん、 あなたと話しても無駄でしょうね。 そこでハンカチでも銜えて、 真の踊りとはなんなのか、 \nよく見るがいい。 まああなたにとって、 見るだけでは理解できないと思うがね。 ": "哼，和你说也没用。你就叼着手帕在那里，好好看看什么是真正的舞蹈。\\n不过对你来说，光看恐怕也理解不了。 ",
        "あぁ、 そういえば、 お菓子を持ってくるつもりでした。 ": "啊，说起来，我本来打算带些点心来。 ",
        "\nナッツを倍くらい多めに入れてもらうのはどうかしら？ ": "\n让他们多放一倍坚果怎么样？",
        "三倍入れて！ ": "放三倍！ ",
        "っはあ！ ？ ": "哈！？ ",
        "この卑怯者！ \n覚えときなさい！ ": "你这个卑鄙小人！\\n给我记住！ ",
        "そのような貧相な頭脳では覚えきれないかもしれないが、 \n少なくとも自分にはあれこれ言う資格がないことが分かるようになるでしょう。 ": "以你那贫乏的头脑，可能记不住这些，不过至少你会明白自己没有资格对别人指手画脚。 ",
        "わたくしはヴィットリオ・ヴェネト、 \n故郷の豊かな海を守るカンピオーネ、 恵まれし二つの島を護衛する英傑。 ": "我是维托里奥·维内托，\\n守护故乡富饶海域的冠军，保卫两座幸运岛屿的英杰。 ",
        "この名こそ勝利を追い求める栄光であり、 使命である。 ": "这个名字代表追求胜利的荣耀与使命。 ",
        "正統なるこの座を挑む者が現れたというのなら、 \nわたくしは喜んで身をもって天命を果たし、 この手で再び勝利の冠を収めよう。 ": "既然有人胆敢挑战这正统之座，\\n我就欣然亲自履行天命，用这双手再次夺回胜利的桂冠。 ",
        "あなたはわたくしに、 世間に認められる重要性を訴えかけているというのなら、 \nわたくしはその世間に認めさせて見せる。 ": "既然你向我强调得到世人认可的重要性，\\n我就让世人认可给你看。 ",
        "あなたはわたくしに、 意地を張る意味を問うのなら、 \nわたくしはあなたに勇気の力を証明してやる。 ": "既然你问我坚持己见有何意义，\\n我就向你证明勇气的力量。 ",
        "パールベイ学園は世界屈指の名門校です。 世界各地から集めてきた生徒たちはここで\n友になり、 学を修め、 そして未来各国を支える人材になるのです。 ": "珍珠湾学院是世界顶尖名校。来自世界各地的学生在这里\\n成为朋友、修习学业，最终成长为支撑各国未来的人才。 ",
        "このような学園を司る者として、 どれほどの名望と人脈、 \nそしてどれほどの妬みと争いを招くのか、 想像に難くないでしょう。 ": "作为管理这样一所学院的人，会拥有多少声望与人脉，\\n又会招来多少嫉妒与争斗，不难想象吧。 ",
        "……あの方の就職はただの 「予想外」 です。 \nその座を奪い合う多方面の勢力が妥協した臨時案、 必ずまもなく退職させられてしまう一時の幻。 ": "……那位的就任只能说是“意料之外”。\\n这是争夺那个位置的各方势力妥协出的临时方案，迟早会被迫离职的短暂幻影。 ",
        "\n\nさあ、 今は道を開けなさい！ \nあなたとの口論よりもっと重要なことがわたくし待っているの！ ": "\n\n好了，现在给我让路！\\n有比和你争论更重要的事在等着我！ ",
        "ゴホッゴホッ！ なっ、 どういうこと！ ？ \nこのケーブルはなに！ ？ ": "咳、咳！什、什么情况！？\\n这条缆线是什么！？ ",
        "ポーズが非常に変な女性": "姿势非常奇怪的女性",
        "完璧だ。 魔女よ、 あなたが選んだ実験体はまさに完璧だ。 \n拍手を！ ": "完美。魔女啊，你选中的实验体简直完美。\\n鼓掌！ ",
        "は、 はい！ \n（ ブチブチ～ ）": "是、是的！\\n（撕拉撕拉～）",
        "ちょっと待ちなさい！ あの袖を隔てながらした中途半端な拍手へのツッコミはおいといて、 \nまさか先ほどの無礼をこのままごまかせるとは思っていないでしょうね！ ？ ": "等一下！先不说隔着袖子敷衍鼓掌这件事，\\n你不会以为刚才的无礼就这样能蒙混过去吧！？ ",
        "……これを。 ": "……给你这个。 ",
        "これは……予算申請のリスト？ \nしかもとても詳しい……難癖をつけたくても難しいくらいの。 ": "这是……预算申请清单？\\n而且非常详细……详细到想挑毛病都难。 ",
        "……わたくしは何をしに来たのか、 言っていたかしら？ ": "……我来这里是做什么的，你刚才说过吗？ ",
        "ふ、 ふん！ あなたたち、 運がよかったわね。 \nわたくし、 今日は忙しいの。 後で覚えときなさい。 ": "哼、哼！你们运气不错。\\n我今天很忙，之后可别忘了这件事。 ",
        "ちょっと待って！ よく見るともう朝じゃない！ ？ \nもう何時になっているの！ ？ ": "等一下！仔细一看，天已经亮了！？\\n现在到底几点了！？ ",
        "午前十時くらいです……": "大概上午十点……",
        "あああ！ このままじゃ遅れちゃう！ ": "啊啊啊！这样下去要迟到了！ ",
        "\n覚えといてね！ ": "\n记住了哦！ ",
        "……これで、 あなたが必要とする要素がすべて揃えた。 ": "……这样，你所需要的要素就全部齐了。 ",
        "ふん、 出発の時刻まで教えていないとは、 感激するほど親切だな。 ": "哼，连出发时间都不告诉我，真是体贴得令人感动。 ",
        "どういたしまして。 ": "不客气。 ",
        "……実験の結果は、 本当にそんなに完璧なの？ ": "……实验结果真的有那么完美吗？ ",
        "そうだ。 一切の揺るぎもない純粋な意志、 そしてそれがもたらした干渉係数を考えに入れる必要\nすらない明確な記録。 それらを提供できるのは、 根っからのバカだけだ。 ": "没错。毫不动摇的纯粹意志，以及由此产生的无需考虑干涉系数\\n也能得到的明确记录。能提供这些的，只有天生的傻瓜。 ",
        "一般論として、  「バカ」 は人を褒めるときに用いられるような言葉ではない。 ": "一般来说，“傻瓜”不是用来夸人的词。 ",
        "だから今年のこの学園長が嫌がらせで外されている状況で、 \n 「おとぎ話」 の執筆役が赤城先輩に変わっても全く問題ない。 なるほどなるほど——": "所以在今年学院长因恶作剧被排除在外的情况下，\\n由赤城前辈改任“童话”的撰写人也完全没问题。原来如此原来如此——",
        "ふん、 研究者にとって、 \nただ聞きなれた呼び名に過ぎないさ。 ": "哼，对研究者来说，\\n这不过是个听惯了的称呼。 ",
        "……すでに存在している認知とかけ離れていることに没頭し、 終着点で自分を待っているのは\n何もない空虚かどうかすら知るようがない。 確かに、  「賢明」 とは言えない。 ": "……沉浸在远离既有认知的事物中，甚至不知道终点等待自己的是否是一片虚无。\\n确实称不上“明智”。 ",
        "しかし踊るピエロとそれをあざ笑う観客、 \nそれぞれの笑みに宿っている意志の強さはどちらが上か、 言うまでもない。 ": "然而，跳舞的小丑与嘲笑他的观众，\\n哪一方笑容中蕴含的意志更强，不言而喻。 ",
        "では、 そろそろ失礼する。 \n雑役、 遠出の荷物をまとめておけ。 ": "那么，我差不多该告辞了。\\n杂役，把远行的行李整理好。 ",
        "ええっ！ ！ ？ ？ そんないきなり！ ？ \nどこへいくのですか？ ": "诶！！？？这么突然！？\\n要去哪里？ ",
        "北だ。 ": "北方。 ",
        "はぁ、 また死ぬほど忙しかった一日……": "唉，又是忙得要死的一天……",
        "まったくもう、 どいつもこいつも、 もっと長い目で、 自分の未来にもっと責任を感じてもらえないの？\nいつもわがままで、 めちゃくちゃなことをして、 私の予定を狂わせて——": "真是的，一个个的，不能把目光放长远些，对自己的未来多负责一点吗？\n总是任性妄为，把事情搞得一团糟，打乱我的计划——",
        "ゴホゴホ！ \nお邪魔します。 ": "咳咳！\n打扰了。 ",
        "ノックもせずに勝手に入ってくるとは、 どれだけ礼儀知らずなの？ \nよくも毎日上流気取りしているのね。 ": "不敲门就擅自进来，你到底有多没礼貌？\n居然每天还摆出一副上流人士的样子。 ",
        "ふん、 言葉遣いには注意した方がいいわ。 \nこれを見た後、 まだそんな偉そうな態度でいられるの？ ": "哼，说话最好注意一点。\\n看过这个之后，你还能保持那副傲慢态度吗？ ",
        "……これは、 春のイベントの予算申請書？ ": "……这是春季活动的预算申请书？ ",
        "そうだゴホッ！ どう？ \nこれであなたの悩みはもう解決でしょう？ ": "没错，咳咳！怎么样？\\n这样你的烦恼就解决了吧？ ",
        "…………………………はぁ。 ": "…………………………唉。 ",
        "な、 なによ？ その呆れたような顔……たしかに一部のリストには\nちょっと問題があるかもしれないけど、 時間はたったの二日だよ。 ": "怎、怎么了？干嘛摆出那副无语的表情……确实有些清单\\n可能有点问题，但时间只有短短两天啊。 ",
        "問題はそこではありません。 \n姉妹なのに、 どうしてそこまで情報共有がなっていないの？ ": "问题不在这里。\\n明明是姐妹，为什么连情报共享都做得这么差？ ",
        "はあ？ \nいったい何を言っているのよ。 ": "哈？\\n你到底在说什么。 ",
        "今日の午後、 リットリオはすでに何人かを連れて、 \n山ほどある書類を運んできましたよ。 ": "今天下午，利托里奥已经带着几个人，\\n把堆积如山的文件送来了。 ",
        "私に彼女のあの 「迎春合同祭り」 企画を通してくれとしつこく言うから、 \n仕方なくもう許可を出しました。 ": "她一直缠着我，让我通过她那个“迎春联合祭典”企划，\\n所以我只好批准了。 ",
        "それは確かに、 単独で各グループの穏健派と交渉した方が効率的ですし……\nばらばらの催しをまとめて一緒に管理統括した方も、 確かによりやりやすいですし……": "确实，单独和各个团体的温和派谈判更有效率……\n把分散的活动汇总后统一管理，也确实更容易……",
        "…あのコードネーム、オークランドは結構気に入っているかもしれませんね。": "……奥克兰可能还挺喜欢那个代号的。",
        "あっ！わかるわかる。\nなんか発音がかっこいいよね〜": "啊！我懂我懂。\n发音听起来很帅嘛～",
        "ちょっと待って！ つまりもともと責任を投げ出して逃げたと思われていたリオが、 \nいきなり出てきて、 そしてもう全部片をつけたってこと？ ゴホン。 ": "等一下！也就是说，原本被认为甩下责任逃跑的里奥，\\n突然出现，然后已经把一切都解决了？咳。 ",
        "それは片付けたうちには入らない！ いいえ、 確かにこのままでもうまく遂行できますが……\nしかしあの人は必要な手順を飛ばしたの！ それは紛れのない規則違反の行動です！ ": "那不算解决问题！不，确实照这样也能顺利执行……\n但那个人跳过了必要步骤！那是毫无疑问的违规行为！ ",
        "……つまり、 申請書類も実は全部まとめた？ \nわたくしがしたのは全部……無駄なこと？ ": "……也就是说，申请文件其实也全都整理好了？\\n我做的一切……都是白费？ ",
        "バタン——": "砰——",
        "ちょっ！ ？ どうしたのあなた？ \nちょっと、 ちょっと！ 私の声聞こえる？ ": "等一下！？你怎么了？\\n喂，喂！听得到我的声音吗？ ",
        "医務室に電話、 電話っと……ああもう！ \nどいつもこいつも世話の焼けるやつだから！ ": "给医务室打电话，电话……真是的！\\n一个个都这么让人操心！ ",
        "待って、指揮権は我々にあるはず。\nオークランド、彼女の指示にすんなりと従わないで。": "等等，指挥权应该在我们手里。\n奥克兰，别轻易听从她的指示。",
        "なんか親しみを感じるからね。以前は怖い人だって思ってたけど、今は…\nうーん、ちょっとダメなお姉さん、みたいな感じ？": "因为总觉得她很亲切。以前我以为她是个可怕的人，但现在……\n嗯，感觉像是有点不靠谱的姐姐？",
        "ちょ、ちょっとダメな…！？": "有、有点不靠谱……！？",
        "ああごめんごめん、さっきのは…いい意味だから！\nそう、親しみやすいって感じ！": "啊，抱歉抱歉，刚才那是……褒义！\n对，就是很容易亲近的感觉！",
        "そ、そうですか…？": "是、是这样吗……？",
        "…正直いうと、私も…ちょっと意外です。\nあなたはもっと…": "……说实话，我也觉得……有点意外。\n你应该更加……",
        "…いいえ、なんでもありません。": "……不，没什么。",
        "オークランド、人を信じすぎ…": "奥克兰，你太容易相信别人了……",
        "…帰投しょう。\nエリザベスたちはすでに待っているかもしれない。": "……返航吧。\n伊丽莎白她们可能已经在等了。",
        "…僕の知っていることは、これくらいだ。": "……我知道的就这些。",
        "なるほど…\nあなたが言ったことを聞く限り、実に酷似しているね…": "原来如此……\n照你所说的来看，确实非常相似……",
        "「フェニー」が現れるまで、\nほとんど一致しているとも言えるわ。": "在“菲妮”出现之前，\n几乎可以说完全一致。",
        "かもな。僕はそんなことにあまり興味がないんだ。\n…その辺のことは、フッドの方が詳しい。": "也许吧。我对那种事没什么兴趣。\n……那方面的事，胡德更清楚。",
        "そう…\nでは、後であの方にも聞いてみましょう。": "这样啊……\n那之后也去问问那位吧。",
        "好きにするがいい。": "随你便。",
        "へっ！": "哼！",
        "エリザベス！\n帰ってきたよ〜！": "伊丽莎白！\n我们回来啦～！",
        "よくやりました！\nさすがはエリザベスさんですね。": "做得好！\n不愧是伊丽莎白小姐。",
        "そちらの装束からすると桐桜の出身とお見受けする方も、ムーバーの方も、\n見事な戦いぶりでした！": "从您的服装来看，您似乎来自桐樱；搬运者小姐也是，\n两位的战斗表现都非常精彩！",
        "偶然が重なった結果とは言え、\n皆さんとともに戦えることは、とても光栄に思います！": "虽说是种种偶然叠加的结果，\n但能与各位并肩作战，我感到非常荣幸！",
        "あら、やっと帰ってきたのね。\n随分待ったわ。": "哎呀，终于回来了。\n我等了很久。",
        "すまない。\nムーバー…MCCのパトロール隊に遭遇したせいで、足止めを食らってしまった。": "抱歉。\n因为遇到了搬运者……MCC的巡逻队，被耽搁了一阵。",
        "はあ！ ？ \n悪い、 少しぼーっとしてて、 息するのを忘れた。 ": "哈！？\n抱歉，我刚才有点发呆，忘了呼吸。 ",
        "あまり深く考えないでください。 ハウお嬢様の作ったものですから、 大体そんな感じです。 \n本気にすると付き合いきれませんよ。 ": "请别想得太深。毕竟是豪小姐做的东西，大概就是这样。\n要是认真对待，可没法陪您玩下去。 ",
        "塩っちってばひどい～このハッピーエンドを作るためにいろいろと苦労したのよ？ \nもっと喜んでくれてもいいじゃないですか？ いっちみたいに。 ": "盐盐真过分～为了做出这个美好结局，我可是费了不少功夫哦？\n你也可以更开心一点嘛？像一池那样。 ",
        "おー！ 悪い奴らは結局何もできなかったし、 \nヒロインもすごい肩書もらったし、 最高じゃん！ ": "哦～！坏人最后什么也没做成，\n女主角还得到了厉害的头衔，太棒了！ ",
        "そうなのか？ 言われてみると、 確かにそうかもしれない……\nじゃあやはり、 いいエンディングなのか？ ": "是吗？听你这么一说，确实可能是这样……\n那果然是个好结局吗？",
        "しっかりしてくださいよ瑞鶴さま！ いつもマイペースのあなたが\nこういう時こそ、 一番自分の意見を持たなきゃダメでしょう！ ": "振作一点，瑞鹤大人！平时总是我行我素的您，\n这种时候才更应该坚持自己的意见吧！ ",
        "！ 確かに。 このストーリーには非合理的なところがあるし、 \nこの終わり方も、 真実味に欠ける。 ": "！确实。这段故事有不合理之处，\n这个结局也缺乏真实感。 ",
        "……まあ、 でも、 いいんじゃないか。 ": "……不过，应该也不错吧。 ",
        "ふふ、 ありがとうございます。 \nあたしうっれしい～": "呵呵，谢谢。\n人家好开心～",
        "ほかにもいくつかのエンディングがあるけど、 どうですか？ \n最初からもう一度やり直して、 他の選択肢を選んでみてはいかが？ ": "还有其他几个结局，要不要试试？\n从头再来一次，选择其他选项怎么样？ ",
        "うん、 すこし考えさせてくれ。 ": "嗯，让我稍微想想。 ",
        "うぅ、 頭がふらふらする……いったい何があったの？ ": "呜，头好晕……到底发生了什么？ ",
        "……なに？ \n遠慮なしに人のフルネームを呼ぶなんて、 態度だけが偉いね。 ": "……什么？\n毫不客气地直呼别人全名，架子倒是不小。 ",
        "\n\nあなたは今までずっと、 一族の長女と自称している。 \nそれはなぜ？ ": "\n\n你一直自称是家族长女。\n为什么？ ",
        "愚問ね。 \nそれが事実だからよ。 ": "真是愚蠢的问题。\n因为那就是事实。 ",
        "\n\nしかしすべての正式文書の記録において、 \nあなたたち姉妹のことは 「リットリオ級」 に分類されている。 ": "\n\n然而在所有正式文件的记录中，\n你们姐妹都被归类为“利托里奥级”。 ",
        "それはただ書類が間違っているだけだわ。 オフィスワークの文官は早く家に帰って\n猫に餌をやりたいからって、 適当なことを書いちゃった、 なんてことはよくあるでしょう？ ": "那只是文件写错了而已。办公室文官为了早点回家\n给猫喂食，随手乱写一通，这种事不是很常见吗？ ",
        "\n\nだがあなたの主張もあれらの記録と同じ、 ただ 「片方の言い分」 に過ぎない。 \n違うのは大多数の人はあなた一人の話より、 \n文書に記されたことの方に信憑性を感じる。 ": "\n\n但你的主张和那些记录一样，也不过是“一面之词”。\n不同的是，比起你一个人的说法，大多数人更相信\n文件上的记载。 ",
        "\n\nなぜ、 間違っているのはあなた自身とは思わない？ ": "\n\n为什么不认为错的是你自己？",
        "なによ？ バカが群れて人一倍の声で騒いでいるから、 わたくしの間違いになるわけ？ \nふざけないで頂戴。 ": "什么？因为一群笨蛋用比别人更大的声音吵闹，\n所以错的就是我？别开玩笑了。 ",
        "\n\n口先だけの根拠のない誹謗中傷でしかないなら、 \nあなたの憤慨も至極当然のもの。 ": "\n\n如果那只是毫无根据的口头诽谤，\n你的愤怒也完全合情合理。 ",
        "\n\nしかしあなたには 「根拠」 がない。 ": "\n\n然而你没有“依据”。",
        "はあ？ 何よそのへ理屈。 \nわたくしの話は信じられなくて、 あの紙屑は信じられるの？ ": "哈？这是什么歪理。\n你不相信我的话，却相信那些废纸？ ",
        "\n\nある意味では、 その通りだ。 ": "\n\n从某种意义上说，确实如此。 ",
        "\n\n普通の幼子は生まれてから長幼の序に認識を持つはずがない。 \nそういった概念はすべて子供が成長するにつれて、\n周囲の者に教えられたのだ。": "\n\n普通的孩子出生时不可能就懂得长幼尊卑。\n这些概念都是孩子成长过程中，\n由周围的人教给他们的。 ",
        "\n\nこの点において、 あなたたちは普通の幼子と何の区別もない。 ": "\n\n在这一点上，你们和普通孩子没有任何区别。 ",
        "\n\nあなたたちに関する大多数のことは、 生まれた瞬間から決められている。 \nあれらの文書はただ、 こういった 「決められた」 ことを人に伝達しているに過ぎない。 ": "\n\n关于你们的大多数事情，从出生那一刻起就已经决定了。\n那些文件只是在向他人传达这些“已决定”的事实。 ",
        "\n\nあなたはいったいどんな情報源を信じて、 \nそれよりも遥かに受けられている 「一般的な認識」 を否定しているの？ ": "\n\n你究竟相信什么信息来源，\n竟要否定远比它更受认可的“普遍认知”？",
        "あんな回りくどい言い方しておいて、 \n結局ただ定番の 「どうしてわたくしは自分こそ長女だと言い切れるの」 か？ ": "说了那么拐弯抹角的话，\n结果还是那个老问题：“为什么我能断言自己就是长女”？ ",
        "いいか、 このわたくしは他の人が何を言っても信じてしまうような無知な幼子などではない。 \n自分こそが長女だと、 はっきりと知っているわ。 ": "听好了，我可不是不管别人说什么都会相信的无知小孩。\n我清楚地知道，自己就是长女。 ",
        "\n\nあのようなあなたに限定している認知は、 ただの妄想ではないか？ \n他の者はそう思わない。 あなただけ、 周囲の者から逸脱している。 ": "\n\n你这种只属于自己的认知，难道不只是妄想吗？\n其他人都不这么认为。只有你偏离了周围人的认知。 ",
        "この程度の戯れ言を一々気にしていると、 \n毎晩枕を濡らすまで泣きまくることになっているわ。 いい加減にしなさい。 ": "如果连这种程度的戏言都要在意，\n你就会每晚哭到泪湿枕头。适可而止吧。 ",
        "何も分かっていないようだから、 特別にはっきりと教えてあげるわ。 ": "既然你似乎什么都不明白，我就特别明确地告诉你。 ",
        "あの頭の高い野蛮女も 「ないよりまし」 と分かるでしょうし、 もうすぐ締め切りのこの時に、 \nわざわざけちをつけるようなことはしないでしょう。 ": "那个傲慢的野蛮女人也知道“总比没有好”，在马上截止的现在，\n应该不会特意挑三拣四。 ",
        "……にしても、 今回をきっかけにこの基地を一回りしてみたら、 変人が多すぎないか？ \nたとえば——": "……话说回来，借这次机会在基地转了一圈，怪人是不是太多了？\n比如——",
        "え～？ イベントするの～？ それともしないの～？ 申請書を出すためじゃないなら、 \n急いで決めないとだめなことでもない、 だよね～？ ": "诶～？要办活动吗～？还是不办呢～？如果不是为了提交申请书，\n也不是必须马上决定的事吧～？ ",
        "ヴェネトお姉様！ そんなことより、 どうやってヴェネトお姉様みたいに\n優雅でセクシーになれるのか、 教えてもらえませんか！ ？ やはり太ももあたりが肝心？ ": "维内托姐姐！比起那些，能不能教教我怎样才能像维内托姐姐一样\n优雅又性感！？果然大腿才是关键？ ",
        "平海離してよ！ \nこのいきなり 「キャラかぶり」 とか言い出すやつに礼儀というものを思い知らせてやる！ ": "平海，放开我！\n我要让这个突然说什么“角色重叠”的家伙明白什么叫礼貌！ ",
        "ヴェネト殿！ 早く頭を低くしてください！ ": "维内托殿下！请快低下头！ ",
        "今研究会で——": "我现在在研究会——",
        "っひ！ 今思い出してもまで冷や汗が止まらない……": "咿！现在回想起来还是冷汗直流……",
        "まったく、 それもこれも\n統括する者がちゃんと仕事を出来ていないせいだわ。 あんなふざけた輩を放置するなんて。 ": "真是的，这一切都是因为负责统筹的人没有好好工作。\n居然放任那种胡闹的家伙不管。 ",
        "はぁ、 もうこんな時間。 残りは明日いち早くするしかないみたいね。 \n一応まだ何が残っているのかチェック——": "唉，已经这么晚了。剩下的看来只能明天尽早完成。\n先检查一下还有什么没做完——",
        "あれ？ この 「運命と英知」 、 二十四時間営業って書いてある？ \n責任者は……デューク・オブ・ヨーク？ ": "咦？这个“命运与睿智”写着二十四小时营业？\n负责人是……约克公爵？ ",
        "確か、 占いのおばあちゃんみたいなことをやっている人だったね。 まったく、 \nこんな子供騙しのものにこんなにいっぱいのコメントがあるとは、 ここの人はセンスがないね。 ": "我记得她好像在做类似算命婆婆的事。真是的，\n这种骗小孩的东西居然有这么多评价，这里的人真没品味。 ",
        "まあ、 そんなことはどうでもいいわ。 \n二十四時間営業なら、 今から向かっても構わないよね。 ": "算了，那些都无所谓。\n既然二十四小时营业，现在过去也没关系吧。 ",
        "ヴィットリオ・ヴェネトのご来訪よ！ \n扉を開けなさい。 ": "维托里奥·维内托来访！\n开门。 ",
        "聞き覚えのない女性の声": "陌生女性的声音",
        "待っていた。 入れ。 ": "等你很久了。进来。 ",
        "もう役になっているの？ \n思っていたより職人精神があるね。 ": "已经进入角色了吗？\n没想到你还挺有匠人精神。 ",
        "どんなことを生業にしても、 仕事に真面目な人には高く評価するわ。 \n遅い時間に失礼するよ。 ": "无论从事什么工作，我都很欣赏认真工作的人。\n这么晚来打扰了。 ",
        "変なポーズを取る女性": "摆出奇怪姿势的女性",
        "ふん、 やはり一秒の狂いもないか。 \nあの魔女、 確かになかなかの腕前だ。 ": "哼，果然一秒都不差。\n那个魔女，确实相当有本事。 ",
        "あれ？ あなたはデューク・オブ・ヨーク……ではないよね？ \nすこし癖のある金色のロングヘヤのはず——": "咦？你不是约克公爵……吧？\n她应该有一头很有特色的金色长发——",
        "これは魔法ではない。 \nもちろんあなたがその呼び方を変えることもない。 ": "这不是魔法。\n当然，也不是你改变称呼的结果。 ",
        "ヒー！ ！ \nいきなり背後から出てきて出口を塞ぐとはどういうつもり！ ？ ": "咿！！\n突然从背后出现，还堵住出口，你想干什么！？ ",
        "ことが起こる前に暇を持て余しているから、 \n散歩に出かけて、時間通りに帰ってきただけ。 ": "事情发生前我闲得没事做，\n所以出去散步，只是按时回来了。 ",
        "わわわわたくしはこんなインチキに惑わされないわ！ \nどうせああいう事前に手配しておいた演出みたいなものでしょう！ ？ ": "我我我我才不会被这种骗术迷惑！\n反正就是事先安排好的表演吧！？ ",
        "あれは…！": "那是……！",
        "信号弾だ！\nオークランドたちが成功した！": "是信号弹！\n奥克兰她们成功了！",
        "よかった…": "太好了……",
        "——ちっ、グズグズしやがって。": "——啧，磨磨蹭蹭的。",
        "オークランドたちは成功したようね。こちらもさっさと退散しましょう。\n弾薬も底尽きたし、私たちが捕まったら本末転倒だわ。": "奥克兰她们似乎成功了。我们也赶紧撤退吧。\n弹药也用光了，要是我们被抓就本末倒置了。 ",
        "言わなくてもわかってる。\nだけどな…！": "不用你说我也知道。\n但是……！",
        "こっちにも敵が現れたの！？\nこれじゃさすがにまずいわね…": "这边也出现敌人了！？\n这样下去可不妙……",
        "その言葉、待ってました！": "我等的就是这句话！",
        "うわああ！？": "呜哇啊啊！？",
        "どうでした？同胞が危機に瀕する時に救いの手を差し伸べるわたくしの姿！\n称賛に値しますよね。": "怎么样？在同胞陷入危机时伸出援手的我！\n值得称赞吧。 ",
        "ここで姿を見せるべきか迷っていたが…\n確かに高みの見物をする場合じゃなさそうだわ。": "我本来还在犹豫是否该现身……\n现在确实不是袖手旁观的时候。 ",
        "エセックス、絶好なタイミングだ。\nよくできました。": "埃塞克斯，时机正好。\n做得很好。 ",
        "ふふ、ありがとう〜\nやはり褒められると、ぐっとやる気が出ますね。": "呵呵，谢谢～\n果然一被夸奖，干劲就一下子上来了。 ",
        "あなたたちは…": "你们是……",
        "…一応心当たりがあります。\n一つ一つ回るつもりです。": "……我大致有些头绪。\n打算一个个去确认。 ",
        "ついに追いついたのね。あれでしょう？\nいたいけな少女から艤装を奪った悪いやつめ～": "终于追上来了。就是那个吧？\n从惹人怜爱的少女手里夺走舰装的坏家伙～",
        "ほっほ～……拘束を実行しているあの二隻も行動できないらしいね。": "哦哦～……看来那两艘正在执行拘束的船也无法行动。",
        "でもあの<color=#f14966>【警戒範囲】</color>には近づかないでね。\n危険な雰囲気がするわ。": "不过别靠近那个<color=#f14966>【警戒范围】</color>哦。\n感觉很危险。",
        "まずは<color=#f14966>【警戒範囲】</color>の外からあの二体をやっつけたほうが妥当よ。": "首先从<color=#f14966>【警戒范围】</color>外解决那两个比较妥当。",
        "うんうん、 終わりよければ全てよし、 だっけ？ ": "嗯嗯，结果好一切都好，是这么说的吧？ ",
        "私の理性が 「おかしいでしょう！ 」 って叫んでるけど、 \nもうツッコミの気力がありません……": "我的理智一直在喊“这不对吧！”，\n但我已经没有吐槽的力气了……",
        "瑞鶴さま、 さあ、 深呼吸してください。 そしてスーッと吐いて。 \nリラックスです。 もう全部終わりましたよ。 ": "瑞鹤大人，来，深呼吸。然后慢慢呼气。\n放松，已经全部结束了。 ",
        "わあ！？": "哇！？",
        "いらっしゃい、 ヴェネトさん。 \nこちらにいらしてくださるのはとても嬉しいです。 ": "欢迎您，维内托小姐。\n您愿意来这里，我非常高兴。",
        "あなたとお話が出来るのは、 わたくしも気分がいいわ。 フッドさん。 \nあなたたちの中に、 淑女の名にふさわしいのはあなただけだもの。 ": "能和您交谈，我也很开心，胡德小姐。\n你们之中，只有您配得上淑女之名。",
        "他のお二人も、 お会いできて嬉しいわ。 ": "见到另外两位，我也很高兴。",
        "はじめまして、 ヴェネト様。 \nヴェネト様とお会いできて、 今日は素敵な一日になりそうです。 ": "初次见面，维内托大人。\n今天能见到维内托大人，感觉会成为美好的一天。",
        "……どうも。 ": "……你好。",
        "お、 お……": "哦、哦……",
        "モブB": "路人B",
        "ちょっと待って！ ？ \nなに、 その見るからに暴力的な武器は！ ？ ": "等一下！？\n那把一看就很暴力的武器是什么！？",
        "そう？ 気のせいよ。 最近の天気がこんなにいいのに、 急に病気にかかれたりするのは\n自分の健康管理すらまともに出来ない輩だけだわ。 ": "是吗？是你多心了。最近天气这么好，居然还会突然生病，\n只有连自己的健康都管理不好的人才会这样。",
        "そうは言いますが、 ちょうど季節の変わり目ですから、 \nやはりちゃんと体調の変化に気を配った方がいいかと思います。 ": "话虽如此，现在正值换季，\n我还是觉得应该好好留意身体状况的变化。",
        "こっちの交戦規定がこうだよ！ ": "我们的交战规定就是这样！",
        "うわあぁぁぁああ！ ！ ！ ": "呜哇啊啊啊啊！！",
        "\n\n ……銀河を跨る壮絶な戦争は、 これにて幕を開けた。 \n 正しさも過ちも滅びる恒星の光に照らされ、 燃やされ尽くした。 \n 無限に広がる暗闇の中にあるのは、 勝利のみ。 ": "\n\n……横跨银河的壮烈战争，就此拉开帷幕。\n正义与错误都在恒星毁灭的光芒下被照亮、燃尽。\n无限延伸的黑暗之中，唯有胜利。",
        "\n\n 新感覚SF黙示録RPG 『プロジェクト未定』 \n 鋭意早期準備中（開発日程未定）。 ": "\n\n全新感觉科幻启示录RPG《项目未定》\n正在积极准备早期开发（开发日程未定）。",
        "\n こちらをクリックして、 \n 公式サイトで開発の最新状況と寄付の詳細をご確認ください。 ": "\n请点击此处，\n前往官方网站查看最新开发进展与捐赠详情。",
    }
    if value in common_event:
        return common_event[value]
    simple_frame = re.fullmatch(r"(.+?)(?:の)?フレーム[　 ]*", value)
    if simple_frame:
        frame_name = simple_frame.group(1)
        for source_term, target_term in {
            "牡羊座": "白羊座", "牡牛座": "金牛座", "双子座": "双子座", "蟹座": "巨蟹座",
            "獅子座": "狮子座", "乙女座": "处女座", "天秤座": "天秤座", "蠍座": "天蝎座",
            "射手座": "射手座", "やぎ座": "摩羯座", "てんびん座": "天秤座", "いて座": "射手座",
            "みずがめ座": "水瓶座", "うお座": "双鱼座", "蟹座": "巨蟹座",
        }.items():
            frame_name = frame_name.replace(source_term, target_term)
        for source_term, target_term in sorted(SHIP_NAME_DIRECT.items(), key=lambda item: -len(item[0])):
            frame_name = frame_name.replace(source_term, target_term)
        if not JP_KANA_RE.search(frame_name):
            return f"{frame_name}头像框"
    simple_medal = re.fullmatch(r"(.+?)(?:の)?勲章", value)
    if simple_medal:
        medal_name = simple_medal.group(1)
        for source_term, target_term in sorted(SHIP_NAME_DIRECT.items(), key=lambda item: -len(item[0])):
            medal_name = medal_name.replace(source_term, target_term)
        if not JP_KANA_RE.search(medal_name):
            return f"{medal_name}勋章"
    medal_desc = re.fullmatch(r"イベント【(夏|春)の大運動会】シーズン([ⅠⅡⅢ])「(ニシン鉄砲|障害物競走|極限パルクール)」項目にてランキング上位に輝いた指揮官に贈与したメダル。", value)
    if medal_desc:
        event_name = {"ニシン鉄砲": "鲱鱼炮", "障害物競走": "障碍赛跑", "極限パルクール": "极限跑酷"}[medal_desc.group(3)]
        season_name = "夏日大运动会" if medal_desc.group(1) == "夏" else "春日大运动会"
        return f"赠予在【{season_name}】赛季{medal_desc.group(2)}“{event_name}”项目中获得排名前列的指挥官的奖牌。"
    command_medal = re.fullmatch(r"第(\d+)期Command Sennkiイベントにて、獲得できる記念勲章！その眩く光る栄光の星は、指揮官の誇り！", value)
    if command_medal:
        return f"第{command_medal.group(1)}期Command 战姬活动可获得的纪念勋章！这颗耀眼的荣誉之星，是指挥官的骄傲！"
    command_proof = re.fullmatch(r"Command Sennkiの証し（第(\d+)期）", value)
    if command_proof:
        return f"Command 战姬的证明（第{command_proof.group(1)}期）"
    event_medal = re.fullmatch(r"(.+?)イベントの限定の勲章。イベント海域にて獲得する事ができます。", value)
    if event_medal:
        event_name = event_medal.group(1).replace("ブルースフィアの来訪者", "蓝星访客")
        if not JP_KANA_RE.search(event_name):
            return f"{event_name}活动限定勋章。可在活动海域中获得。"
    obtainable = re.fullmatch(r"[“『]([^”』]+)[”』]にて、獲得可能！", value)
    if obtainable:
        event_name = {"朝日の指令": "朝日的指令", "ブルースフィアの来訪者": "蓝星访客"}.get(obtainable.group(1), obtainable.group(1))
        if not JP_KANA_RE.search(event_name):
            return f"可在“{event_name}”中获得！"
    sbwb_medal = re.fullmatch(r"SBWB-?(\d+)にて、(?:最強のNO\.1大艦隊に輝いた|栄光のNUMVBERS入りの|奮闘のAPPROVAR入りの)戦友たちに送られる純(金|銀|銅)で装飾された記念勲章！(?:その偉業は凄まじく、英雄大艦隊として後世に受け継がれるであろう！|NUMVBERSの意地を見せつけ、次は最強を奪還してみせろ！|APPROVARは無限の可能性に溢れ、下克上を狙い、上位を奪え！)", value)
    if sbwb_medal:
        metal = {"金": "金", "銀": "银", "銅": "铜"}[sbwb_medal.group(2)]
        return f"SBWB-{sbwb_medal.group(1)}中授予战友的纯{metal}装饰纪念勋章！"
    sbwb_frame = re.fullmatch(r"SBWB-?(\d+)にて、(.+?)証しフレーム！(.+?)", value)
    if sbwb_frame:
        title = {"輝かしきNO.1に輝いた": "荣登耀眼NO.1的", "頂きを競うNUMVBERSに輝いた": "荣登争夺顶峰的NUMVBERS的", "惜しくもAPPROVARに輝いた": "荣登虽憾但仍强大的APPROVAR的"}.get(sbwb_frame.group(2))
        if title:
            return f"SBWB-{sbwb_frame.group(1)}中授予{title}指挥官的强者证明头像框！"
    month_frame = re.fullmatch(r"(\d+)月(\d+)日の月(始め|末|締め)記念として、限定フレームを配布いたします。?", value)
    if month_frame:
        timing = "月初" if month_frame.group(3) == "始め" else "月底"
        return f"为纪念{month_frame.group(1)}月{month_frame.group(2)}日{timing}，发放限定头像框。"
    command_level = re.fullmatch(r"第(\d+)期Command Sennkiイベントの”朝日の指令”の指令レベル45にて、獲得できる記念勲章！その眩く光る栄光の星は、指揮官の誇り！", value)
    if command_level:
        return f"第{command_level.group(1)}期Command 战姬活动“朝日的指令”指令等级45可获得的纪念勋章！这颗耀眼的荣誉之星，是指挥官的骄傲！"
    if value == "夏姫祭イベント-【鶴鷹の舞う海】にて、獲得できる限定勲章！隼鹰が中央に終えられ、隼鹰ファンには堪らない一品となっている。":
        return "夏日战姬祭活动—可在【鹤鹰飞舞的海】中获得的限定勋章！隼鹰位于中央，是隼鹰粉丝不容错过的珍品。"
    mail_reward = re.fullmatch(r"【(.+?)】イベントが終了したので、指揮官の(.+?)を配布させていただきます。", value)
    if mail_reward:
        event_name = mail_reward.group(1).replace("夏の大運動会", "夏日大运动会")
        reward_name = {
            "大艦隊ランキング報酬": "大舰队排名奖励", "ランキング報酬": "排名奖励",
            "ポイント報酬": "点数奖励", "個人ランキング報酬": "个人排名奖励",
        }.get(mail_reward.group(2), mail_reward.group(2))
        if not JP_KANA_RE.search(event_name + reward_name):
            return f"【{event_name}】活动已结束，现发放指挥官的{reward_name}。"
    frame_desc = re.fullmatch(r"(.+?)イベントの限定のフレーム。イベントミッションをクリアする事で獲得する事ができます。", value)
    if frame_desc:
        event_name = frame_desc.group(1).replace("ブルースフィアの来訪者", "蓝星访客")
        if not JP_KANA_RE.search(event_name):
            return f"{event_name}活动限定头像框。完成活动任务即可获得。"
    event_bonus = value
    for source_term, target_term in {
        "敵艦に与えた魚雷ダメージ＋": "对敌舰造成的鱼雷伤害+",
        "敵艦に与えた副砲ダメージ＋": "对敌舰造成的副炮伤害+",
        "敵艦に与えたダメージ－": "对敌舰造成的伤害-",
        "味方全体の攻撃ダメージ－": "我方全体攻击伤害-",
        "クリティカル率－": "暴击率-", "航空攻撃除く": "不含航空攻击", "主砲攻撃除く": "不含主炮攻击",
        "戦姫": "战姬", "敵艦": "敌舰", "に与えるダメージ+": "造成的伤害+",
    }.items():
        event_bonus = event_bonus.replace(source_term, target_term)
    for source_term, target_term in sorted(SHIP_NAME_DIRECT.items(), key=lambda item: -len(item[0])):
        event_bonus = event_bonus.replace(source_term, target_term)
    if event_bonus != value and not JP_KANA_RE.search(event_bonus):
        return event_bonus
    raid_progress = re.fullmatch(r"本日累計して%s(\d+)個以上の消費する。挑戦回数\+(%d)", value)
    if raid_progress:
        return f"今日累计消费%s{raid_progress.group(1)}个以上。挑战次数+{raid_progress.group(2)}"
    raid_now = re.fullmatch(
        r"只今<color=#ff9b43>(%s)</color>が<color=#ff9b43>(%s（%d周目）)</color>を挑戦しています。", value
    )
    if raid_now:
        return f"当前<color=#ff9b43>{raid_now.group(1)}</color>正在挑战<color=#ff9b43>{raid_now.group(2)}</color>。"
    raid_damage = re.fullmatch(
        r"<color=#ff9b43>(%s)</color>が<color=#ff9b43>(%s（%d周目）)</color>にて<color=#34ff3e>(%s)</color>のダメージを与え、貢献した<color=#34ff3e>(%s)</color>分のレイドボスPTを獲得しました。",
        value,
    )
    if raid_damage:
        return f"<color=#ff9b43>{raid_damage.group(1)}</color>在<color=#ff9b43>{raid_damage.group(2)}</color>造成了<color=#34ff3e>{raid_damage.group(3)}</color>伤害，获得了贡献<color=#34ff3e>{raid_damage.group(4)}</color>对应的Raid Boss PT。"
    raid_kill = re.fullmatch(
        r"<color=#ff9b43>(%s)</color>が<color=#ff9b43>(%s（%d周目）)</color>レイドボスを撃沈しました！にて<color=#34ff3e>(%s)</color>のダメージを与え、貢献した<color=#34ff3e>(%s)</color>分のレイドボスPTを獲得しました。",
        value,
    )
    if raid_kill:
        return f"<color=#ff9b43>{raid_kill.group(1)}</color>在<color=#ff9b43>{raid_kill.group(2)}</color>击沉了Raid Boss！造成<color=#34ff3e>{raid_kill.group(3)}</color>伤害，获得了贡献<color=#34ff3e>{raid_kill.group(4)}</color>对应的Raid Boss PT。"
    first_charge = re.fullmatch(r"使用する事で、SSR戦姫[-‐](.+?)＆(.+?)から１体を選んで獲得することができます。", value)
    if first_charge:
        a = SHIP_NAME_DIRECT.get(first_charge.group(1), first_charge.group(1))
        b = SHIP_NAME_DIRECT.get(first_charge.group(2), first_charge.group(2))
        if not JP_KANA_RE.search(a + b):
            return f"使用后可从SSR战姬{a}和{b}中任选1名获得。"
    first_charge = re.fullmatch(r"使用する事で、SSR戦姫【(.+?)】か【(.+?)】または上級祈願石(\d+)個から1つ選択し、獲得する事ができます。", value)
    if first_charge:
        a = SHIP_NAME_DIRECT.get(first_charge.group(1), first_charge.group(1))
        b = SHIP_NAME_DIRECT.get(first_charge.group(2), first_charge.group(2))
        if not JP_KANA_RE.search(a + b):
            return f"使用后可从SSR战姬【{a}】、【{b}】或高级祈愿石{first_charge.group(3)}个中任选1项获得。"
    first_charge = re.fullmatch(r"使用する事で、SSR戦姫【(.+?)】か【(.+?)】または上級祈願石(\d+)個から1つ選択し、獲得する事ができます。", value)
    if first_charge:
        a = SHIP_NAME_DIRECT.get(first_charge.group(1), first_charge.group(1))
        b = SHIP_NAME_DIRECT.get(first_charge.group(2), first_charge.group(2))
        if not JP_KANA_RE.search(a + b):
            return f"使用后可从SSR战姬【{a}】、【{b}】或高级祈愿石{first_charge.group(3)}个中任选1项获得。"
    first_charge = re.fullmatch(r"使用する事で、SSR戦姫[-‐](.+?)＆(.+?)から１体を選択して、獲得する事ができます。", value)
    if first_charge:
        a = SHIP_NAME_DIRECT.get(first_charge.group(1), first_charge.group(1))
        b = SHIP_NAME_DIRECT.get(first_charge.group(2), first_charge.group(2))
        if not JP_KANA_RE.search(a + b):
            return f"使用后可从SSR战姬{a}和{b}中任选1名获得。"
    if value == "使用する事で、下記の中から任意着せ替えを選択して、獲得する事ができます。":
        return "使用后可从以下内容中任选一套换装并获得。"
    if value == "使用する事で、下記の中から任意1体戦姫を獲得できる。":
        return "使用后可从以下内容中任选1名战姬。"
    if value == "使用する事で、下記の中から任意SSR戦姫を選択して、獲得する事ができます。":
        return "使用后可从以下SSR战姬中任选并获得。"
    random_ssr = re.fullmatch(r"開けると、SSR(戦艦|駆逐艦)からランダムに1体入手する事ができます。(?:（優先して無獲得のSSR\1から入手できます。）)?", value)
    if random_ssr:
        ship_type = "战列舰" if random_ssr.group(1) == "戦艦" else "驱逐舰"
        suffix = "（优先从尚未获得的SSR" + ship_type + "中获得。）" if "優先" in value else ""
        return f"打开后可从SSR{ship_type}中随机获得1名{suffix}"
    equip_random = re.fullmatch(r"使用する事で、(戦艦|駆逐艦)専属装備出現率UPガチャで入手可能な装備から、ランダムにSSR装備を獲得できます", value)
    if equip_random:
        ship_type = "战列舰" if equip_random.group(1) == "戦艦" else "驱逐舰"
        return f"使用后可从{ship_type}专属装备出现率UP抽卡中可获得的装备里随机获得SSR装备"
    voyage = re.fullmatch(r"ある(戦艦|駆逐艦)の航海日誌。戦闘に役立つ心得がたくさん書かれている。", value)
    if voyage:
        ship_type = "战列舰" if voyage.group(1) == "戦艦" else "驱逐舰"
        return f"某艘{ship_type}的航海日志。上面记载着许多有助于战斗的心得。"
    equip_select = re.fullmatch(r"(駆逐艦|軽巡洋艦|重巡洋艦)装備選択箱（ガチャ限定）", value)
    if equip_select:
        ship_type = {"駆逐艦": "驱逐舰", "軽巡洋艦": "轻巡洋舰", "重巡洋艦": "重巡洋舰"}[equip_select.group(1)]
        return f"{ship_type}装备选择箱（抽卡限定）"
    if value == "<redacted>、Wee Veeのチョコレートを受け取ってください。そして早めに食べるべきと助言します。":
        return "<redacted>，请收下Wee Vee的巧克力。并建议尽早享用。"
    if value == "チームは解散しました。":
        return "队伍已解散。"
    if value == "チームの暗証番号はまだ設定されていません。":
        return "队伍密码尚未设置。"
    if value == "スピードアップモード":
        return "加速模式"
    if value == "お風呂":
        return "浴室"
    if value == "チーム暗号化":
        return "队伍加密"
    if value == "パスワードをチェックする":
        return "检查密码"
    if value == "招待状は発送済みです":
        return "邀请函已发送"
    if value == "チーム作成はステージを選択してから行ってください":
        return "请先选择关卡，再创建队伍"
    if value == "機能を開放":
        return "解锁功能"
    if value == "強化機能を開放":
        return "解锁强化功能"
    if value == "共闘RPが不足でスピードアップモードをオンにすることができません。":
        return "协同作战RP不足，无法开启加速模式。"
    if value == "該当ミッションをクリアしていないので、スピードアップモードをオンにすることができません。":
        return "尚未完成对应任务，无法开启加速模式。"
    if value == "出陣に必要なRPが足りず、購入しますか？":
        return "出击所需RP不足，要购买吗？"
    if value == "%s秒後に再度招待状を送ることができます。":
        return "%s秒后可以再次发送邀请函。"
    if value == "毎日おみくじは一日一回まで引くことができます、\n達成日数をタップしてね、":
        return "每日签最多每天抽取一次，\n点击达成天数吧。"
    duplicate = re.fullmatch(r"戦姫(.+?)が重複しています。編成を調整してください", value)
    if duplicate:
        return f"战姬{duplicate.group(1)}重复，请调整编队。"
    if value == "空いている艦隊があります。この艦隊で出撃しますか？":
        return "有空闲舰队。要使用这支舰队出击吗？"
    if value == "錬金術士戦姫のLV上限を引き上げる事ができる特別な素材、構造のすべてを解明にはまだ時を要するという。（錬金術士専属）":
        return "可提升炼金术士战姬等级上限的特殊素材。据说要彻底解析其结构还需要一些时间。（炼金术士专属）"
    if value == "錬金術士オースチップ":
        return "炼金术士奥斯芯片"
    if value == "非常に高密度に圧縮されたオースエネルギーは、一つだけで大型設備に長時間安定して供給できる。戦姫に直接使用されることはできず、おそらく要塞クラスの戦艦だけがその中の獰猛なエネルギーを直接受けることができる。":
        return "高度压缩的奥斯能量，仅一枚就能长时间稳定地为大型设备供能。不能直接用于战姬，恐怕只有要塞级战列舰才能直接承受其中狂暴的能量。"
    if value == "「K・G　5世級戦艦シリーズ」出現率UPガチャの40連確定報酬として獲得できる記念スタンプ！「さあ！私に続け！」「え…？」":
        return "作为“乔治五世级战列舰系列”出现率UP抽卡40连保底奖励可获得的纪念印章！“来吧！跟上我！”“诶……？”"
    if value == "タップし、ドラッグして位置調整する事ができます":
        return "可以点击并拖动调整位置"
    if value == "出撃艦隊を選択":
        return "选择出击舰队"
    if value == "しばらくお待ちください…":
        return "请稍候……"
    if value == "クリア条件:":
        return "通关条件："
    if value == "S評価：110s以内にクリアする":
        return "S评价：在110秒内通关"
    if value == "空っぽ…装飾品をゲットしよう！":
        return "空空如也……去获取饰品吧！"
    if value == "全てのディリー任務を完了しました":
        return "已完成所有每日任务"
    if value == "全ての週間任務を完了しました":
        return "已完成所有每周任务"
    if value == "%sは%sを購入しました":
        return "%s购买了%s"
    if value == "%sは%sを使用しました":
        return "%s使用了%s"
    if value == "1.プレイヤーは毎日、回数制限なしで海域に挑戦できます。\n2.デイリークエストでは不定期で追加報酬イベントが開催されます。イベント期間中は、1クエストにつき毎日1回の追加報酬獲得チャンスがあります（朝日の配当金を購入すると獲得回数を1回増やせます）。\n3.装備更新ステージのオース戦姫水晶と装備のドロップ確率は異なります。\n4.毎日クエストの第ハ章を開放後に、その毎日クエストのEXモードが解放されます。\n5.「錬金術士」には艦種制限がありません、全ての毎日クエスト海域に出撃する事ができます。":
        return "1.玩家每天可不限次数挑战海域。\n2.每日任务会不定期举办追加奖励活动。活动期间，每个任务每天有1次获得追加奖励的机会（购买朝日的分红可增加1次获得次数）。\n3.装备更新关卡的奥斯战姬水晶与装备掉落概率不同。\n4.解锁每日任务第八章后，将解锁该每日任务的EX模式。\n5.“炼金术士”没有舰种限制，可出击所有每日任务海域。"
    if value == "あなたは大艦隊の「神秘の宝蔵」x1を解放しました":
        return "你解锁了大舰队的“神秘宝藏”×1"
    if value == "この宝蔵は失効されました":
        return "该宝藏已失效"
    if value == "残り時間：%s":
        return "剩余时间：%s"
    if value == "500PTを獲得する毎に、報酬として大艦隊賞品宝箱x1を受取る事ができます":
        return "每获得500PT，即可领取大舰队奖品宝箱×1作为奖励"
    if value == "説明：グリッドレイに委託すると、完成度が多くもらえるよ":
        return "说明：委托给格里德利可获得更多完成度"
    if value == "説明：クレイヴンに委託すると、完成度が多くもらえるよ":
        return "说明：委托给克雷文可获得更多完成度"
    if value == "説明：マッコールに委託すると、完成度が多くもらえるよ":
        return "说明：委托给麦考尔可获得更多完成度"
    if value == "説明：モーリーに委託すると、完成度が多くもらえるよ":
        return "说明：委托给莫利可获得更多完成度"
    if value == "あま〜いモノ大好きな戦姫たち❣️たくさんのケーキをプレゼントして、好感度UP！\n記念装飾品をゲットしよう！":
        return "最喜欢甜食的战姬们❣️送上大量蛋糕，提升好感度！\n去获取纪念饰品吧！"
    if value == "レンズ":
        return "镜片"
    if value == "イベント中、出席ポイントをたくさん集めて、点数に応じたボーナスがもらえるよ":
        return "活动期间收集大量出席点数，可获得与分数对应的奖励哦"
    if value == "現在の出席ポイント":
        return "当前出席点数"
    if value == "着せ替えショップ\n好評販売中":
        return "换装商店\n热卖中"
    if value == "買い物へ":
        return "去购物"
    if value == "受取り済み":
        return "已领取"
    if value == "1、宝のロゴがある福袋を購入すると、大艦隊に宝箱を共有して大量的なPTを獲得できます。\n2、「対決！大艦隊作戦元日篇！」ミッションクリアで、大艦隊の「神秘の宝蔵」が解放し、一定数のPTを獲得する事ができます。\n3、受け取っていない賞品宝箱は最大20個まで預かることができます。これ以上は、PTを獲得し続けても、賞品宝箱を獲得することができませんので、ご注意ください。\n4、宝蔵解放後、カウントダウンが開始します。カウントダウン終了後にこの宝蔵は受け取られない場合は、失効となりますので、ご注意ください。":
        return "1、购买带有宝藏标志的福袋后，可向大舰队共享宝箱并获得大量PT。\n2、完成“对决！大舰队作战元旦篇！”任务后，大舰队的“神秘宝藏”将解锁，并可获得一定数量的PT。\n3、未领取的奖品宝箱最多可保管20个。超过上限后，即使继续获得PT，也无法获得奖品宝箱，请注意。\n4、宝藏解锁后开始倒计时。倒计时结束后若未领取该宝藏，宝藏将失效，请注意。"
    if value == "宝蔵は解放していません。福袋コインで福袋購入または「新年に迎え!雪の玉を集めましょう！」ミッションクリアで、「神秘の宝蔵」を解放する事ができます":
        return "宝藏尚未解锁。可使用福袋硬币购买福袋，或完成“迎接新年！收集雪球吧！”任务来解锁“神秘宝藏”。"
    if value == "<size=24>ルール説明：</size>\n1.特別任務にて、ランダムに様々なケーキの材料を入手することができます。\n2.主なケーキ制作工程として、固定材料（小麦粉）+任意素材（好情イチゴ、恩恵チェリー、厚意マンゴー、慈悲ブルーベリー）を配合させて、制作完了できます。\n3.制作したケーキは保有数が増え、任意の時に右上の戦姫たちにもてなす事ができます。その時に、1～2の好感度を獲得する事ができます。\n4.一定の好感度に達するとここでしか獲得できない豪華装飾品などを獲得する事ができます。\n5.イベント終了後、まだ保有しているケーキや未使用の材料や未獲得の報酬は回収されますので、予めご了承ください！":
        return "<size=24>规则说明：</size>\n1.可通过特殊任务随机获得各种蛋糕材料。\n2.蛋糕制作流程为：固定材料（小麦粉）+任意素材（好情草莓、恩惠樱桃、厚意芒果、慈悲蓝莓）进行调配，完成制作。\n3.制作的蛋糕会计入持有数量，可随时招待右上方的战姬。届时可获得1～2点好感度。\n4.达到一定好感度后，可获得只能在此处取得的豪华饰品等奖励。\n5.活动结束后，尚未持有的蛋糕、未使用材料和未领取奖励都会被回收，请提前知悉！"
    if value == "指令イベントの仕様説明\n1.指令イベント期間中、無料で朝日の指令を受ける事ができます。\n2.朝日の指令は毎日00:00に更新、週間指令はイベント開始から1週間毎に更新されます。指令を達成しポイントを積み上げる事によって、指令レベルを上げる事ができます。指令レベルに従って豪華報酬を獲得する事ができます。指揮本部の指令を解放する事で、追加して特典報酬を獲得する事ができます。\n3.指揮本部の指令を解放する際に、[極秘ファイル]と[究極機密]からどちらかの指令を選択できます。（片方の指令を選択後、もう片方の指令を解放する事はできなくなります。）※途中で解放しても、到達したレベルまでの特典報酬をすべて獲得する事ができます。\n4.指令リスト上の「報酬」をタップする事で、報酬を受け取る事ができます。※朝日の御礼券はと指令ショップで、様々なアイテムと交換する事ができます。\n5.指令イベント終了後、未獲得の報酬はメールボックスに送信されます。また「朝日の御礼券」はリセットされずに、次の指令イベントでも使用可能です。\n":
        return "指令活动规则说明\n1.指令活动期间可免费接受朝日的指令。\n2.朝日指令每天00:00更新，周指令从活动开始起每周更新一次。完成指令并累积点数即可提升指令等级，并根据等级获得丰厚奖励。解锁指挥部指令后，还可额外获得特典奖励。\n3.解锁指挥部指令时，可从[机密文件]和[终极机密]中选择一项指令。（选择一项后，无法再解锁另一项。）※中途解锁也可领取达到等级为止的全部特典奖励。\n4.点击指令列表中的“奖励”即可领取奖励。※朝日答谢券可在指令商店兑换各种道具。\n5.指令活动结束后，未领取奖励会发送至邮箱。“朝日答谢券”不会重置，下次指令活动仍可使用。\n"
    if value == "出席集め\n無料ゲット":
        return "收集出席点数\n免费获取"
    if value == "出席集めへ":
        return "前往收集出席点数"
    if value == "1.戦姫が装備している装備品は倉庫容量に影響しません":
        return "1.战姬装备中的装备不会占用仓库容量"
    if value == "以下は使用間隔の詳細：\n":
        return "以下是使用间隔详情：\n"
    if value == "72時間限定　あと":
        return "限时72小时　剩余"
    if value == "SB2CヘルダイヴァーX1":
        return "SB2C地狱俯冲者X1"
    prayer_detail = re.fullmatch(r"·祈願後、<color=(#[A-Fa-f0-9]+)>(SR|SSR|UR)</color>戦姫(?:を|破片を)獲得した際の基本使用間隔：([0-9]+)日間(?:([0-9]+)時間)?", value)
    if prayer_detail:
        hours = f"{prayer_detail.group(4)}小时" if prayer_detail.group(4) else ""
        return f"·祈愿后，获得<color={prayer_detail.group(1)}>{prayer_detail.group(2)}</color>战姬{('碎片' if '破片' in value else '')}时的基础使用间隔：{prayer_detail.group(3)}天{hours}"
    prayer_limit = re.fullmatch(r"·祈願後、<color=(#[A-Fa-f0-9]+)>(SR|SSR|UR)</color>戦姫(?:を|破片を)獲得した際の使用間隔上限：([0-9]+)日間(?:([0-9]+)時間)?", value)
    if prayer_limit:
        hours = f"{prayer_limit.group(4)}小时" if prayer_limit.group(4) else ""
        return f"·祈愿后，获得<color={prayer_limit.group(1)}>{prayer_limit.group(2)}</color>战姬{('碎片' if '破片' in value else '')}时的使用间隔上限：{prayer_limit.group(3)}天{hours}"
    prayer_wall_state = re.fullmatch(r"祈願壁は<color=(#[A-Fa-f0-9]+)>(%s)</color>の使用間隔に入ります", value)
    if prayer_wall_state:
        return f"祈愿墙进入<color={prayer_wall_state.group(1)}>{prayer_wall_state.group(2)}</color>使用间隔"
    if value == "<color=#D6C112>UR</color>戦姫破片を獲得すると祈願壁は使用間隔に入ります:":
        return "获得<color=#D6C112>UR</color>战姬碎片后，祈愿墙进入使用间隔："
    if value == "オースパーツ-B x25":
        return "奥斯部件-B ×25"
    if value == "オースパーツ-C x35":
        return "奥斯部件-C ×35"
    if value == "にもらえる":
        return "可获得"
    if value == "コードを入力してください":
        return "请输入兑换码"
    if value == "引き継ぎ":
        return "继承"
    if value == "アンロックキャンディ獲得：<color=#ffffff00>糖果</color>%s/%s":
        return "获得解锁糖果：<color=#ffffff00>糖果</color>%s/%s"
    if value == "<color=#5E718A> <size=22>\n·全員の出撃ポイントを回復します</size></color>":
        return "<color=#5E718A> <size=22>\n·恢复全员出击点数</size></color>"
    if value == "<color=#5E718A> <size=22>\n·全員の装備ロックを解除します</size></color>":
        return "<color=#5E718A> <size=22>\n·解除全员装备锁定</size></color>"
    if value == "<color=#3F5064> <size=20>艦隊に以下のバフ強化を付与しますか？</size></color><color=#5E718A> <size=22>\n·%s</size></color>":
        return "<color=#3F5064> <size=20>要为舰队赋予以下增益强化吗？</size></color><color=#5E718A> <size=22>\n·%s</size></color>"
    if value == "<color=#3F5064> <size=20>海域バフ強化：</size></color><color=#5E718A> <size=22>\n·%s</size></color>":
        return "<color=#3F5064> <size=20>海域增益强化：</size></color><color=#5E718A> <size=22>\n·%s</size></color>"
    if value == "装備１０連ガチャと川内のSR装備プレゼント！":
        return "装备十连抽卡与川内的SR装备赠礼！"
    if value == "初回チャージ特典":
        return "首充特典"
    if value == "このレシピの料理報酬の回数が上限に達成しました。「料理報酬」から「リセット報酬」に変更されます。":
        return "该配方的料理奖励次数已达上限，将从“料理奖励”变更为“重置奖励”。"
    if value == "<color=#5E718A> <size=24>·出撃ルートをリセットしますか？ \n*リセット後、クリア回数とバフ強化が保留となり、全てのBOSS艦の耐久値がMAXに回復します。</size></color>":
        return "<color=#5E718A> <size=24>·要重置出击路线吗？ \n*重置后，通关次数与增益强化会保留，所有BOSS舰耐久值恢复至MAX。</size></color>"
    if value == "<color=#5E718A> <size=24>·出撃ルートをリセットできます。 \n*リセット後、クリア回数とバフ強化が保留となり、全てのBOSS艦の耐久値がMAXに回復します。</size></color>":
        return "<color=#5E718A> <size=24>·可以重置出击路线。 \n*重置后，通关次数与增益强化会保留，所有BOSS舰耐久值恢复至MAX。</size></color>"
    if value == "レイドボス挑戦":
        return "Raid Boss挑战"
    if value == "まだ開放時間外です":
        return "尚未到开放时间"
    if value == "報酬倍率が最大になりました。":
        return "奖励倍率已达到最大。"
    if value == "このプレイヤーの貢献値が700PT以上ですので、脱退させることはできません。":
        return "该玩家贡献值达到700PT以上，无法将其移出。"
    if value == "SS評価：80s以内にクリアする":
        return "SS评价：在80秒内通关"
    if value == "SSS評価：70s以内にクリアする":
        return "SSS评价：在70秒内通关"
    if value == "50%OFFクーポンを使用して購入":
        return "使用50%OFF优惠券购买"
    if value == "オークランドをタップ":
        return "点击奥克兰"
    if value == "強化ボタンをタップ":
        return "点击强化按钮"
    if value == "艦隊に戦姫6体を編成する事で、出撃する事ができます。":
        return "舰队编入6名战姬后即可出击。"
    if value == "出撃可能な艦隊数が不足しています。":
        return "可出击舰队数量不足。"
    if value == "この海域は掃討する事ができません。":
        return "无法扫荡本海域。"
    if value == "この海域の掃討は星3で解放されます。":
        return "本海域达到3星后解锁扫荡。"
    if value == "この海域は%s艦隊で同時に出撃する事ができます。":
        return "本海域可同时派出%s支舰队。"
    if value == "まだこの海域は解放されていません。":
        return "本海域尚未解锁。"
    if value == "この艦隊は出撃する事ができません。":
        return "这支舰队无法出击。"
    if value == "敵陣：":
        return "敌阵："
    if value == "推薦戦力：":
        return "推荐战力："
    if value == "本当に変更されますか？":
        return "确定要更改吗？"
    if value == "アカウント削除失敗、後ほど再度お試しください":
        return "账号删除失败，请稍后再试。"
    if value == "敵を全て倒す":
        return "击败所有敌人"
    if value == "聖なるスプレー":
        return "神圣喷雾"
    if value == "戦争指令":
        return "战争指令"
    if value.startswith("1.【周回海域第九＆十章】では、複数の艦隊を出撃し、敵戦艦と戦います。"):
        return "1.【周回海域第九＆十章】中，可派出多支舰队与敌方战列舰战斗。\n2.各海域可出击舰队数量不同，不得超过指定范围。每支出击舰队必须编入6名战姬。（同名战姬不可同时出击）\n3.每次遭遇敌方战列舰时，可选择任意1支舰队参战。每支舰队每次出击最多参战1次。\n4.战败后，若我方可参战舰队数量多于敌方，可从可参战舰队中选择1支继续战斗。（敌方战列舰耐久不会重置，先参战舰队不会获得经验值。）若我方剩余可战斗舰队少于敌方战列舰数量，则挑战失败。\n5.带有【EXP↑】标记的战姬出击【周回海域第九＆十章】时，可获得比通常更多的经验值。"
    if value == "一定の海域にて、SSR戦姫「ル・ファンタスク」と「妙高」をドロップ獲得可能に！イベント期間中に更にドロップ率大幅UP！":
        return "部分海域可掉落SSR战姬“可怖”和“妙高”！活动期间掉落率大幅提升！"
    if value.startswith("基地では、補給や資源、一部の必要なアイテムを入手できます"):
        return "基地可获得补给、资源和部分必要道具。\n・资源说明\n1．机械勋章：消耗机械勋章建造和强化建筑；生产部的部分道具也需要机械勋章才能生产。强化办公室可提升机械勋章上限，机械勋章会随时间增加，奥斯能量室可提升增加率。\n2．奥斯能量：关系到建筑建造与强化上限。提升奥斯能量室等级可提高奥斯能量上限。\n3．食物储备：影响可值班战姬人数。食物不足时无法增加值班战姬。\n建筑类型\n1．办公室：办公室等级决定其他建筑等级上限。升级可解锁新建筑、提升建筑等级上限和机械勋章上限；安排战姬值班可提升生产部与酒馆效率。\n2．奥斯能量室：生产电力并恢复机械勋章；升级可提高机械勋章增加率。\n3．珍珠湾食堂：生产食物；食物不足时战姬无法值班；安排战姬值班可提升食物储备上限。\n4．酒馆：产出资源；升级可提升资源生产速度，安排战姬值班可提升资源生产效率。\n5．宿舍：恢复战姬心情；升级可提升值班战姬心情恢复速度，值班可加快宿舍内战姬心情恢复。\n6．生产部：生产各种培养道具；升级可解锁更高级培养道具配方，安排战姬值班可提升生产效率。\n7．浴室：消耗温泉硬币快速恢复沐浴中战姬的心情。\n战姬特殊能力\n每名战姬拥有1～2个特殊能力，可为不同建筑提供修正效果。值班战姬能力与建筑匹配时可获得更高修正。（注：战姬心情降为0会进入疲劳状态，此时修正效果无效。）"
    tutorial = value
    for source_term, target_term in {
        "指揮官さん": "指挥官", "操舵輪": "舵轮", "左右に回すと": "向左右转动即可",
        "旋回": "转向", "操作ができますよ": "操作哦", "画面をタップ": "点击画面",
        "または": "或", "主砲ボタン": "主炮按钮", "主砲スキルを放つことができますよ": "即可释放主炮技能",
        "素晴らしいです": "做得好", "進撃": "进击", "ボタンをタップして": "点击按钮",
        "作戦を続けましょう": "继续作战吧", "魚雷": "鱼雷", "発射してみましょう": "试着发射吧",
        "照準線": "瞄准线", "ドラッグしてみましょう": "试着拖动", "予測線": "预测线",
        "重なったら": "重合后", "出撃": "出击", "新しい授業": "新课程", "始めましょう": "开始吧",
        "副砲": "副炮", "ボタンをタップしてください": "请点击按钮", "夜戦": "夜战", "モードに入りましょう": "进入模式吧",
        "してみましょう": "吧", "してください": "请", "できますよ": "即可", "ですよ": "哦",
        "を発射して": "发射", "タップして切り替えましょう": "点击切换吧", "従って航行し": "按照指示航行并",
        "新しい授業": "新课程", "重なったら": "重合后",
        "を押すと": "并按下", "する、": "，", "押すと": "，按下", "放つことができます": "即可释放",
        "複縦陣": "复纵阵", "矢印": "箭头", "T字戦法有利": "T字战法有利", "索敵": "索敌",
        "索敵スキル": "索敌技能", "未知エリア": "未知区域", "偵察してください": "请侦察",
        "発令": "发布", "使用しましょう": "使用吧", "近距離射程": "近距离射程", "維持するように": "请保持",
        "切り換えて": "切换至", "しましょう": "吧", "発動させると": "触发后", "砲撃照準": "炮击瞄准",
        "リセットできます": "可以重置", "ギア": "齿轮", "加減速": "加减速", "タップすると": "点击后",
        "先に占拠しましょう": "先占据位置吧", "攻撃してみましょう": "攻击吧", "接近し": "接近并",
        "発射しましょう": "发射吧", "いわゆる": "所谓的", "ですよ": "哦", "の指示": "的指示",
        "と": "，",
        "になれる位置": "可以形成的位置", "隙を見て": "抓住时机", "至近弾": "近失弹", "探索をタップして": "点击探索",
        "新しい戦姫を探しましょう": "寻找新的战姬吧", "探索するたびに一定の": "每次探索都会消耗一定数量的",
        "ダイヤ": "钻石", "消費されます": "被消耗", "マップ": "地图", "マークされた場所": "标记位置",
        "探索を行いましょう": "进行探索吧", "早く新しい戦姫を": "快把新的战姬编入",
        "編入しましょう": "吧", "魚雷の数": "鱼雷数量", "全ての敵艦を消滅させましょう": "消灭所有敌舰吧",
        "航空攻撃を使って敵艦を消滅させましょう": "使用航空攻击消灭敌舰吧",
    }.items():
        tutorial = tutorial.replace(source_term, target_term)
    tutorial = tutorial.replace("を", "").replace("ください", "请")
    if tutorial != value and not JP_KANA_RE.search(tutorial):
        return tutorial
    if value == "敵艦に接近し、近距離射程を維持するように！":
        return "接近敌舰，并保持近距离射程！"
    if value == "この海域では、艦隊に「ライザ」を編成する事で、下記のエレメントが海域に出現し、採取する事で、特殊なパワーを獲得する事ができます。":
        return "在本海域中，舰队编入“莱莎”后，以下元素会出现在海域内，采集元素即可获得特殊力量。"
    if value == "1.製作機能では、製作に【デザイン案】が必要となります。\n2.獲得したデザイン案を選択し、一定の素材などを消費することで、製作を進めることができ、更に連結した製作枠を解放することができます。\n3.全ての製作枠を解放し完成することで、「製作開始」をタップして、限定装備を獲得することができます。":
        return "1.制作功能需要【设计方案】。\n2.选择获得的设计方案并消耗一定素材，即可推进制作并解锁相连的制作栏位。\n3.解锁全部制作栏位并完成后，点击“开始制作”即可获得限定装备。"
    if value == "BOSS敵艦のスキル詳細\nスキル①・皇女の威圧：【戦闘開始時】自身は20秒毎に【皇女の威圧】を発動する。自身周囲に減速エリアを発動する。（持続10秒）また減速エリアに進入した戦姫は速力-50%。\nスキル②・暴風の目：自身が小破または中破した時に自身周囲、吹き荒れる暴風を作り出す。発動から8秒後に爆発を起こし、戦姫は残り耐久値の20%分に相当するダメージを受ける。【暴風の目】に魚雷が命中する事で消失させる事ができる、また全味方戦姫の魚雷を1発補充する。一度の戦闘で最大2回まで発動できる。":
        return "BOSS敌舰技能详情\n技能①·皇女的威压：【战斗开始时】自身每20秒发动【皇女的威压】。在自身周围发动减速区域。（持续10秒）进入减速区域的战姬速力-50%。\n技能②·暴风之眼：自身小破或中破时，在周围制造肆虐的暴风。发动8秒后爆炸，战姬受到相当于剩余耐久值20%的伤害。鱼雷命中【暴风之眼】即可使其消失，并补充我方全体战姬1发鱼雷。每场战斗最多发动2次。"
    if value == "コラボ限定ガチャでは特別に出現率アップする事がございません。":
        return "联动限定卡池不会特别提升出现率。"
    if value == "コラボ限定ガチャでは特別に出現率アップする事がございません。\nイベント終了後、未使用の「魔法の呼び鈴」は回収され、同数の推薦状と交換されます。\nメールボックスにてお受取をお願いします。":
        return "联动限定卡池不会特别提升出现率。\n活动结束后，未使用的“魔法铃铛”会被回收，并兑换为相同数量的推荐信。\n请在邮箱中领取。"
    if value == "目標を喚起":
        return "唤起目标"
    if value == "残酷な赤色の戦姫「赤」。\nその持つ力は悪夢のように強大だった。\nその真の実力を垣間見ることができる氷山の一角が、\nついに訪れる——。":
        return "残酷的赤色战姬“赤”。\n她拥有的力量强大得如同噩梦。\n能够窥见她真正实力的冰山一角，\n终于到来了——。"
    if value == "戦闘途中の記録が残ってます。\n戦闘に戻りますか？\n<color=#ff0000ff>（戻らない場合は挑戦失敗になります）</color>":
        return "保留了战斗中途的记录。\n要返回战斗吗？\n<color=#ff0000ff>（不返回将视为挑战失败）</color>"
    if value == "<size=10>あと</size>%s":
        return "<size=10>剩余</size>%s"
    if value == "本ガチャイベントでは、ピックアップ中のSSR戦姫を獲得する際に、優先して未獲得の戦姫を獲得する事ができます。":
        return "本卡池活动中，获得精选SSR战姬时，会优先获得尚未拥有的战姬。"
    if value == "%s期間限定に獲得可能！":
        return "%s期间限定可获得！"
    if value == "この探索は<color=#417AE3>【期間限定戦姫探索】です</color>。":
        return "本次探索为<color=#417AE3>【期间限定战姬探索】</color>。"
    if (value.startswith("この探索は<color=#417AE3>【一般戦姫探索】です。</color>")
            or value.startswith("この探索は<color=#417AE3>【限定戦姫探索】です。</color>")
            or value.startswith("この探索は<color=#417AE3>【特別戦姫探索】です。</color>")):
        explore_name = (
            "一般战姬探索" if "一般戦姫探索" in value
            else "限定战姬探索" if "限定戦姫探索" in value
            else "特别战姬探索"
        )
        return f"本次探索为<color=#417AE3>【{explore_name}】</color>。\n所有<color=#417AE3>【{explore_name}】</color>中，若连续50次未获得SSR战姬，下次SSR战姬出现率将从2%提升至12%；若仍未获得，则从12%提升至22%，以此类推，达到100%后必定获得SSR战姬。\n未出现SSR战姬的次数会记录在系统中，活动结束后不会重置，提升后的出现率可用于任意<color=#417AE3>【{explore_name}】</color>。\n<color=#D95434>【注意】</color>在任意<color=#417AE3>【{explore_name}】</color>中获得SSR战姬后，记录次数重置，出现率恢复为2%。\n※若获得的SSR战姬已达到突破MAX，可额外获得一定数量的精锐战姬勋章。"
    if value == "2周年周姫祭イベント期間中に、伊168ガチャでは特別に出現率が３倍アップし、0.75%になります。\nイベント終了後、伊168の出現率は0.25%になります。\nこのガチャは永久ガチャです。":
        return "2周年战姬祭活动期间，伊168卡池出现率特别提升至3倍，变为0.75%。\n活动结束后，伊168出现率变为0.25%。\n该卡池为永久卡池。"
    rate_up = value
    for source_term, target_term in sorted(SHIP_NAME_DIRECT.items(), key=lambda item: -len(item[0])):
        rate_up = rate_up.replace(source_term, target_term)
    rate_up = rate_up.replace("アルジェリー", "阿尔及利亚").replace("出現率アップ！！", "出现率提升！！")
    if rate_up != value and not JP_KANA_RE.search(rate_up):
        return rate_up
    reward_list = value
    if value.startswith("戦姫：\n伊168*1(") and "航海日誌" in value:
        for source_term, target_term in {
            "戦姫：": "战姬：", "装備：": "装备：", "アイテム:": "道具：", "祈願石": "祈愿石",
            "航海日誌": "航海日志", "潜水艦": "潜艇", "空母": "航空母舰", "戦艦": "战列舰", "巡洋艦": "巡洋舰", "駆逐艦": "驱逐舰",
            "育成パック": "培养礼包", "潜水艦オースチップ": "潜艇奥斯芯片", "選択箱": "选择箱", "ランダム箱": "随机箱",
            "合同演習記録": "联合演习记录", "海域演習記録": "海域演习记录", "深海記憶のカケラ": "深海记忆碎片",
            "万象ファクター": "万象因子", "戦火の記憶": "战火的记忆", "戦姫育成パック": "战姬培养礼包",
            "誓いの指輪": "誓约戒指", "装備育成パック": "装备培养礼包", "上級装備交換コイン": "高级装备兑换硬币", "豪華装備交換コイン": "豪华装备兑换硬币",
            "の": "的", "個": "个", "装備選択箱": "装备选择箱", "ガチャ限定版": "抽卡限定版",
            "オースチップ": "奥斯芯片", "戦姫": "战姬", "上級": "高级", "装備": "装备", "祈願": "祈愿",
        }.items():
            reward_list = reward_list.replace(source_term, target_term)
        reward_list = reward_list.replace("8型ディーゼルサイクル改", "8型柴油循环改").replace("Z工房実験", "Z工房实验")
        if not JP_KANA_RE.search(reward_list):
            return reward_list
    support_text = value
    if any(term in value for term in ("処理技術", "戦術の最適化", "抵抗ユニット", "予備弾薬支援", "衛生看護")):
        for source_term, target_term in {
            "炎上処理技術": "着火处理技术", "浸水処理技術": "进水处理技术", "衛生看護": "卫生护理",
            "緊急事故処理": "紧急事故处理", "砲撃戦術の最適化": "炮击战术优化", "魚雷戦術の最適化": "鱼雷战术优化",
            "空襲戦術の最適化": "空袭战术优化", "制圧戦術の最適化": "压制战术优化", "壊滅上手": "歼灭高手",
            "壊滅エリート": "歼灭精英", "妨害抵抗ユニット": "干扰抵抗单元", "炎上抵抗ユニット": "着火抵抗单元",
            "浸水抵抗ユニット": "进水抵抗单元", "予備弾薬支援": "备用弹药支援", "広域": "广域", "集中": "集中",
            "味方チーム": "我方队伍", "チーム全体": "队伍全体", "敵全体": "敌方全体", "敵旗艦": "敌方旗舰",
            "耐久値": "耐久值", "回避率": "闪避率", "砲撃防御": "炮击防御", "魚雷防御": "鱼雷防御", "空襲防御": "空袭防御",
            "ダメージ": "伤害", "下げて": "降低", "上げて": "提高", "重複できません": "不可叠加",
            "効果は重複できず": "效果不可叠加", "その最高値を取って発動します": "取最高值生效",
            "味方チームが受ける": "我方队伍受到的", "味方チームに": "对我方队伍",
            "味方全体は": "我方全体", "味方が": "我方",
            "敵全体の": "敌方全体的", "敵旗艦の": "敌方旗舰的",
            "敵の砲撃と航空攻撃と魚雷攻撃から": "敌方炮击、航空攻击和鱼雷攻击的",
            "いずれかのキャラクターが": "任一角色",
            "耐久値は": "耐久值", "耐久値が": "耐久值",
            "ミュー": "缪", "ゼータ": "泽塔", "シャルンホルスト": "沙恩霍斯特",
            "ホーエル": "霍埃尔", "ヒーアマン": "希尔曼", "ダンケルク": "敦刻尔克",
            "フィウメ": "菲乌梅", "エディンバラ": "爱丁堡", "龍驤": "龙骧",
            "デューク・オブ・ヨーク": "约克公爵", "ル・テリブル": "可怖", "ジョンストン": "约翰斯顿",
            "伊勢": "伊势", "翔鶴": "翔鹤", "愛宕": "爱宕", "高雄": "高雄",
            "一度チーム全体に": "使队伍全体一次", "敵からの": "来自敌方的", "防げます": "防止",
            "戦闘内で重複でトリガーすることはできません": "同场战斗内无法重复触发", "トリガー可能です": "可触发",
            "秒毎に": "每秒", "回復し": "恢复", "最大": "最多", "場合": "时", "受ける": "受到",
            "まで": "为止", "いずれか": "任一", "から": "的", "時": "时", "時、": "时，",
            "は": "", "複数": "多个", "重複": "叠加", "以下": "以下", "以上": "以上",
            "に対して": "对", "敵に": "敌方", "戦闘内で": "同场战斗内", "一度": "一次",
            "新たな支援効果": "新的支援效果", "戦闘開始時": "战斗开始时", "戦闘中": "战斗中",
            "攻撃を受ける時": "受到攻击时", "一回の回避機会があり": "有1次闪避机会",
            "魚雷1枚": "1发鱼雷", "魚雷を搭載した全ての戦姫": "所有装备鱼雷的战姬",
            "魚雷が": "鱼雷", "同じ位置にいる敵": "处于相同位置的敌人",
            "回復させる": "恢复", "可能です": "可以", "確率で": "有概率",
            "大破した場合": "大破时", "すぐ": "立即", "発動": "发动", "命中しなかった場合": "未命中时",
            "即装填完了させます": "立即完成装填", "攻撃が命中": "攻击命中", "魚雷を搭載した": "装备鱼雷的",
            "魚雷を発射する時": "发射鱼雷时", "確率で": "有概率", "補充する": "补充",
            "敵の砲撃": "敌方炮击", "航空攻撃": "航空攻击", "魚雷攻撃": "鱼雷攻击", "必ず回避できる": "必定闪避",
            "編隊中": "编队中", "同じ位置にいる敵": "处于相同位置的敌人", "受ける魚雷ダメージ": "受到的鱼雷伤害",
            "アップする": "提升", "下げます": "降低", "所持": "持有", "キャラクター": "角色",
        }.items():
            support_text = support_text.replace(source_term, target_term)
        for source_term, target_term in sorted(SHIP_NAME_DIRECT.items(), key=lambda item: -len(item[0])):
            support_text = support_text.replace(source_term, target_term)
        support_text = support_text.replace("ヴェネト", "维内托").replace("島風", "岛风").replace("イータ", "伊塔")
        for source_term, target_term in {
            "されます": "会被", "できます": "可以", "します": "", "される": "被", "する": "",
            "ます": "", "です": "是", "の": "的", "を": "", "に": "", "と": "和", "が": "",
        }.items():
            support_text = support_text.replace(source_term, target_term)
        for source_term, target_term in {
            "命中しなかった": "未命中", "大破した": "大破", "可能是": "可以",
            "戦闘開始时": "战斗开始时", "防衛戦備": "防卫战备", "魚雷補給線": "鱼雷补给线",
            "集中突撃・魚雷": "集中突击·鱼雷", "支配力": "支配力", "集中突撃": "集中突击",
            "攻撃": "攻击", "魚雷": "鱼雷", "発射": "发射", "補給": "补给",
            "編入": "编入", "チーム": "队伍", "特定": "特定", "効果": "效果",
            "得る": "获得", "特殊な": "特殊", "味方的": "我方的", "こ和で": "后即可",
        }.items():
            support_text = support_text.replace(source_term, target_term)
        if not JP_KANA_RE.search(support_text):
            return support_text
    item_list = value
    for source_term, target_term in {
        "セレスティアシーカー": "塞莱丝蒂亚探索者", "暗夜のドレス": "暗夜礼服", "渡り鳥のお守り": "候鸟护符",
        "魔石のチェイン": "魔石锁链", "グリムクォーツ": "格林石英", "妖精の羽衣": "妖精羽衣", "退魔のブローチ": "退魔胸针",
        "クォーツネックレス": "石英项链", "錬金四連装魚雷": "炼金四联装鱼雷", "錬金三連装砲": "炼金三联装炮",
        "錬金連装砲": "炼金连装炮", "錬金爆撃機": "炼金轰炸机", "錬金攻撃機": "炼金攻击机", "錬金徹甲弾": "炼金穿甲弹",
        "万能オース装備コア": "万能奥斯装备核心", "錬金SSR素材選択箱": "炼金SSR素材选择箱", "上級祈願石": "高级祈愿石",
        "戦姫育成必須セット": "战姬培养必备套装", "ヘリオプロクス": "赫利俄斯之光", "チェインベスト": "锁链背心",
        "焔雪の耳飾り": "焰雪耳饰", "雷嵐の耳飾り": "雷岚耳饰", "レヘルン": "雷赫尔恩", "ローゼフラム": "蔷薇烈焰",
        "錬金SR素材選択箱": "炼金SR素材选择箱", "マグマパウダー": "岩浆粉", "ロテスヴァッサ鉱水": "洛特斯瓦塞矿水",
        "大貝の白玉": "大贝白玉", "琥珀の欠片": "琥珀碎片", "清水の白姫": "清水白姬", "白煙炭": "白烟炭",
        "翼竜の翼": "翼龙之翼", "幸せクローバー": "幸福四叶草", "琥珀水晶": "琥珀水晶", "スピリットフラワー": "精灵花",
        "灯篭ホタル": "灯笼萤火虫", "リバーストン": "逆转石", "蒼炎": "苍炎", "幻獣の毛皮": "幻兽毛皮", "聖樹の大枝": "圣树大枝",
        "アクア鉱": "水蓝矿", "彗星岩": "彗星岩", "香る蜜木": "芬芳蜜木", "大ぷに玉": "大噗尼玉", "エレメントコア": "元素核心",
        "常世の焔": "常世之焰", "魔獣の甲殻": "魔兽甲壳", "洞窟サンゴ": "洞窟珊瑚", "ぷにぷに玉": "噗尼玉", "デルフィローズ": "德尔菲玫瑰",
        "ゴーレムのコア": "魔像核心", "聖石の欠片": "圣石碎片", "太陽の花": "太阳花", "黄金うに": "黄金海胆", "永遠結晶": "永恒结晶",
        "竜眼": "龙眼", "中和剤": "中和剂", "研磨剤": "研磨剂", "インゴット": "铸块", "ブロンズアイゼン": "青铜艾森",
        "スタルチウム": "斯塔尔钛姆", "クロース": "布料", "錬金繊維": "炼金纤维", "ネイチャークロス": "自然布",
        "ビーストエア": "兽皮气囊", "パールクリスタル": "珍珠水晶", "アンバーライト": "琥珀光", "火薬のもと": "火药原料",
        "蒼炎の種火": "苍炎种火", "ホーリーナット": "圣洁坚果", "ぷにレザー": "噗尼皮革", "ガラスの花": "玻璃花",
        "ブーバ": "布巴", "三柱": "三柱", "バジル": "罗勒", "コクーン": "茧", "空海": "空海", "練式": "炼式",
        "ディヴェル": "迪贝尔",
    }.items():
        item_list = item_list.replace(source_term, target_term)
    for source_term, target_term in sorted(SHIP_NAME_DIRECT.items(), key=lambda item: -len(item[0])):
        item_list = item_list.replace(source_term, target_term)
    item_list = item_list.replace("ライザ-ディヴェルの抱擁", "莱莎·迪贝尔的拥抱")
    item_list = item_list.replace("ディヴェルの抱擁", "迪贝尔的拥抱")
    item_list = item_list.replace("の", "的")
    if item_list != value and "\n" in value and not JP_KANA_RE.search(item_list):
        return item_list
    exploration = value
    for source_term, target_term in {
        "今回の探索では必ず": "本次探索必定",
        "あと": "还需",
        "回探索すると": "次探索后",
        "今回の探索では": "本次探索",
        "探索では": "探索中",
        "または": "或",
        "戦姫を獲得できます": "获得战姬",
        "を必ず獲得できます": "必定获得",
        "指定戦姫ボックス（限定版）": "指定战姬箱（限定版）",
        "出現率アップ": "出现率提升",
        "出現率": "出现率",
        "探索": "探索",
        "戦姫": "战姬",
        "未獲得": "未获得",
        "連続": "连续",
        "回とも": "次都",
        "獲得する事ができます": "可以获得",
        "獲得時": "获得时",
        "獲得済み": "已获得",
        "追加報酬": "追加奖励",
        "精鋭戦姫勲章": "精锐战姬勋章",
        "すべて": "所有",
        "もれなく": "全部",
        "ようになります": "即可",
        "出てこない": "未出现",
        "記載されます": "会被记录",
        "使えます": "可以使用",
        "類推して": "依此",
        "達すると": "达到后",
        "必ず": "必定", "獲得できます": "可获得", "獲得すると": "获得后", "獲得時に": "获得时",
        "です": "是", "では": "中", "場合": "时", "次回": "下次", "上昇": "提升", "できるようになります": "即可",
        "回数": "次数", "システム": "系统", "期間が終了後も": "活动结束后仍", "アップした": "提升后的",
        "所持していた": "持有", "追加報酬として": "作为追加奖励", "一定数の": "一定数量的", "から": "从",
        "戻ります": "恢复",
        "任意": "任意",
        "期間": "期间",
        "終了": "结束",
        "注意": "注意",
    }.items():
        exploration = exploration.replace(source_term, target_term)
    for source_term, target_term in sorted(SHIP_NAME_DIRECT.items(), key=lambda item: -len(item[0])):
        exploration = exploration.replace(source_term, target_term)
    exploration = exploration.replace("サミュエル・B・ロバーツ", "塞缪尔·B·罗伯茨").replace("アップ！！", "提升！！")
    if exploration != value and not JP_KANA_RE.search(exploration):
        return exploration
    if value == "1.期間中、虹色チップx1を消費して、抽選ガチャを1回引くことができます。\n2.ガチャに、幾つかの大当たり品が含まれており、大当たり品の任意一つ抽選する事で、在庫補充する事ができます。\n3.ガチャする度に確率でおまけにSP賞品を獲得する事ができます。\n4.獲得確率詳細：\n    S級賞品：1.50%\n    A級賞品：9.60%\n    B級賞品：17.90%\n    C級賞品：34.50%\n    D級賞品：36.50%\n    SP賞品：5.00%":
        return "1.活动期间消耗彩虹芯片×1，可抽取抽奖卡池1次。\n2.卡池中包含若干大奖物品，抽取任意大奖物品后即可补充库存。\n3.每次抽卡都有概率额外获得SP奖品。\n4.获得概率详情：\n    S级奖品：1.50%\n    A级奖品：9.60%\n    B级奖品：17.90%\n    C级奖品：34.50%\n    D级奖品：36.50%\n    SP奖品：5.00%"
    if value == "このフレームは未獲得":
        return "该头像框尚未获得"
    if value == "フレーム":
        return "头像框"
    if value == "フレーム一覧":
        return "头像框列表"
    if value == "キャンセル済み":
        return "已取消"
    if value == "%sを消費して%sを購入しますか？":
        return "要消耗%s购买%s吗？"
    if value == "このアイコンが解放されていません":
        return "该头像尚未解锁"
    if value == "戦姫を所持する事で、この戦姫のアイコンを購入する事ができます。":
        return "持有该战姬后即可购买该战姬头像。"
    if value == "アイコンが選択されていません。ご選択ください。":
        return "尚未选择头像，请选择。"
    if value == "このアイコンは獲得済みですが、受け取りますか？":
        return "该头像已经获得，要领取吗？"
    if value == "この戦姫と誓約していませんので、「愛のフレーム」を使用する事ができません":
        return "尚未与该战姬缔结誓约，无法使用“爱的头像框”。"
    if value == "必要な素材が足りません":
        return "所需素材不足"
    if value == "成功に強化しました":
        return "强化成功"
    if value == "成功に開放しました":
        return "解锁成功"
    if value == "開放条件を満たしていない":
        return "未满足解锁条件"
    if value == "強化条件を満たしていない":
        return "未满足强化条件"
    if value == "前の調合材料を投入した後に解放になります。":
        return "投入上一份调合材料后解锁。"
    if value == "瀑粉うに大量投擲":
        return "大量投掷瀑粉海胆"
    if value == "好感度が最大値に到達しているため、これ以上好感度を上げられません。":
        return "好感度已达到最大值，无法继续提升。"
    if value == "好感度が上限に到達しました。戦姫と誓いを交わす事で好感度の上限を解放する事ができます。":
        return "好感度已达到上限。与战姬缔结誓约后即可解锁好感度上限。"
    if value == "好感度アイテムを選択してください":
        return "请选择好感度道具"
    if value == "好感度アイテムを所持していません":
        return "未持有好感度道具"
    if value == "贈る":
        return "赠送"
    if value == "現在の部屋状態を戦友チームに切り替えますか？":
        return "要将当前房间状态切换为战友队伍吗？"
    if value == "装着中の装備を選択しました。調合材料として使いますか？":
        return "已选择正在装备的装备。要作为调合材料使用吗？"
    if value == "強化した装備を選択しました。調合材料として使いますか？":
        return "已选择强化过的装备。要作为调合材料使用吗？"
    if value == "装着中の強化装備を選択しました。調合材料として使いますか？":
        return "已选择正在装备的强化装备。要作为调合材料使用吗？"
    if value == "ロックする事で、装備のLV上限を解放する事ができます。\n※ロックを解除するためには、“ロックブロック”を消費する事で、解除できます。":
        return "锁定后即可解锁装备等级上限。\n※消耗“锁定方块”即可解除锁定。"
    if value == "“ロックブロック”を消費してロックを解除できます。\n※ロック後に消費した強化素材は、解除後に全返還されます。":
        return "消耗“锁定方块”即可解除锁定。\n※锁定后消耗的强化素材将在解除锁定后全部返还。"
    if value == "%s研究してから解放":
        return "研究%s后解锁"
    if value == "あと%s":
        return "还剩%s"
    if value == "一斉装備":
        return "一键装备"
    if value == "1.巡洋ラボの影響を受けた艦種は軽巡洋艦と重巡洋艦と錬金術士である。\n2.戦艦ラボの影響を受けた艦種には、巡洋戦艦、戦艦、補修艦が含まれています。\n3.ラボが提供する一部のボーナスは戦闘中のみ有効":
        return "1.受巡洋实验室影响的舰种为轻巡洋舰、重巡洋舰和炼金术士。\n2.受战列舰实验室影响的舰种包括战列巡洋舰、战列舰和维修舰。\n3.实验室提供的部分加成仅在战斗中有效"
    if value == "1．Comeback戦姫召喚イベント期間中、戦姫召喚は毎日5回まで行う事が可能です。この回数は毎日00:00にリセットされます；\n2．Comeback戦姫召喚に参加し、召喚した戦姫をキープする事で、イベント終了後に最後までキープした戦姫を一体を獲得する事ができます。※イベント終了後にて、順次キープした戦姫はゲーム内メールボックスにてお送りいたしますので、そちらからお受け取りいただけます。皆様お忘れなくご参加してみてくださいね~♥":
        return "1．Comeback战姬召唤活动期间，每天最多可进行5次战姬召唤。次数每天00:00重置；\n2．参加Comeback战姬召唤并保留召唤出的战姬，活动结束后即可获得最后保留的1名战姬。※活动结束后，保留的战姬会陆续发送至游戏内邮箱，请从邮箱领取。不要忘记参加哦~♥"
    if value == "既に【%s】が仕事をしています。外して【%s】に代わりますか？":
        return "【%s】已经在工作。要将其移出并换成【%s】吗？"
    if value == "この着せ替えは既に【%s】にて、着用済みです。一度外して、こちらのライザに着用しますか？":
        return "该换装已被【%s】穿着。要先卸下，再让莱莎穿上吗？"
    if value == "この戦姫と誓いを交わす事はできません。":
        return "无法与该战姬缔结誓约。"
    if value == "任意のところをタップして召喚しよう！":
        return "点击任意位置进行召唤吧！"
    if value == "選択した調合材料を売却する事で下記アイテムを獲得可能":
        return "出售选定的调合材料即可获得以下道具"
    if value == "調合材料を選択してください":
        return "请选择调合材料"
    if value == "コード発行":
        return "生成代码"
    if value == "「倉庫」-「装備」にて解体してください":
        return "请在“仓库”-“装备”中进行分解"
    if value == "コラボ素材売却":
        return "出售联动素材"
    if value == "魔法の呼び鈴が足りません。":
        return "魔法铃铛不足。"
    if value == "コラボアイテム『錬金釜』を獲得する事で、調合機能を解放、使用する事が可能になります。":
        return "获得联动道具『炼金釜』后，即可解锁并使用调合功能。"
    if value == "コラボイベントにて、調合レシピを獲得してから、この調合を行えます。":
        return "在联动活动中获得调合配方后，才能进行该调合。"
    if value == "%sが足りません。\n%sを使って%sを%s個購入してから探索しますか？":
        return "%s不足。要使用%s购买%s%s个后再进行探索吗？"
    if value == "艦隊に<color=#FFB700FF>SR/SSRの任意の戦姫</color>を1名編成するごとに、戦わずに敵艦隊を<color=#FFB700FF>即時殲滅する</color>チャンスを1回獲得（最大5回）\n砲撃戦に突入していない時の味方艦隊の全ユニットの速力が30ノット上昇":
        return "舰队每编入1名<color=#FFB700FF>任意SR/SSR战姬</color>，即可获得1次不战斗<color=#FFB700FF>立即歼灭</color>敌舰队的机会（最多5次）\n未进入炮击战时，我方舰队全体单位速力提升30节"
    if value == "·雷の元素（<color=#DB7B00>エレメントコア・雷</color>）を採取した際に、BOSS艦に【麻痺】効果を与える。（持続10秒）【麻痺】効果はBOSS艦の主砲攻撃、魚雷攻撃及び航空攻撃を使用不可にする。":
        return "·采集雷元素（<color=#DB7B00>元素核心·雷</color>）时，对BOSS舰施加【麻痹】效果。（持续10秒）【麻痹】效果会使BOSS舰无法使用主炮攻击、鱼雷攻击和航空攻击。"
    if value == "·氷の元素（<color=#2181FF>エレメントコア・氷</color>）を採取した際に、味方全員は最大耐久値の50％分を回復する。":
        return "·采集冰元素（<color=#2181FF>元素核心·冰</color>）时，我方全体恢复最大耐久值的50%。"
    if value == "·風の元素（<color=#49A949>エレメントコア・風</color>）を採取した際に、味方全員は次の敵艦からの攻撃を1回だけ、無効にする事ができます。":
        return "·采集风元素（<color=#49A949>元素核心·风</color>）时，我方全体可使下一次敌舰攻击无效。"
    if value == "1.イベント期間中、限定アイテム『サンタ帽』を消費して、ガチャを引くことができます。LV毎の豪華賞品をランダムに獲得することができます。\n2.このイベントには『LV抽選』と『周回抽選』の二種類があり、LV1～最大LV4は『LV抽選』となります。\nLV抽選：各LV毎に、大当たり賞品は２点が含まれ，２点を全部引くことで、賞品内容をリセットし、LVアップすることができます。\n周回抽選：LV抽選を最大LVまでリセットしてから、大当たり賞品以外の賞品を継続して引くことができます。\n3.大当たり賞品リスト\nLV1抽選：【ゆけっ！】【まかせて！】\nLV2抽選：【サンタクロースの証】【合同演習記録】\nLV3抽選：【お姉ちゃんの頼みなら】【合同演習記録】\n最大LV4抽選：【なんでもできる妹を召喚】【合同演習記録】":
        return "1.活动期间消耗限定道具『圣诞帽』即可抽卡，可随机获得各等级的丰厚奖品。\n2.活动分为『等级抽选』和『循环抽选』两种，LV1至最高LV4为『等级抽选』。\n等级抽选：每个等级包含2项大奖，抽完2项后可重置奖品内容并提升等级。\n循环抽选：等级抽选重置至最高等级后，可继续抽取大奖以外的奖品。\n3.大奖列表\nLV1抽选：【冲啊！】【交给我吧！】\nLV2抽选：【圣诞老人的证明】【联合演习记录】\nLV3抽选：【既然是姐姐的请求】【联合演习记录】\n最高LV4抽选：【召唤无所不能的妹妹】【联合演习记录】"
    if value == "1、一斉装備は、指揮官の決めた基準（レベル、或いは戦力、或いは火力）で、戦姫全員から、一斉装備可能な戦姫に一斉装備を行う\n“一斉装備可能の条件とは：\n戦姫レベル＞1且つ、前回出撃した際に使用した装備以外の装備欄を対象とする（前回使用した装備は、変更することはできない）”\n2、専属艦隊にいる戦姫を優先して、一斉装備を行います\n3、最大55名までの戦姫を対象に一斉装備を行います\n4、戦姫の戦力、火力は使用している装備と出撃ポイントに影響されます":
        return "1、一键装备会按照指挥官设定的标准（等级、战力或火力），从全体战姬中为可一键装备的战姬统一装备。\n“一键装备条件：\n战姬等级＞1，且对象为除上次出击时使用装备以外的装备栏（上次使用的装备无法更换）”\n2、优先为专属舰队中的战姬进行一键装备。\n3、一键装备最多以55名战姬为对象。\n4、战姬战力与火力会受正在使用的装备和出击点数影响。"
    if value == "出撃艦隊を選択":
        return "选择出击舰队"
    if value == "改造後の戦姫のみを表示します":
        return "仅显示改造后的战姬"
    gacha_rule = value
    for source_term, target_term in {
        "最大120連以内に、必ず対象装備を全て獲得する事が可能です": "最多120连内必定获得全部目标装备",
        "全対象装備が【獲得済み】でない状態は": "在全部目标装备均未【获得】时",
        "最大30連以内イベントガチャを引くごとに": "每次抽取活动卡池，最多30连内",
        "ランダムに必ず【未獲得状態】の対象装備を獲得できます": "必定随机获得处于【未获得】状态的目标装备",
        "（獲得した対象装備は獲得済み状態になります）": "（获得的目标装备将变为已获得状态）",
        "全対象装備を獲得した以降は": "获得全部目标装备后",
        "ランダムに必ず対象装備を獲得できます": "必定随机获得目标装备",
        "全対象装備を獲得したら、続けてガチャを引くことをオススメいたしません。": "获得全部目标装备后，不建议继续抽卡。",
        "全対象装備を獲得するまでの、【確定獲得】の確率一覧": "获得全部目标装备前的【必定获得】概率列表",
        "獲得済みとなった対象装備は【確定獲得対象装備】から外されます": "已获得的目标装备将从【必定获得目标装备】中移除",
        "全対象装備を獲得した以降の、【確定獲得】の確率一覧": "获得全部目标装备后的【必定获得】概率列表",
        "の確率": "的概率",
        "S級賞品": "S级奖品", "A級賞品": "A级奖品", "B級賞品": "B级奖品", "C級賞品": "C级奖品", "D級賞品": "D级奖品", "SP賞品": "SP奖品",
        "獲得確率詳細：": "获得概率详情：", "1回引くことができます": "可抽取1次", "抽選ガチャ": "抽奖卡池",
        "期間中": "活动期间", "幾つかの": "若干", "含まれており": "包含", "引くことができます": "可以抽取",
        "対象装備": "目标装备", "獲得済み": "已获得", "未獲得状態": "未获得状态", "イベントガチャ": "活动卡池",
        "引くごとに": "每次抽取", "オススメいたしません": "不建议", "続けて": "继续",
        "虹色チップ": "彩虹芯片", "大当たり品": "大奖物品", "任意一つ": "任意一项", "抽選する事で": "抽取后",
        "在庫補充する事ができます": "可以补充库存", "ガチャする度に確率でおまけに": "每次抽卡有概率额外",
        "SP賞品を獲得する事ができます": "获得SP奖品",
        "三連装砲": "三联装炮", "四連装魚雷": "四联装鱼雷", "連装砲": "连装炮", "単装副砲": "单装副炮",
        "対空砲": "防空炮", "対魚雷バルジ": "防鱼雷突出部", "艦上攻撃機": "舰载攻击机", "格納庫": "机库",
        "バッファロー": "水牛", "インペリアル": "帝国", "エリオコン": "厄利空", "エリコン": "厄利空", "装甲": "装甲",
    }.items():
        gacha_rule = gacha_rule.replace(source_term, target_term)
    if gacha_rule != value and not JP_KANA_RE.search(gacha_rule):
        return gacha_rule
    if value == "改造後の戦姫のみを表示します":
        return "仅显示改造后的战姬"
    if value == "この抽選ガチャの大当たり賞品をすべて抽選していません。リセットしますか？":
        return "该抽奖卡池的全部大奖物品尚未抽取，要重置吗？"
    if value == "ガチャをリセットする事ができます。リセットしますか？":
        return "可以重置卡池。要重置吗？"
    if value == "この抽選ガチャの大当たり賞品をすべて抽選していません。在庫補充しますか？":
        return "该抽奖卡池的全部大奖物品尚未抽取，要补充库存吗？"
    if value == "在庫補充する事ができます。在庫補充しますか？":
        return "可以补充库存。要补充库存吗？"
    if value == "次の賞品内容を見る":
        return "查看下一项奖品内容"
    if value == "まで":
        return "为止"
    if value == "お得な":
        return "优惠"
    if value == "学習済み":
        return "已学会"
    if value == "改造の仕様説明\n 1.改造では、改造処置が許された戦姫にのみ、改造を施せます。（定期的に解放予定）\n2.改造は戦姫の詳細ページの緑のボタンをタップして、行う事ができます。\n3.改造工程には、一定の資源に、ステータス条件、特殊な素材を必要とする事で、改造工程を完遂する事ができます。また改造には段階フェイズがあり、第Ⅰ~最大第Ⅲまで、改造強化する事ができます。（次の段階フェイズに行くためには、上の段階の工程を全達成する事で解放されます。）\n4.第Ⅰフェイズでは、スキル強化+を、第Ⅱフェイズでは、新たなスキルを、第Ⅲフェイズでは、上のレア度を解放し、改造版の着せ替えを獲得できます。その戦姫の更なる可能性を引き出す事ができます。":
        return "改造规则说明\n1.改造仅可对允许改造的战姬进行。（计划定期开放）\n2.点击战姬详情页的绿色按钮即可进行改造。\n3.改造过程需要一定资源、属性条件和特殊素材才能完成。改造分为多个阶段，最多可强化至第Ⅲ阶段。（完成上一阶段的全部流程后，才能解锁下一阶段。）\n4.第Ⅰ阶段解锁技能强化+，第Ⅱ阶段解锁新技能，第Ⅲ阶段解锁更高稀有度并获得改造版换装。可以发掘该战姬更大的潜力。"
    if value == "Comeback戦姫召喚イベント期間中、戦姫召喚は毎日5回まで行う事が可能です。この回数は毎日00:00にリセットされます；":
        return "Comeback战姬召唤活动期间，每天最多可进行5次战姬召唤。次数每天00:00重置；"
    if value == "<color=#D6C112>UR</color>戦姫破片を獲得すると祈願壁は使用間隔に入ります:":
        return "获得<color=#D6C112>UR</color>战姬碎片后，祈愿墙进入使用间隔："
    if value == "個人の雪花の所持数は<color=#3679f5>%s</color>に達することで入手できます。":
        return "个人持有雪花数量达到<color=#3679f5>%s</color>即可获得。"
    if value == "·祈願後、<color=#AC54F7>SR</color>戦姫を獲得した際の基本使用間隔：01日間12時間":
        return "·祈愿后获得<color=#AC54F7>SR</color>战姬时的基础使用间隔：1天12小时"
    if value == "·祈願後、<color=#D6C112>SSR</color>戦姫を獲得した際の基本使用間隔：06日間":
        return "·祈愿后获得<color=#D6C112>SSR</color>战姬时的基础使用间隔：6天"
    if value == "·祈願後、<color=#AC54F7>SR</color>戦姫を獲得した際の使用間隔上限：02日間06時間":
        return "·祈愿后获得<color=#AC54F7>SR</color>战姬时的使用间隔上限：2天6小时"
    if value == "·祈願後、<color=#D6C112>SSR</color>戦姫を獲得した際の使用間隔上限：09日間":
        return "·祈愿后获得<color=#D6C112>SSR</color>战姬时的使用间隔上限：9天"
    if value == "·祈願後、<color=#D6C112>UR</color>戦姫破片を獲得した際の基本使用間隔：09日間":
        return "·祈愿后获得<color=#D6C112>UR</color>战姬碎片时的基础使用间隔：9天"
    if value == "·祈願後、<color=#D6C112>UR</color>戦姫破片を獲得した際の使用間隔上限：12日間":
        return "·祈愿后获得<color=#D6C112>UR</color>战姬碎片时的使用间隔上限：12天"
    if value == "·突破MAXにされたことのない<color=#AC54F7>SR</color>戦姫を1名保持する毎に、その戦姫を祈願壁に加えない場合、祈願壁の使用間隔 +04時間。<color=#D6C112>SSR</color>戦姫の場合は、祈願壁の使用間隔 +02時間。<color=#D6C112>UR</color>戦姫の場合は、祈願壁の使用間隔 +02時間":
        return "·每持有1名未达到突破MAX的<color=#AC54F7>SR</color>战姬，若不将该战姬加入祈愿墙，祈愿墙使用间隔+4小时。<color=#D6C112>SSR</color>战姬时，祈愿墙使用间隔+2小时。<color=#D6C112>UR</color>战姬时，祈愿墙使用间隔+2小时"
    if value == "1.大艦隊の順位報酬は基本報酬と報酬倍率の2つで構成される。実際の報酬＝基本報酬＊報酬倍率。\n2．報酬倍率は大艦隊参加者がイベントアイテムを寄付することでアップします。\n3．イベントが終わった後、大艦隊の順位報酬はメールから配布します。\n4．各ステージのミッションは開放時間内にのみクリアと受賞が可能です。\n5．資源消費段階では高強度再生合金板或はゲート接続維持器と資源を消費するミッションが開放されます。\n6．戦姫いの許願段階では、高級許願石を消費するミッションと戦姫レベルアップミッションが解放されます。\n7．戦姫共鳴段階では戦姫共鳴レベルアップと共闘ミッションが解放されます。\n8．レイドボス挑戦段階では累積ダメージミッションと海域演習記録を消費するミッションが開放します。":
        return "1.大舰队排名奖励由基础奖励和奖励倍率组成。实际奖励＝基础奖励＊奖励倍率。\n2．奖励倍率会因大舰队成员捐献活动道具而提升。\n3．活动结束后，大舰队排名奖励将通过邮件发放。\n4．各阶段任务只能在开放时间内完成并领取奖励。\n5．资源消耗阶段会开放消耗高强度再生合金板或网关连接维持器及资源的任务。\n6．战姬祈愿阶段会开放消耗高级祈愿石的任务与战姬升级任务。\n7．战姬共鸣阶段会开放战姬共鸣升级与协同作战任务。\n8．Raid Boss挑战阶段会开放累计伤害任务与消耗海域演习记录的任务。"
    if value == "9.祈願特注石とは、戦姫ごとに特注に製造された祈願石です。\n●使用方法：祈願壁を使用した際に任意の戦姫を獲得した際に、その獲得した戦姫の祈願特注石を所持している場合、祈願特注石を利用して、祈願壁の冷却時間を減少させる事ができます。（他戦姫の祈願特注石は対象外のため、使用する事はできません）例：SSR戦姫プリッツ・オイゲンを祈願壁で獲得した際に、プリッツ・オイゲンの祈願石、祈願石、上級祈願石の3種類の祈願石を使用する事ができます。":
        return "9.祈愿特制石是为每名战姬定制制造的祈愿石。\n●使用方法：使用祈愿墙获得任意战姬时，若持有该战姬的祈愿特制石，即可使用祈愿特制石减少祈愿墙冷却时间。（其他战姬的祈愿特制石不适用，无法使用。）例：通过祈愿墙获得SSR战姬欧根亲王时，可使用欧根亲王祈愿石、祈愿石和高级祈愿石这3种祈愿石。"
    if value == "10.ムーバー系戦姫を獲得した後に、祈願壁を使ってこの戦姫の<color=#f14949>万象の破片x30</color>を獲得する事ができます。祈願後、ムーバー系戦姫破片を獲得した際の使用間隔上限は<color=#f14949>12日間</color>になります。":
        return "10.获得搬运者系战姬后，可使用祈愿墙获得该战姬的<color=#f14949>万象碎片×30</color>。祈愿后，获得搬运者系战姬碎片时的使用间隔上限为<color=#f14949>12天</color>。"
    if value == "<color=#FFFF00> <size=28>%s</size></color>回入荷すると必ず<color=#FFE051> <size=28>SSR</size></color>装備を獲得できます":
        return "补货<color=#FFFF00> <size=28>%s</size></color>次后必定获得<color=#FFE051> <size=28>SSR</size></color>装备"
    if value == "今回の入荷は必ず<color=#FFE051> <size=28>SSR</size></color>装備を獲得できます":
        return "本次补货必定获得<color=#FFE051> <size=28>SSR</size></color>装备"
    if value == "出席表の達成によって、出席ポイントを獲得":
        return "完成出席表可获得出席点数"
    if value == "出席表ラインナップ":
        return "出席表奖励列表"
    if value == "每日00：00に更新する":
        return "每天00:00更新"
    if value == "指揮官さん、新学期を迎えて、おみくじ一本引いていかがですか？\n好運のおまけに、補給アイテムもいっぱいもらえますよ！":
        return "指挥官，迎来新学期，要不要抽一签？\n作为好运附赠，还能获得大量补给道具哦！"
    if value == "補給一覧":
        return "补给列表"
    if value == "毎日00:00にて更新されます":
        return "每天00:00更新"
    if value == "「シリアルコード使用上のご注意」\n1．一度使用になられたシリアルコードの再使用はできません\n2．同一ゲームアカウントに対して、コードの複数使用することはできません\n3．受け取られたプレゼントは、メールボックスで確認してください":
        return "“序列码使用须知”\n1．序列码使用一次后无法再次使用。\n2．同一游戏账号无法重复使用代码。\n3．收到的礼物请在邮箱中确认。"
    if value == "タップ<color=#FFB700FF>画面</ color>または<color =#FFB700FF>主砲</ color>ボタンをクリックして主砲を射撃できます。":
        return "点击<color=#FFB700FF>画面</ color>或<color =#FFB700FF>主炮</ color>按钮即可发射主炮。"
    prayer_wall = re.fullmatch(r"祈願壁の使用間隔が<color=(#[A-Fa-f0-9]+)>(%s)</color><color=(#[A-Fa-f0-9]+)>(延長されました|短縮されました)</color>", value)
    if prayer_wall:
        action = "延长" if prayer_wall.group(4) == "延長されました" else "缩短"
        color = prayer_wall.group(3)
        return f"祈愿墙使用间隔<color={prayer_wall.group(1)}>{prayer_wall.group(2)}</color><color={color}>{action}了</color>"
    if value == "<color=#FF0000FF>%s福袋コインを消費して</color>指揮本部の指令を解放しますか？":
        return "消耗<color=#FF0000FF>%s福袋硬币</color>解锁指挥部指令吗？"
    if value == "レベル%sに達成すると、下記の報酬を獲得できます。":
        return "达到等级%s后可获得以下奖励。"
    if value == "※ダイヤを消費する事で、ポイント獲得し、\nレベルアップする事ができます。":
        return "※消耗钻石即可获得点数，\n并提升等级。"
    if value == "※素早く全敵艦を撃沈する事で、豪華報酬を獲得する事ができます\n・編成ではイータ、ビスマルク、伊勢、日向、ベルファスト、アルジェリア、Z23、ヤウズ、綾波、アンソン、ブリュッヒャーの中から4名を選択する事ができます。\n・戦姫はレベルや装備が固定され、変更する事はできません。\n・敵全体が自己治療の能力を所持します。ご注意ください。":
        return "※快速击沉所有敌舰即可获得丰厚奖励。\n・编队时可从伊塔、俾斯麦、伊势、日向、贝尔法斯特、阿尔及利亚、Z23、雅乌兹、绫波、安森、布吕歇尔中选择4名。\n・战姬等级与装备固定，无法更改。\n・所有敌人都拥有自我治疗能力，请注意。"
    if value == "訓練説明\n1.敵空母：頻度の高い航空攻撃を発動します。\n2.敵空母：撃沈後、10秒後に復活します。この効果は一度の戦闘で最大1回まで発動可能。\n3.護衛原種：旗艦が攻撃された際に、そのダメージを旗艦の代わりに自分が受けます。復活する事ができません。":
        return "训练说明\n1.敌方航空母舰：发动频率较高的航空攻击。\n2.敌方航空母舰：被击沉后10秒复活。该效果每场战斗最多发动1次。\n3.护卫原种：旗舰受到攻击时，代替旗舰承受伤害。无法复活。"
    multi_select = re.fullmatch(r"使用する事で、(.+?)から任意1体を選択して、獲得する事ができます。", value)
    if multi_select:
        contents = multi_select.group(1)
        for source_term, target_term in sorted(SHIP_NAME_DIRECT.items(), key=lambda item: -len(item[0])):
            contents = contents.replace(source_term, target_term)
        if not JP_KANA_RE.search(contents):
            return f"使用后可从{contents}中任选1名并获得。"
    if value == "開けると下記の中から任意1体を入手できます。":
        return "打开后可从以下内容中任选1名获得。"
    if value == "使用する事で、「大破着せ替え選択箱①」＆「着せ替えチケット*98枚」から一つを選んで獲得することができます。":
        return "使用后可从“大破换装选择箱①”和“换装券*98张”中任选一项获得。"
    if value == "開けると、下記の戦姫の中から、選んだ１体を獲得できます。":
        return "打开后可从以下战姬中获得选定的1名。"
    if value == "中身はなーんだ！":
        return "里面是什么呢！"
    if value == "——ライザ-ディヴェルの抱擁":
        return "——莱莎·迪贝尔的拥抱"
    if value == "ライザ-ディヴェルの抱擁からのバレンタインプレゼント！":
        return "来自莱莎·迪贝尔的拥抱的情人节礼物！"
    if value == "ライザリン・シュタウトからのバレンタインプレゼント！":
        return "来自莱莎琳·斯托特的情人节礼物！"
    if value == "%sから%sへの挑戦をご招待します。[リンクをクリック]してチームに入ります。":
        return "邀请%s参加%s的挑战。点击[链接]加入队伍。"
    colored_ship_box = re.fullmatch(r"開けると<color=([^>]+)>SSR</color>戦姫（<color=([^>]+)>(.+?)</color>）から一つ選んで獲得できます", value)
    if colored_ship_box:
        contents = colored_ship_box.group(3)
        for source_term, target_term in sorted(SHIP_NAME_DIRECT.items(), key=lambda item: -len(item[0])):
            contents = contents.replace(source_term, target_term)
        contents = contents.replace("、", "、")
        if not JP_KANA_RE.search(contents):
            return f"打开后可从<color={colored_ship_box.group(1)}>SSR</color>战姬（<color={colored_ship_box.group(2)}>{contents}</color>）中任选一个获得"
    if value == "開けると資源x300000、艦隊コインx1500、勲功x3000、鋼材x2000、アルミx2000から一つ選んで獲得できます":
        return "打开后可从资源×300000、舰队硬币×1500、功勋×3000、钢材×2000、铝×2000中任选一项获得"
    special_stone = re.fullmatch(r"使用する事で、任意の戦姫の祈願特注石を(少量|中量)獲得できる。詳細は説明文をご確認ください。", value)
    if special_stone:
        amount = {"少量": "少量", "中量": "中量"}[special_stone.group(1)]
        return f"使用后可获得任意战姬的{amount}祈愿特制石。详情请查看说明。"
    if value == "使用後万象の破片＊30を獲得できます":
        return "使用后可获得万象碎片*30"
    selected = re.fullmatch(r"使用する事で、選んだ任意のSSR戦姫を１体獲得する事ができるスペシャルボックスです。", value)
    if selected:
        return "使用后可获得任意选定的1名SSR战姬。"
    selected = re.fullmatch(r"使用する事で、(.+?)から１体を選んで獲得することができます。", value)
    if selected:
        contents = selected.group(1)
        for source_term, target_term in sorted(SHIP_NAME_DIRECT.items(), key=lambda item: -len(item[0])):
            contents = contents.replace(source_term, target_term)
        contents = contents.replace("SSR戦姫", "SSR战姬").replace("の", "的")
        if not JP_KANA_RE.search(contents):
            return f"使用后可从{contents}中任选1名获得。"
    choice = re.fullmatch(r"使用後は(.+?)のかけらと(.+?)のかけらのどちらかを選ぶことができます", value)
    if choice:
        a = SHIP_NAME_DIRECT.get(choice.group(1), choice.group(1))
        b = SHIP_NAME_DIRECT.get(choice.group(2), choice.group(2))
        if not JP_KANA_RE.search(a + b):
            return f"使用后可在{a}碎片和{b}碎片中任选其一"
    choice = re.fullmatch(r"使用する事で、(.+?)から一つ選んで獲得できます", value)
    if choice:
        contents = choice.group(1)
        for source_term, target_term in sorted(SHIP_NAME_DIRECT.items(), key=lambda item: -len(item[0])):
            contents = contents.replace(source_term, target_term)
        contents = contents.replace("サミュエル・B・ロバーツ", "塞缪尔·B·罗伯茨")
        contents = contents.replace("の", "的")
        if not JP_KANA_RE.search(contents):
            return f"使用后可从{contents}中任选一个获得"
    choice = re.fullmatch(r"使用する事で、(.+?)から一つ選んで獲得する事ができます。?", value)
    if choice:
        contents = choice.group(1)
        for source_term, target_term in sorted(SHIP_NAME_DIRECT.items(), key=lambda item: -len(item[0])):
            contents = contents.replace(source_term, target_term)
        contents = contents.replace("サミュエル・B・ロバーツ", "塞缪尔·B·罗伯茨")
        contents = contents.replace("の", "的")
        if not JP_KANA_RE.search(contents):
            return f"使用后可从{contents}中任选一个获得"
    generic_choice = re.fullmatch(r"使用する事で、下記の(.+?)から任意(1つ|1体)を選択して、獲得する事ができます。?", value)
    if generic_choice:
        item_kind = {"ミニ戦姫": "迷你战姬", "戦姫": "战姬", "アイコン": "头像", "UR装備": "UR装备"}.get(generic_choice.group(1), generic_choice.group(1))
        count = "1名" if generic_choice.group(2) == "1体" else "1件"
        if not JP_KANA_RE.search(item_kind):
            return f"使用后可从以下{item_kind}中任选{count}。"
    generic_choice = re.fullmatch(r"使用する事で、下記のアイコンを獲得する事ができます。", value)
    if generic_choice:
        return "使用后可获得以下头像。"
    # Common item-description grammar. Apply only when all Japanese markers
    # disappear; this avoids counting partially translated text as complete.
    common_terms = {
        "使用する事で": "使用后",
        "使用することで": "使用后",
        "使用後は": "使用后",
        "開けると": "打开后可从",
        "から一つ選んで獲得できます": "中任选一个获得",
        "一つ選んで獲得できます": "任选一个获得",
        "から一つ選んで獲得する事ができます": "中任选一个获得",
        "から任意1体を選択して、獲得する事ができます": "中任选1名并获得",
        "から任意1つを選択して、獲得する事ができます": "中任选1件并获得",
        "から任意1体を選択して、獲得することができます": "中任选1名并获得",
        "から任意1つを選択して、獲得することができます": "中任选1件并获得",
        "選んだ任意のSSR戦姫を１体獲得する事ができる": "任意获得1名选定的SSR战姬",
        "選んだ任意のSSR装備を獲得できる": "任意获得选定的SSR装备",
        "下記の": "以下的",
        "の": "的",
        "戦姫": "战姬",
        "祈願石": "祈愿石",
        "かけら": "碎片",
        "欠片": "碎片",
        "装備": "装备",
        "アイコン": "头像",
        "スタンプ": "印章",
        "着せ替え": "换装",
        "選択箱": "选择箱",
        "ボックス": "箱",
        "パック": "礼包",
        "チケット": "兑换券",
        "限定版": "限定版",
        "限定": "限定",
        "または": "或",
        "イベント": "活动",
        "獲得できる": "可获得",
        "獲得する事ができます": "可以获得",
        "獲得することができます": "可以获得",
        "獲得できます": "可获得",
        "獲得する": "获得",
        "できます": "可以",
        "です": "是",
        "から": "从",
        "任意": "任意",
        "選択": "选择",
        "選んで": "选择",
        "購入": "购买",
        "報酬": "奖励",
        "記念": "纪念",
        "解放": "解锁",
        "開放": "开放",
        "戦闘": "战斗",
        "情報": "信息",
        "強化": "强化",
        "突破": "突破",
        "改造": "改造",
        "艦隊": "舰队",
        "大艦隊": "大舰队",
        "虹色": "彩虹色",
        "レア": "稀有",
        "超レア": "超稀有",
        "精選": "精选",
        "アイコンセット": "头像套装",
        "図鑑": "图鉴",
        "赤のコイン": "红色硬币",
        "コイン": "硬币",
        "燃料": "燃料",
        "から": "中",
    }
    generic = value
    for source_term, target_term in sorted(common_terms.items(), key=lambda item: -len(item[0])):
        generic = generic.replace(source_term, target_term)
    for source_term, target_term in sorted(SHIP_NAME_DIRECT.items(), key=lambda item: -len(item[0])):
        generic = generic.replace(source_term, target_term)
    generic = generic.replace("グローウォーム", "萤火虫").replace("ライザ", "莱莎")
    generic = generic.replace("サミュエル・Ｂ・ロバーツ", "塞缪尔·B·罗伯茨")
    generic = generic.replace("サミュエル・B・ロバーツ", "塞缪尔·B·罗伯茨")
    generic = generic.replace("アドミラル・ヒッパー", "希佩尔海军上将")
    generic = generic.replace("アドミラル・グラーフ・シュペー", "斯佩伯爵海军上将")
    generic = generic.replace("ヤウズ・スルタン・セリム", "雅乌兹·苏丹·塞利姆")
    generic = generic.replace("プリンス・オブ・ウェールズ", "威尔士亲王")
    generic = generic.replace("マクシム・ゴーリキー", "马克西姆·高尔基")
    generic = generic.replace("クイーン・エリザベス", "伊丽莎白女王")
    generic = generic.replace("サウスダコタ", "南达科他").replace("ノースカロライナ", "北卡罗来纳")
    generic = generic.replace("一つ", "一个").replace("１体", "1名").replace("1体", "1名")
    generic = generic.replace("x", "×")
    if generic != value and not JP_KANA_RE.search(generic):
        return generic
    color_match = COLOR_TEXT_RE.fullmatch(value)
    if color_match:
        inner = color_match.group(2)
        inner_map = {
            "左や右に旋回することで向きを変える": "向左或向右转向即可改变方向",
            "80秒の昼戦": "80秒昼战",
            "60秒の夜戦": "60秒夜战",
            "ボタンを押して射撃": "按下按钮射击",
            "小さなターゲットを当たるとより高いダメージが与えられる！": "命中小目标可以造成更高伤害！",
        }
        if inner in inner_map:
            return color_match.group(1) + inner_map[inner] + color_match.group(3)
        feature_map = {
            "装備エフェクト": "装备特效",
            "ドロップ率UP": "掉落率UP",
            "コラボ図鑑": "联动图鉴",
            "最強艦隊": "最强舰队",
            "ラボ": "实验室",
            "オート戦闘": "自动战斗",
            "シナリオ": "剧情",
            "デイリークエスト": "每日任务",
            "2倍速早送り": "2倍速快进",
            "3倍速早送り": "3倍速快进",
            "突破": "突破", "強化": "强化", "除隊": "退役", "作戦": "作战",
            "改造": "改造", "合成": "合成", "建造": "建造", "学院": "学院",
            "戦術": "战术", "訓練所": "训练所", "イベントシナリオ": "活动剧情",
            "大艦隊": "大舰队", "着せ替え": "换装", "敵魚雷発射動画スキップ": "跳过敌方鱼雷发射动画",
            "前哨基地": "前哨基地", "SBWB": "SBWB", "時間限定作戦": "限时作战",
            "ハロウィンイベント": "万圣节活动", "調合": "调合", "MultiPve": "多人PVE",
            "海域": "海域",
            "支援": "支援",
        }
        if inner in feature_map:
            return color_match.group(1) + feature_map[inner] + color_match.group(3)
    effect_start = re.fullmatch(r"(<color=[^>]+>)(装備エフェクト)(</color>)始動！", value)
    if effect_start:
        return f"{effect_start.group(1)}装备特效{effect_start.group(3)}启动！"
    level_unlock = re.fullmatch(
        r"指揮官レベル(-?\d+)(?:(?:に達すると|から)「(.+?)」が(?:開放|解放)されます。.*|で「(.+?)」を(?:開放|解放)できるので、.*)",
        value,
    )
    if level_unlock:
        raw_feature = level_unlock.group(2) or level_unlock.group(3)
        feature = {
            "ドロップ率UP": "掉落率UP", "ムーバー図鑑": "搬运者图鉴",
            "コラボ図鑑": "联动图鉴", "最強艦隊": "最强舰队",
            "ラボ": "实验室", "指令イベント（スペシャル）": "指令活动（特别）",
            "世界事件": "世界事件",
            "特殊訓練": "特殊训练",
            "調合": "调合",
        }.get(raw_feature, raw_feature)
        return f"达到指挥官等级{level_unlock.group(1)}后将解锁“{feature}”。请努力提升等级！"
    colored_unlock = re.fullmatch(
        r"(<color=[^>]+>)(.+?)(</color>)(?:機能)?が開放(?:されました|された)！", value
    )
    if colored_unlock:
        feature = {
            "ドロップ率UP": "掉落率UP", "コラボ図鑑": "联动图鉴",
            "最強艦隊": "最强舰队", "ラボ": "实验室", "オート戦闘": "自动战斗",
            "シナリオ": "剧情", "海域": "海域", "支援": "支援", "デイリークエスト": "每日任务",
            "2倍速早送り": "2倍速快进", "3倍速早送り": "3倍速快进", "突破": "突破",
            "強化": "强化", "除隊": "退役", "作戦": "作战", "改造": "改造", "合成": "合成",
            "建造": "建造", "学院": "学院", "戦術": "战术", "訓練所": "训练所",
            "イベントシナリオ": "活动剧情",
            "イベント海域": "活动海域", "クリア評価訓練所": "通关评价训练所",
            "訓練所：AR海戦": "训练所：AR海战", "シナリオ・テロップ": "剧情·字幕",
            "訓練所・テロップ": "训练所·字幕", "挑戦：物資争奪戦": "挑战：物资争夺战",
            "味方スキル発動動画スキップ": "跳过我方技能动画", "敵スキル発動動画スキップ": "跳过敌方技能动画",
            "スキル効果動画スキップ": "跳过技能效果动画", "装備解体": "装备分解",
            "最終決戦": "最终决战", "挑戦": "挑战", "挑戦：ムーバー防衛線": "挑战：搬运者防卫线",
            "陣形変更": "阵型变更", "大艦隊": "大舰队", "着せ替え": "换装",
            "敵魚雷発射動画スキップ": "跳过敌方鱼雷发射动画", "前哨基地": "前哨基地",
            "SBWB": "SBWB", "時間限定作戦": "限时作战", "ハロウィンイベント": "万圣节活动",
            "調合": "调合", "MultiPve": "多人PVE",
            "特殊訓練": "特殊训练", "イベント海域": "活动海域", "クリア評価訓練所": "通关评价训练所",
            "訓練所：AR海戦": "训练所：AR海战", "シナリオ・テロップ": "剧情·字幕",
            "訓練所・テロップ": "训练所·字幕", "挑戦：物資争奪戦": "挑战：物资争夺战",
            "味方スキル発動動画スキップ": "跳过我方技能动画", "敵スキル発動動画スキップ": "跳过敌方技能动画",
            "スキル効果動画スキップ": "跳过技能效果动画", "装備解体": "装备分解",
            "最終決戦": "最终决战", "挑戦：ムーバー防衛線": "挑战：搬运者防卫线", "陣形変更": "阵型变更",
        }.get(colored_unlock.group(2))
        if feature:
            return f"{colored_unlock.group(1)}{feature}{colored_unlock.group(3)}功能已解锁！"
    colored_start = re.fullmatch(r"(<color=[^>]+>)(.+?)(</color>)機能始動！", value)
    if colored_start:
        feature = {
            "ハロウィンイベント": "万圣节活动",
        }.get(colored_start.group(2))
        if feature:
            return f"{colored_start.group(1)}{feature}{colored_start.group(3)}功能启动！"
    world_unlock = re.fullmatch(
        r"(<color=[^>]+>)世界事件 「迷い込む」(</color>)をクリアすると開放されます！", value
    )
    if world_unlock:
        return f"完成{world_unlock.group(1)}世界事件“误入”{world_unlock.group(2)}后即可解锁！"
    bath_unlock = "「基地」が開放されると「風呂場」が使用できます。"
    if value == bath_unlock:
        return "解锁“基地”后即可使用“浴室”。"
    resonance = re.fullmatch(r"任意キャラクターの共鳴を(\d+)回レベルアップします", value)
    if resonance:
        return f"将任意角色的共鸣提升等级{resonance.group(1)}次"
    records = re.fullmatch(r"合同演習記録(\d+)個を消費します", value)
    if records:
        return f"消耗{records.group(1)}个联合演习记录"
    sea_records = re.fullmatch(r"海域演習記録(\d+)個を消費します", value)
    if sea_records:
        return f"消耗{sea_records.group(1)}个海域演习记录"
    coop = re.fullmatch(r"共闘を(\d+)回挑戦します", value)
    if coop:
        return f"挑战协同作战{coop.group(1)}次"
    fuel = re.fullmatch(r"燃料x(\d+)を消費する", value)
    if fuel:
        return f"消耗燃料x{fuel.group(1)}"
    raid_damage = re.fullmatch(r"レイドボスへ累積ダメージ(\d+万)を与えます", value)
    if raid_damage:
        return f"对Raid Boss造成累计伤害{raid_damage.group(1)}"
    prayer_stones = re.fullmatch(
        r"祈願石（上級祈願石、専属祈願石含む）x(\d+)を消費する", value
    )
    if prayer_stones:
        return f"消耗祈愿石（包括高级祈愿石、专属祈愿石）x{prayer_stones.group(1)}"
    soda = re.fullmatch(r"オースソーダを(\d+)回使用する", value)
    if soda:
        return f"使用奥斯苏打水{ soda.group(1) }次"
    purchase = re.fullmatch(r"(.+)\nこの商品を購入後、倉庫に収納されます", value)
    if purchase:
        contents = purchase.group(1)
        for source_term, target_term in {
            "燃料": "燃料", "ブルーチップ": "蓝色芯片", "上級祈願石": "高级祈愿石",
            "超高濃度オースソーダ": "超高浓度奥斯苏打水",
        }.items():
            contents = contents.replace(source_term, target_term)
        return f"{contents}\n购买此商品后会存入仓库"
    coin_pack = re.fullmatch(
        r"使用する事で、艦隊コイン\*(\d+)と艦隊コインII\*(\d+)を獲得する事ができます。", value
    )
    if coin_pack:
        return f"使用后可获得舰队硬币*{coin_pack.group(1)}和舰队硬币II*{coin_pack.group(2)}"
    cuisine = re.fullmatch(r"美食マスターの道イベントの出来上がり料理。(.+?)[！!]美味しそうな一品だ！", value)
    if cuisine:
        ingredients = cuisine.group(1)
        for source_term, target_term in {
            "さつまいも": "红薯", "さんま": "秋刀鱼", "まつたけ": "松茸", "玄米": "糙米",
        }.items():
            ingredients = ingredients.replace(source_term, target_term)
        return f"美食大师之路活动成品料理。{ingredients}！看起来真好吃！"
    resource_bundle = value
    for source_term, target_term in {
        "物資": "物资", "燃料": "燃料", "上級祈願石": "高级祈愿石",
        "高濃度オースソーダ": "高浓度奥斯苏打水", "超高濃度オースソーダ": "超高浓度奥斯苏打水",
    }.items():
        resource_bundle = resource_bundle.replace(source_term, target_term)
    if resource_bundle != value and not JP_KANA_RE.search(resource_bundle):
        return resource_bundle
    random_ship_box = re.fullmatch(r"開けると、SSR(空母|巡洋艦)からランダムに1体入手する事ができます。", value)
    if random_ship_box:
        ship_type = "航空母舰" if random_ship_box.group(1) == "空母" else "巡洋舰"
        return f"打开后可从SSR{ship_type}中随机获得1名。"
    alloy = re.fullmatch(r"高強度再生合金板或は、ゲート接続維持器或を(\d+)個消費する", value)
    if alloy:
        return f"消耗{alloy.group(1)}个高强度再生合金板或网关连接维持器"
    area_unlock = re.fullmatch(
        r"海域調査をクリアすると(<color=[^>]+>)(.+?)(</color>)開放されます！", value
    )
    if area_unlock:
        return f"完成海域调查后解锁{area_unlock.group(1)}{area_unlock.group(2)}{area_unlock.group(3)}！"
    formation_terms = {
        "駆逐艦": "驱逐舰", "軽巡洋艦": "轻巡洋舰", "重巡洋艦": "重巡洋舰",
        "巡洋戦艦": "战列巡洋舰", "戦艦": "战列舰", "錬金術士": "炼金术士",
    }
    if any(term in value for term in formation_terms):
        replaced = value
        for source_term, target_term in sorted(formation_terms.items(), key=lambda item: -len(item[0])):
            replaced = replaced.replace(source_term, target_term)
        if replaced != value and not JP_KANA_RE.search(replaced):
            return replaced
    timing_color = re.fullmatch(r"当海域での戦闘は(<color=[^>]+>)(\d+)秒の(昼戦|夜戦)(</color>)です", value)
    if timing_color:
        phase = "昼战" if timing_color.group(3) == "昼戦" else "夜战"
        return f"本海域战斗为{timing_color.group(1)}{timing_color.group(2)}秒{phase}{timing_color.group(4)}"
    generic_terms = {
        "してください": "请", "ください": "请", "できます": "可以", "できる": "能",
        "できない": "不能", "しましょう": "一起吧", "しました": "了", "します": "进行",
        "している": "正在", "して": "进行", "について": "关于", "として": "作为",
        "から": "从", "まで": "到", "だけ": "只", "もう": "已经", "また": "再次",
        "とても": "非常", "すこし": "稍微", "そして": "然后", "でも": "但是", "しかし": "但是",
        "だから": "所以", "ここ": "这里", "そこ": "那里", "これ": "这个", "それ": "那个",
        "あの": "那个", "あなた": "你", "わたし": "我", "みんな": "大家", "敵": "敌人",
        "味方": "我方", "戦闘": "战斗", "攻撃": "攻击", "ダメージ": "伤害", "耐久": "耐久",
        "獲得": "获得", "消費": "消耗", "購入": "购买", "使用": "使用", "解放": "解锁",
        "必要": "需要", "可能": "可能", "成功": "成功", "失敗": "失败", "報酬": "奖励",
        "条件": "条件", "限定": "限定", "イベント": "活动", "任務": "任务", "毎日": "每日",
        "戦姫": "战姬", "艦隊": "舰队", "艦": "舰", "砲撃": "炮击", "魚雷": "鱼雷",
        "主砲": "主炮", "副砲": "副炮", "火力": "火力", "命中": "命中", "回避": "回避",
        "上昇": "提升", "減少": "减少", "追加": "追加", "発動": "发动", "回復": "恢复",
        "最大": "最多", "場合": "时", "時": "时", "回": "次", "個": "个", "名": "名",
    }
    generic_value = value
    for source_term, target_term in sorted(generic_terms.items(), key=lambda item: -len(item[0])):
        generic_value = generic_value.replace(source_term, target_term)
    if generic_value != value and not JP_KANA_RE.search(generic_value):
        return generic_value
    if value == "残念だけど、そういう漠然とした調査方法なら、付き合うことはできないわ。\n…あなたたちの幸運を祈るしかないでしょう。":
        return "很遗憾，如果是那种模糊的调查方法，我无法奉陪。\n……只能祝你们好运了。"
    if value == "…お気遣い感謝します。\n約束は必ず守るので、その点についてはご安心を。":
        return "……感谢你的关心。\n我一定会遵守约定，这点请放心。"
    if value == "もし私たちがお力になれる時が来たら、\nいつでも連絡をください。":
        return "如果有一天我们能帮上忙，\n请随时联系。"
    if value == "束の間とは言え…こうしてムーバーの人間と共に行動することになるなんて、\n貴重な経験だったと言えるでしょう。":
        return "虽然时间短暂……能这样和搬运者的人一起行动，\n可以说是一次宝贵的经历。"
    if value == "うむ。\nそれで、次は…":
        return "嗯。\n那么，接下来……"
    if value == "へっ！へっ！へっ！\nげき〜しゃ〜☆！！":
        return "嘿！嘿！嘿！\n猛烈～射击～☆！！"
    if value == "料理はすでに真っ赤に…\nサンディエゴさん、すごい！すごいですねサンディエゴさん！":
        return "料理已经变得通红……\n圣地亚哥小姐，厉害！真厉害啊，圣地亚哥小姐！"
    if value == "何事もないようにこんなに唐辛子を撒くなんて、実に敬服すべき気迫！\nこれほど辛さに強くなるために、きっと無数の修羅場をくぐり抜けてきたのですね？":
        return "若无其事地撒这么多辣椒，真是令人敬佩的气势！\n为了变得如此耐辣，你一定经历过无数修罗场吧？"
    if value == "心に激射☆の意志が宿っていれば、この程度はちょろいもんだよ。\nまだまだ辛さが全然足りないぞ〜":
        return "只要心中怀有猛烈射击☆的意志，这点程度根本不算什么。\n辣度还远远不够呢～"
    if value == "なんなら、エセックスも一口どう？\n赤く見えてるだけで、実は全然辛くないからさ～":
        return "要不然，埃塞克斯也来一口？\n只是看起来红，其实一点都不辣～"
    if value == "よろしいですか？\nぜひ試してみたいと思います！":
        return "可以吗？\n我非常想试试看！"
    if value == "よ〜し！そう来なくっちゃ！\n行くよ、激射——":
        return "好～！就该这样！\n上吧，猛烈射击——"
    if value == "待って待って待って待って！\nその唐辛子は本当に辛いから！慎重にしてくれ！！":
        return "等等等等！\n那个辣椒真的很辣！小心一点！！"
    if value == "仮にもオークランドのいとこだから、\n同じように辛さには強いんでしょう…":
        return "毕竟也是奥克兰的表亲，\n应该同样很能吃辣吧……"
    if value == "っていうか空気読みなさい！\n人が真剣な話をしてるのに、食事の話ばかりしてないで！":
        return "话说你能不能看点气氛！\n别人正在认真谈话，别只顾着聊吃饭！"
    if value == "サンディエゴさんはさておいて。\nエセックス、あなたまでこんなに無神経になったの？":
        return "先不说圣地亚哥小姐。\n埃塞克斯，连你也变得这么不体贴了吗？"
    if value == "叱られちゃいました…\n本当は褒められてもいいのに～～～":
        return "被训了……\n明明应该被夸奖才对～～～"
    if value == "こうして異世界で出会いましたから、張り詰めた空気より、やはり友好的になった方がいいですよね？\nだからこうして場を和ませているんですよ？":
        return "既然我们这样在异世界相遇，比起紧张的气氛，果然还是友好相处更好吧？\n所以我才这样缓和气氛啊？"
    if value == "優しいな、よくやってくれた！":
        return "真温柔，做得好！"
    if value == "わあ〜！ありがとうございます！\n話のわかる指揮官のようで、気が合いそうですね！":
        return "哇～！谢谢你！\n看来你是个通情达理的指挥官，我们应该很合得来！"
    if value == "あのさ！\n「サンディエゴさんはさておいて」ってどゆことよ〜！":
        return "那个！\n“先不说圣地亚哥小姐”是什么意思啊～！"
    if value == "「一度会っただけの間柄だけど、あなたは自由気ままな方だと気付いた」\nっていう意味だわ。":
        return "意思是“虽然我们只见过一次，但我发现你是个自由随性的人”。"
    if value == "なるほど。\nうむ！その認識は間違っていない。":
        return "原来如此。\n嗯！这个认识没错。"
    if value == "それにしても…エリザベスさんの人脈はすごいですね。\n全員に面識があるなんて。":
        return "话说回来……伊丽莎白小姐的人脉真厉害。\n竟然和所有人都认识。"
    if value == "エセックスとそっちのボルチモアさんは、\n以前接触したことがあるわ。":
        return "我以前和埃塞克斯以及那边的巴尔的摩小姐接触过。"
    if value == "そしてオークランドのこのいとこ、サンディエゴさんについては、\n以前彼女がオークランドの見舞いのためにパールベイに来た時に一度会っただけ。":
        return "至于奥克兰的这位表亲圣地亚哥小姐，\n以前她来珍珠湾探望奥克兰时，我只见过她一次。"
    if value == "えっ？\nそんなことがあったの？":
        return "诶？\n发生过那种事吗？"
    if value == "だいぶ昔のことだわ。\n夕立はまだうちに来てなかったし、神通もちょうどパトロールで外出してたから…":
        return "那是很久以前的事了。\n那时夕立还没来我们这里，神通也正好外出巡逻……"
    if value == "…って、またあなたたちのせいで話が脱線したじゃない。":
        return "……看吧，话题又因为你们而跑题了。"
    if value == "コホン、今、あなたたちと一緒にいるのはブルースフィアの指名手配犯よ。\nさて、どうするつもり？":
        return "咳咳，现在和你们在一起的是蓝星的通缉犯。\n那么，你们打算怎么办？"
    if value == "どうする～？\nボルチモア。":
        return "怎么办～？\n巴尔的摩。"
    if value == "どうします～？\nボルチモア～":
        return "怎么办～？\n巴尔的摩～"
    if value == "あなたたちはね、全部私に投げ出すのか？\n…まあ、これも隊長としての勤めか。":
        return "你们要把一切都丢给我吗？\n……算了，这也是作为队长的职责吧。"
    if value == "どうするって聞かれてもね…どうしようもありませんよ。":
        return "就算问我怎么办……我也无能为力啊。"
    if value == "そっちは人数が勝ってる上に、テーテューアスまでついている。\nで、こっちはたっだの3人。":
        return "你们不仅人数占优，还有特修斯在身边。\n而我们这边只有区区3个人。"
    if value == "頭がいかれてない限り、あなたたちを攻撃したりするわけがないでしょう。\nそれに、そもそも私たちの任務も別にあなたたちの逮捕なわけじゃないの。":
        return "只要脑子没坏，就不可能攻击你们吧。\n而且，我们的任务本来就不是逮捕你们。"
    if value == "さらにいうと、軍はあなたたちのことを\n「戦姫の武力を乱用するテロリストおよびその配下の戦姫」って認定したけど…":
        return "再说，军方已经认定你们是\n“滥用战姬武力的恐怖分子及其麾下战姬”……"
    if value == "よ〜〜〜し！そうと決まったら、歓迎会をやろう！\n今デザートを用意してくる！":
        return "好～～～！既然决定了，就来办欢迎会吧！\n我现在去准备甜点！"
    if value == "デザート！":
        return "甜点！"
    if value == "——ムーバーの二人も、よければ一緒にどうだ？昔はまあ、いざこざがあったけど…\nこの数日間の付き合いで、悪いやつじゃないってことはわかったし。":
        return "——搬运者的两位，如果愿意也一起来吧？以前确实发生过一些摩擦……\n但相处这几天后，我知道你们不是坏人。"
    if value == "残りわずかの期間、仲良くいこう。\n後で部屋で治療を受けているチビっこにもデザートを届けてあげてくれ。":
        return "剩下的时间不多了，大家好好相处吧。\n之后也给正在房间接受治疗的小不点送些甜点。"
    if value == "いいアイデアだな":
        return "好主意。"
    if value == "この場で断るのは…さすがにナンセンスね。\nそれでは、遠慮なくお言葉に甘えさせていただきます。":
        return "在这里拒绝……确实不合情理。\n那么，我们就不客气地接受你的好意了。"
    if value == "食った食った〜！\n新人が来たおかげでいっぱい食べられた。もっと新人が来ないかな？":
        return "吃饱啦～！\n多亏来了新人，我才能吃这么多。要是再来些新人就好了？"
    if value == "食料品の備蓄が一気に減った…\n明日からはもっと遠い海域に行って魚を獲らなければ…":
        return "食物储备一下子减少了……\n从明天起必须去更远的海域捕鱼了……"
    if value == "後片付けは私がやりますので、\n皆様は先に休んでてください。":
        return "我来收拾残局，\n大家先休息吧。"
    if value == "私たちも手伝おう。":
        return "我们也来帮忙吧。"
    if value == "…じゃあ、私も…":
        return "……那我也……"
    if value == "わあ！\n":
        return "哇！\n"
    if value == "サンディエゴ姉さん…":
        return "圣地亚哥姐姐……"
    if value == "…ふん。":
        return "……哼。"
    if value == "誰が障害よ！\nせっかく助けに来たのに、失礼じゃないかしら！？":
        return "谁是障碍啊！\n我好不容易来帮忙，你这样说不觉得失礼吗！？"
    if value == "この声は…エリザベス！":
        return "这个声音是……伊丽莎白！"
    if value == "素晴らしいです——！サンディエゴ、見て見て！\nわたくしたちが船なのに、今船を乗っていますよ！":
        return "太棒了——！圣地亚哥，快看快看！\n明明我们就是船，现在却坐在船上哦！"
    if value == "エセックスにボルチモア！\nあなたたちもいるんだね！":
        return "埃塞克斯和巴尔的摩！\n你们也在这里啊！"
    if value == "みんな、世間話は後にして頂戴！\n下の三人もさっさと乗って、逃げちゃうよ～☆":
        return "大家先别闲聊了！\n下面的三个人也快点上来，我们要逃啦～☆"
    if value == "早く来い！！":
        return "快过来！！"
    if value == "…うん！\n今行くよ！":
        return "……嗯！\n这就来！"
    if value == "ヒィィィィィイ～～！ ？ ！ ？ ":
        return "咿呀呀呀呀呀～～！？！？ "
    if value == "こちらMC13-0426-0101、異常なし。":
        return "这里是MC13-0426-0101，一切正常。"
    if value == "やつらは行った。":
        return "他们走了。"
    if value == "うわ…緊張した。":
        return "哇……真紧张。"
    if value == "こっそり…":
        return "偷偷地……"
    if value == "こっそり……":
        return "偷偷地……"
    if value == "こっそりと………":
        return "偷偷地……"
    if value == "はいはい。 この前誘った時は予定が詰まっているから来られるかどうか分からないとか言っていたのに、 結局やはり学園のことが気掛かりなのね。 ":
        return "好啦好啦。上次邀请你时，你还说日程很满、不知道能不能来，结果果然还是放心不下学园呢。 "
    if value == "忙しいのは本当よ。 投資なんて、 庶民などが考えているような、 \n金をあちこちに振り撒けばいい話じゃないから。 ":
        return "我确实很忙。投资并不是像平民想的那样，\n只要把钱到处撒出去就行。 "
    if value == "別にパールベイ学園が好きなわけではないが、 価値のある人脈を得たのは確かなのよ。 \n私は恩知らずの女じゃないわ。 ":
        return "我并不是喜欢珍珠湾学园，但确实在那里结识了有价值的人脉。\n我不是忘恩负义的女人。 "
    if value == "ふふ、 そうね。 卒業して何年も経ったのに、 まだしばしば集まっては 「最近何キロも太ったのよ」 \nみたいな話題で盛り上がる人脈は、 確かに相当価値の高いものだね。 ":
        return "呵呵，是啊。毕业多年后还经常聚在一起，\n围绕“我最近胖了几公斤”之类的话题热闹聊天的人脉，确实相当有价值。 "
    if value == "日向さんは普段ちょっと近寄りがたい感じがしますが、 なんだかんだ言って、 \n基地のみんなのことをちゃんと見ているんですね。 ":
        return "日向小姐平时让人有点难以接近，但说到底，\n她一直在认真关注基地里的大家呢。 "
    if value == "ふふ、 どうやらジョージ5世の心配は無用だった。 \nそれに越したことはない。 ":
        return "呵呵，看来不用担心乔治五世了。\n这再好不过。 "
    if value == "それは、 どういうことでしょうか……？ ":
        return "那是什么意思……？ "
    if value == "いや、 気にしないでくれ。 \nプレゼントは全部買ったんだから、 もう帰るとしようか。 ":
        return "不，别在意。\n礼物都买好了，我们回去吧。 "
    if value == "みんな……と言うのは？ ":
        return "你说的大家……是指？ "
    if value == "そうですね。 基地に残っている伊勢さんチームの方がどうなっているのか、 すこし気になりますね。 ":
        return "是啊。我有点在意留在基地的伊势小姐那一队现在怎么样了。 "
    if value == "クリスマスパーティーの準備のやり直しは伊勢さんに任されました。 \nふざけていない時はとても頼もしい人ですが、 夜まであまり時間がありません。 ":
        return "重新准备圣诞派对的工作交给了伊势小姐。\n她不胡闹时非常可靠，但距离晚上已经没多少时间了。 "
    if value == "こんなに短い時間で、 本当に準備を整えられるのでしょうか……？ ":
        return "这么短的时间，真的能准备好吗……？ "
    if value == "ハロウィーンに乗じて、 とんでもないことを企むいたずらっ子は、 \nこの基地にはごまんといますから。 ":
        return "趁着万圣节策划荒唐恶作剧的淘气鬼，\n在这个基地里多得很。 "
    if value == "その子たちを自由にさせたら、 \n何をしでかすのか分かるもんじゃありません。 ":
        return "要是放任那些孩子，\n谁知道她们会做出什么事。 "
    if value == "今年はみんなで用意したキャンディーを鼻くそ味のものにすり替えようとした悪ガキは\n既に何人も捕まりましたが、 残りの者がまた他のイタズラをするかもしれません。 ":
        return "今年已经抓住了好几个想把大家准备的糖果换成鼻屎味糖果的坏孩子，\n但剩下的孩子可能还会搞别的恶作剧。 "
    if value == "うるさいわね！ \n無駄口叩かないで、 早くいくのよ！ ":
        return "吵死了！\n别废话，快走！ "
    if value == "え！ ？ リザ歩くの早くない？ \nこっちは毎日オフィスワークなのよ？ 足がついていかないの！ ":
        return "诶！？莱莎你走得也太快了吧？\n我每天都坐办公室，腿跟不上啊！ "
    if value == "はぁ、 はぁ……\nよし、 ここまでくれば、 お化けもついてこないよね。 ":
        return "哈啊，哈啊……\n好，到了这里，幽灵应该不会跟来了。 "
    if value == "やれやれ、 スカウトされて国民アイドルになるのは超正統派な乙女の夢のはずなのに、 \n実際にあったらこんなことになるなんて。 ":
        return "真是的，被星探发掘成为国民偶像，本该是正统少女的梦想，\n没想到真的发生后竟然会变成这样。 "
    if value == "あっ、 ネコちゃんだ。 \n白い靴下履いているね、 かわいい。 どこの家のでしょう？ ":
        return "啊，是小猫。\n穿着白袜子，好可爱。是哪个家的呢？ "
    if value == "……少女よ、 一日中逃げ続けているじゃないか。 ":
        return "……少女啊，你不是逃了一整天吗。 "
    if value == "ネコがしゃべった！ ？ \nしかもすっごく渋いおっさんみたいな声で！ ？ ":
        return "猫说话了！？\n而且还是像沉稳大叔一样的声音！？ "
    if value == "まあ。 ネコの寿命は人よりずっと短い。 \nいつの間にか、 年をとっているんだ。 ":
        return "是啊。猫的寿命比人短得多。\n不知不觉就会老去。 "
    if value == "あ……確かにそうね。 ":
        return "啊……确实如此。 "
    if value == "……悪いな。 昼の日差しを楽しんでいる時に慌てているあんたを見たから、 つい声をかけた。 \n困らせているならこれで失礼するよ。 ":
        return "……抱歉。我看到你在享受白天阳光时慌慌张张的，就忍不住搭话。\n如果给你添麻烦了，那我先告辞。 "
    if value == "あの立派なクリスマスツリーはロドニーさんが自分の部屋用のやつを貸してくれたの。 \nあれがないとクリスマスパーティーとは言えないようなものだから、 ほんと助かった！ ":
        return "那棵漂亮的圣诞树是罗德尼小姐借给我们的，本来是她房间用的。\n没有它就不能算圣诞派对，真是帮大忙了！ "
    if value == "なるほど……では、 テーブルを埋めつくすこれらの豪華な料理も、 \nみんなが持ってきてくれたのですか？ ":
        return "原来如此……那么，摆满桌子的这些豪华料理，\n也是大家带来的吗？ "
    if value == "ざっと見ると、 ショートケーキにクッキー、 アップルパイ、 ラザニア、 ドーナツ、 ローストチキン……\n色とりどりの料理が揃っていますね。 ":
        return "大致看去，有奶油蛋糕、曲奇、苹果派、千层面、甜甜圈、烤鸡……\n各种各样的料理都准备齐了呢。 "
    if value == "えへへ、 そのとおり！":
        return "嘿嘿，正是如此！"
    if value == "料理の腕に自信がある人と、 おいしいものをみんなと一緒に食べたい人に声を掛けて、 \nそれぞれ一品ずつ持ってきてもらったら、 いつの間にかテーブルいっぱいになってしまった！ ":
        return "我邀请了擅长料理的人，以及想和大家一起吃美食的人，\n请每人带来一道菜，不知不觉就摆满了桌子！ "
    if value == "他の仕事もみんなに任せているよ。 盛り上げるためのゲームを考える役とか、 司会とか、 撮影担当とか。 \nやりたい人！ って聞いたら、 いっぱい手を挙げた人がいたから。 ":
        return "其他工作也交给大家了，比如设计活跃气氛的游戏、主持和摄影。\n我问“谁想做”，结果有很多人举手。 "
    if value == "ついでに言うと、そのゲームを考える役はあたしとグリッドレイで、 \n司会はノースカロライナさんとハーミーズで、 撮影担当は愛宕と摩耶だよ。 ":
        return "顺便说一下，游戏由我和格里德利设计，\n北卡罗来纳小姐和赫尔墨斯主持，爱宕和摩耶负责摄影。 "
    if value == "つまり、 こいつは担当者って振舞っているけど、 \n実際にやっているのは放送で助っ人を呼ぶことだけだよ。 ":
        return "也就是说，这家伙虽然摆出负责人的样子，\n实际做的只有通过广播叫来帮手。 "
    if value == "ずるい奴だね～":
        return "真狡猾啊～"
    if value == "人の力を借りるのもリーダーの仕事だよ！ ":
        return "借助他人的力量也是领导者的工作！ "
    if value == "いいえ！ そんな、 困ってなど……！ むしろネコと話せるって\n千載一遇のチャンスじゃないですか？ もう二度とないかもしれないから、 大切にしないと！ ":
        return "不！我才没有困扰……！倒不如说，能和猫说话不是千载难逢的机会吗？\n可能再也不会有第二次，所以一定要珍惜！ "
    if value == "でもネコのおじさんと何を話せばいいのかはさっぱりだよね。 \n年の差に加えて種族も違うもんね、 難易度 「地獄」 だわ……":
        return "不过完全不知道该和猫大叔聊什么呢。\n不但年龄相差悬殊，连种族都不同，难度是“地狱”啊……"
    if value == "こういう時は、 初心に戻ればいいさ。 \n少女よ、 学校に行かず、 ここまで慌ててやってきたのは、 一体何があったんだ？ ":
        return "这种时候，回到初心就好。\n少女啊，你没去上学还慌慌张张跑到这里，到底发生了什么？ "
    if value == "別に隠すことではないけど……長くなるから簡単にまとめると\nあんなことやこんなことがあったから、 はい説明完了。 ":
        return "也没什么好隐瞒的……不过说来话长，简单总结就是\n发生了这样那样的事，好，解释完毕。 "
    if value == "なるほど、 学園のイベントと将来の道、 \nあんたはどっちからも逃げたのか。 ":
        return "原来如此，学园活动和未来的道路，\n你两边都逃避了吗。 "
    if value == "うぅ……面目ない……":
        return "呜……无地自容……"
    if value == "謝ることはない。 \n逃げるのは別に誇らしいことじゃないが、 役には立つ。 ":
        return "不用道歉。\n逃跑虽然不值得骄傲，但有时确实有用。 "
    if value == "いやいや、 こういう時年長者として、 \n普通 「元気を出して真面目に向き合え！ 」 とか言うでしょう？ ":
        return "不不，作为年长者，这种时候通常会说“打起精神认真面对吧！”之类的话吧？ "
    if value == "オレはただの通りすがりのおっさんだ。 \n話に付き合ったり、 自分の知っていることを話したりする、 それだけだ。 ":
        return "我只是个路过的大叔。\n陪你聊聊天，说些自己知道的事，仅此而已。 "
    if value == "オレが知っているのは、 何かをやるって決めると、 \nそのあとは必ず 「なぜもっとうまくやれなかった」 と後悔することだけだ。 ":
        return "我所知道的只有：一旦决定做某件事，\n之后一定会后悔“为什么没能做得更好”。 "
    if value == "さすがお姉ちゃんですね。 \n私の心配はいらないようですね。 ":
        return "不愧是姐姐。\n看来不用担心你了。 "
    if value == "そっちはどう？ 順調？ ":
        return "你那边怎么样？顺利吗？ "
    if value == "はい、 皆さんへのクリスマスプレゼントは全部無事購入できました。 ":
        return "是的，给大家的圣诞礼物都顺利买好了。 "
    if value == "日向さんが選んだプレゼントはどれもとても素敵です。 \n皆さんはきっと喜んでくれると思います！ ":
        return "日向小姐选的每件礼物都很棒。\n大家一定会喜欢的！ "
    if value == "瑞鶴さん！ ？ ":
        return "瑞鹤小姐！？ "
    if value == "瑞鶴！ ？ \nちょっと、 瑞鶴！ ":
        return "瑞鹤！？\n喂，瑞鹤！ "
    if value == "はっ！ ？ \nここは？ ":
        return "哈！？\n这里是？ "
    if value == "あぁ、 ようやく目が覚めましたね。 ":
        return "啊，你终于醒了。 "
    if value == "瑞鶴！ おはよう！ ":
        return "瑞鹤！早上好！ "
    if value == "やると決めたことが多いほど、 後悔することも多くなる。 ":
        return "决定要做的事情越多，后悔也会越多。 "
    if value == "それに比べると、 何もしなければ、 後悔することは一つだけで済む。 ":
        return "相比之下，如果什么都不做，就只会后悔一件事。 "
    if value == "……それは、 なにを？ ":
        return "……那是什么？ "
    if value == " 「何もしなかった」 。 ":
        return "“什么都没做”。 "
    if value == "逃げる":
        return "逃跑"
    if value == "……少女よ、 今あんたに任せたい大事なことがあるんだが、 受け入れるか？ \nそれとも、 また逃げるを選ぶか？ ":
        return "……少女啊，现在有件重要的事想交给你，你愿意接受吗？\n还是要再次选择逃跑？ "
    if value == "若者が迷っている時に、 その背中を押してやるのも年長者としての責任だ。 ":
        return "年轻人迷茫时推他们一把，也是年长者的责任。 "
    if value == "盲目的にその背後の合理性を疑ったり、 他人の意見を頼ったりしないで、 \n自分の気持ちで選んでくれ。 ":
        return "不要盲目怀疑背后的合理性，也不要依赖他人的意见，\n凭自己的心情做选择。 "
    if value == "……ふん。 そういうことにしておきましょう。 ":
        return "……哼。那就当作是这样吧。 "
    if value == "ところで、 プレゼントはどうするの？ \n基地からみんなへのものでしょう？ 今から配る？ ":
        return "话说回来，礼物怎么办？\n这是基地送给大家的吧？现在分发吗？ "
    if value == "いいえ、 グローウォームさんの話によると、 夜にトナカイの引くそりを乗って、 \n良い子たちの靴下にクリスマスプレゼントを入れるまでがサンタクロースの仕事らしいです。 ":
        return "不，据萤火虫小姐所说，圣诞老人的工作是晚上乘坐驯鹿拉的雪橇，\n直到把圣诞礼物放进乖孩子们的袜子里。 "
    if value == "それでも結構長いじゃん……":
        return "即使这样也挺久的……"
    if value == "というか、 まさか学園が本当に生徒に変なものを飲ませるとは……\nそれで、 投票はもう終わったの？ ":
        return "话说回来，没想到学园真的会让学生喝奇怪的东西……\n所以投票已经结束了吗？ "
    if value == "……確かに、 そうです。 ":
        return "……确实如此。 "
    if value == "むむむむ……":
        return "唔唔唔唔……"
    if value == "よし！ \n何もなかったことにしよう。 ":
        return "好！\n就当什么都没发生吧。 "
    if value == "瑞鶴の頭は壊れた！ ？ ":
        return "瑞鹤的脑袋坏掉了！？ "
    if value == "壊れてないよ。 ただ、 こういう明らかに裏で糸を引いている人がいる状況で、 \n真相を無理矢理に暴こうとするのはいい選択じゃないと思っただけだよ。 ":
        return "没有坏。只是，在这种明显有人在幕后操纵的情况下，\n强行揭露真相并不是好选择。 "
    if value == "昼寝したようなものだから、 この辺にしとこう。 ":
        return "就当是睡了午觉，到此为止吧。 "
    if value == "……賢明な判断です。 ":
        return "……明智的判断。 "
    if value == "そうですか……":
        return "这样啊……"
    if value == "今私がお姉ちゃんに渡したいのは、 お姉ちゃんのために私が選んだ、 \n私個人としてお姉ちゃんに贈るクリスマスプレゼントです。 ":
        return "我现在想交给姐姐的，是我为姐姐挑选的，\n作为我个人送给姐姐的圣诞礼物。 "
    if value == "……喜んでくれると嬉しいです。 ":
        return "……你能喜欢的话，我会很高兴。 "
    if value == "そういうことか～！ じゃあ、 あたしも！ \n日向へのプレゼントだよ、 じゃーん！ ":
        return "原来是这样～！那我也来！\n这是给日向的礼物，锵锵！ "
    if value == "……なら、 さきほどのことはおいておくことにして、 \n瑞鶴さんさえよければ、 摩耶さんと一緒にうちのテーマパークに遊びにきませんか？ ":
        return "……既然如此，先把刚才的事放一边，\n如果瑞鹤小姐愿意，要不要和摩耶小姐一起来我的主题乐园玩？ "
    if value == "え？ \nあたしも行っていいの？ ":
        return "诶？\n我也可以去吗？ "
    if value == "もちろん。 実は先ほどもお誘いしたかったのですが、 あいにく中断されてしまいました。 \nですから、 ここでもう一度正式にお誘いしたいのです。 ":
        return "当然。其实刚才就想邀请你，但不巧被打断了。\n所以现在想在这里再次正式邀请你。 "
    if value == "瑞鶴、 一緒に来てよ！ \nせっかくのチャンスじゃん！ ":
        return "瑞鹤，和我们一起来吧！\n这可是难得的机会！ "
    if value == "（ああ……これが表裏のない心優しい正統派美少女！ これこそが王道！ ）":
        return "（啊……这就是表里如一、心地善良的正统美少女！这才是王道！）"
    if value == "（決めた！ あたし、 これからはこういうのを目指す！ 個性派なんて一時の気の迷いよ！ \n王道こそが正義！ 世間の真理だ！ ）":
        return "（决定了！我以后就要以这种人为目标！个性派只是暂时迷失了方向！\n王道才是正义！才是世间真理！）"
    if value == "瑞鶴さまがこんなに考え込むのは、 \nやはり薬物の影響で、 脳がまだうまく働かないのが原因でしょう。 ":
        return "瑞鹤大人如此苦思，\n果然是因为药物影响，大脑还没有正常运作吧。 "
    if value == "ご安心を。 私に頭皮マッサージをさせてください。 \nそうしたら、 きっと脳もすぐ機能回復します。 ":
        return "请放心。请让我为您按摩头皮。\n这样一来，大脑一定也能马上恢复功能。 "
    if value == "い、 いいえ、 そんな大丈夫ですよ。 \nさっきのはただ人生の目標について考えてたというか——":
        return "不、不用了，真的没关系。\n刚才只是在思考人生目标之类的——"
    if value == "ご遠慮なさらず。 ":
        return "请不要客气。 "
    if value == "それじゃいくよ！ せーのっ ！ ":
        return "那就开始了！一、二！ "
    if value == "メリークリスマス！ ":
        return "圣诞快乐！ "
    if value == "うぎゃぁぁ！ ！ ？ ？ ":
        return "呜呀啊啊！！？ "
    if value == "ドン、ダン、バタン！":
        return "咚、砰、啪！"
    if value == "うそでしょ！ ？ あの胸、 あのお尻！ ！ どうして全然見覚えがないの！ ？ \nそんなのを見落としたなんて天罰に値するよ——！ ":
        return "不会吧！？那胸部、那个臀部！！为什么我完全没有印象！？\n竟然错过那种美景，简直应受天罚——！ "
    if value == "（もうだいぶ日が経ったし。\nお母さん、お父さん……心配してるかな。）":
        return "（已经过去很久了。\n妈妈、爸爸……你们担心我吗。）"
    if value == "あたしが最初に使って、世界を生成した材料は…\nもしかしたらオースエネルギーを帯びていたから、あの謎の結晶に変化したのかもしれない。":
        return "我最初使用并生成世界的材料……\n或许是因为带有奥斯能量，才变成了那种神秘结晶。"
    if value == "だとしたら、あの結晶のオースエネルギーを除去すれば、\n元の状態に戻れる。":
        return "如果是这样，只要去除那块结晶中的奥斯能量，\n就能恢复原状。"
    if value == "そしてこの世界でもう一度あの材料を採取地調合器に入れれば、\n生成された世界に門が現れるかもしれない！":
        return "然后在这个世界再次把那种材料放入采集地调合器，\n生成的世界里或许就会出现大门！"
    if value == "オースジェルはオースエネルギーをたくさん含んでて、\n安定しながらも、活発な状態を保っているでしょ？":
        return "奥斯凝胶含有大量奥斯能量，\n同时保持着稳定而活跃的状态，对吧？"
    if value == "だから、もとの材料よりもこのジェルの方が、\nオースエネルギーと調和しやすいと思うの。":
        return "所以我觉得，比起原材料，这种凝胶\n更容易与奥斯能量协调。"
    if value == "なるほど…置換反応みたいなやり方で、\nあの謎の結晶からオースエネルギーを分離させるのね。":
        return "原来如此……用类似置换反应的方法，\n把奥斯能量从那块神秘结晶中分离出来。"
    if value == "その通り！だからまず、\nオースエネルギーが含まれていないジェルの調合からだね。":
        return "没错！所以首先要从不含奥斯能量的凝胶调合开始。"
    if value == "え？なになに？\nちょっとついていけないんだけど……":
        return "诶？什么什么？\n我有点跟不上了……"
    if value == "大丈夫よ、オークランドは……そうね、\nあっちでお嬢さんのことを応援していればいいから～":
        return "没关系，奥克兰……对了，\n你在那边为小姑娘加油就好～"
    if value == "なんかバカにされているような気が……\nまあ、いいや、分かった！頑張ってね、ライザ！":
        return "总觉得好像被小看了……\n算了，知道了！加油啊，莱莎！"
    if value == "おお！いい感じかも！":
        return "哦！感觉不错！"
    if value == "ぷにぷにだ！\nゼリーみたい！":
        return "软乎乎的！\n像果冻一样！"
    if value == "まだ途中でしょ？ 集中して！":
        return "还没完成吧？集中精神！"
    if value == "よーし、あとは仕上げを、慎重に慎重に……。":
        return "好——接下来是最后处理，要小心、再小心……"
    if value == "これがお嬢さんが最初に使った材料か…\n今度こそ成功するかも。":
        return "这就是小姑娘最初使用的材料吗……\n这次或许真的能成功。"
    if value == "うん！！":
        return "嗯！！"
    if value == "善は急げ、早く採取地調合器に入れましょう。\nオークランド、出撃の用意を。":
        return "事不宜迟，快放进采集地调合器。\n奥克兰，准备出击。"
    if value == "はっははは！ 久しぶりに満足に動けたね！ ":
        return "哈哈哈哈！好久没这么痛快地活动了！ "
    if value == "ハロウィンの時一儲けするつもりだったのに、 わけわかんなくて爆発に\n巻き込まれて生き埋めになって以来、 何をやってもうまくいかないな。 ":
        return "本来想在万圣节赚一笔，没想到莫名其妙被卷入爆炸并活埋后，\n做什么都不顺利。 "
    if value == "ここ半年、 あの悪魔シスターに散々ひどい仕打ちを……ぐえぇ、 蒼天すでに死す。 ":
        return "这半年被那个恶魔修女狠狠折磨……呃啊，苍天已死。 "
    if value == "だがそれは全部過ぎたことよ！ 今晩、 栄光はグリッドレイ級に再び輝く！ \nそう、 基地のアイスクリームを一網打尽することから——":
        return "但那些都已经过去了！今晚，格里德利级的荣光将再次闪耀！\n没错，就从把基地的冰淇淋一扫而空开始——"
    if value == "そこまでだ。 ":
        return "到此为止。 "
    if value == "……ラムダは冷たいお菓子のほうがいい。 ":
        return "……拉姆达更喜欢冰凉的点心。 "
    if value == "ふざけんなよ！ そこの権力者、 仲間を捨てて敵に寝返るつもりか！ \n厳重に抗議する！ ":
        return "别胡闹！那边的掌权者，你打算抛弃同伴投靠敌人吗！\n我提出严正抗议！ "
    if value == "そうですか……そうなるとは、 さすがに予想外ですね。 ":
        return "这样啊……没想到会变成这样，确实出乎意料。 "
    if value == "『フェニーやムーバーのところに繋がっている可能性も排除できませんが、 先輩の話によると、 \nまったく別の、 未知の世界であると考えられます。 』":
        return "『虽然不能排除与菲妮或搬运者相连的可能，但根据前辈所说，\n这里应该是一个完全不同的未知世界。』"
    if value == "『ですからシグマさん、 あなたがそのエネルギー源を探す行動は無意味とは言えませんが、 \n少なくとも現時点では価値のあるものとは思いません。 すみませんでした。 』":
        return "『所以西格玛小姐，你寻找那个能源的行动不能说毫无意义，\n但至少在目前，我不认为它有价值。对不起。』"
    if value == "いやいやいや、 たったの三日でこれだけの進展があったのよ？ \nそれで謝らせるなんて、 私はそこまで自己中で図々しいやつじゃないよ。 ":
        return "不不不，短短三天就有了这么大的进展啊？\n我还没自私厚颜到要因为这个让你道歉。 "
    if value == "まあ、 同じ電気でも、 直流でしか使えない電気製品にそのまま交流を流すのがいけないように、 \nまったく無関係のエネルギーなら、 この体はとっくに爆発したかもしれないし。 ":
        return "嘛，就像只能使用直流电的电器不能直接通入交流电，\n如果是完全无关的能量，这具身体可能早就爆炸了。 "
    if value == "この前勝手に私で実験したことのけりはまだつけていないというのに、 \nまたこんなおもちゃを身近のところに置かれるとは。 なめられたものだね。 ":
        return "上次擅自拿我做实验的账还没算清，\n竟然又把这种玩具放到身边。真是小看我了。 "
    if value == "あのヒトの性格はあなたもよく知っているでしょう……\nラムダちゃんがわざわざそっちから持ってきたのです。 今回は大目に見てもらえませんか？ ":
        return "你也很清楚那个人的性格吧……\n是拉姆达特意从那边带来的。这次能不能睁一只眼闭一只眼？ "
    if value == "ちょっと、何なのよ、そのバカバカしいコンビ名は！\n聞いてないけど！？":
        return "等等，那个蠢透了的组合名是什么啊！\n我可没听说过！？"
    if value == "はやく帰ってこないと、\n基地の綺麗な美少女たちはみんな、あたしが攫っちゃうよ——！":
        return "你再不快点回来，\n基地里漂亮的美少女们我可全都要拐走了——！"
    if value == "本当にそれでいいんですか、 シグマさん？ \nあなたの妹さんに気付かれてしまいましたら……":
        return "真的这样就可以吗，西格玛小姐？\n要是被你妹妹发现了……"
    if value == "大丈夫、 大丈夫~ もう気配を消している。 それにうちのバカ妹はすごく鈍いから、 \nまさか私がお嬢ちゃんの懐に隠れているなんて、 絶対気がつかないよ。 ":
        return "没事，没事～我已经消除气息了。而且我家那个笨妹妹很迟钝，\n绝对不会发现我藏在小姑娘怀里。 "
    if value == "そこまで断言するとは思わなかった。 \nじゃあ……大丈夫？ ":
        return "没想到你会说得这么肯定。\n那……没问题？ "
    if value == "よく考えてみれば、 ジョージ5世姉さんもうなずいたのですから、 つまりそのゼータさんというヒトは\n少なくとも問題を起こすようなタイプではない……でしょうか？ ":
        return "仔细想想，既然乔治五世姐姐也点头了，也就是说那位泽塔小姐\n至少不是会惹出问题的类型……吧？ "
    if value == "島風ちゃんがそう言うなら、 私たちはあなたの話を信じるとしよう。 \nじゃあ私たちは先に行くよ。 後でパーティーで会おうね！ ":
        return "既然岛风这么说，我们就相信你的话吧。\n那我们先走了，之后派对上见！ "
    if value == "はっ！ ":
        return "是！ "
    if value == "おおおおおお！ \nすごいですね！ ":
        return "哦哦哦哦哦哦！\n好厉害！ "
    if value == "一瞬でくわえました！ \n颯爽とした動き、 まるで猛禽類のようです！ ":
        return "一瞬间就叼出来了！\n干脆利落的动作，简直像猛禽一样！ "
    if value == "あんた……なかなかやるじゃないか。 ":
        return "你……还挺能干的嘛。 "
    if value == "ふん、 こんなの朝飯前だ。 ":
        return "哼，这种事小菜一碟。 "
    if value == "……待て、 私は何を自慢しているんだ。 \nそもそも、 なぜリンゴをくわえる？ ":
        return "……等等，我在炫耀什么啊。\n说到底，为什么要叼苹果？ "
    if value == "これはアップルボビングと言って、 ハロウィンの伝統的な遊びですよ。 \nもともと恋愛占いに使うものでしたけど……ゼーっちにはまだ早いかもしれませんね？ ":
        return "这叫咬苹果，是万圣节的传统游戏。\n本来是用来占卜恋爱的……不过对泽泽来说可能还太早了？ "
    if value == "くだらない。 むしろあんたら、 食べ物を使ってゲームをするとは、 \nもったいないとは思わないのか。 ":
        return "无聊。倒是你们，竟然用食物来玩游戏，\n不觉得浪费吗？ "
    if value == "わあ~ ！ キレイですね！":
        return "哇～！真漂亮！"
    if value == "これはさすがにちょっと予想外だな。 ":
        return "这确实有点出乎意料。 "
    if value == "クリスマスの飾り付けで華やぐ室外パーティー会場に、 テーブルに並ぶ素敵な料理…… \nもうみんな集まっていますね。 それも、 とても楽しそうにしています。":
        return "被圣诞装饰点缀得华丽的室外派对会场，桌上摆满了精美料理……\n大家已经聚齐了，而且看起来都很开心。"
    if value == "こんな短期間で、 こんな素敵なクリスマスパーティーにできるなんて、 \nさすがお姉ちゃん！ すごいです！ ":
        return "这么短的时间就能办成如此棒的圣诞派对，\n不愧是姐姐！太厉害了！ "
    if value == "しかし、 一体どうやってこんな短期間でこのような素敵なパーティーを……？ ":
        return "可是，到底是怎样在这么短的时间里办成这么棒的派对的……？ "
    if value == "どう見ても、 一人で数時間でなんとかできる仕事量ではない。 \nつまり——":
        return "怎么看都不是一个人能在几小时内完成的工作量。\n也就是说——"
    if value == "もちろんあたしたちが手伝ってあげたおかげだよ！ ":
        return "当然是多亏我们帮忙啦！ "
    if value == "ぷはー！ \nやっと行っちゃった。 ":
        return "噗哈！\n终于走了。 "
    if value == "ああ、 それなら心配ありません。 食べ物を無駄にしたりはしません。 \nこのリンゴ、 ゼーっちさえよければ持って行っても構いませんよ。 ":
        return "啊，那不用担心。我们不会浪费食物。\n这个苹果，如果泽泽不介意，可以带走。 "
    if value == "ちなみに衛生面についてもご安心ください。 \nちゃんとその辺のことを考えて、 対応していますので。 ":
        return "顺便说一下，卫生方面也请放心。\n我们已经考虑到这些问题并做好处理了。 "
    if value == "そっちは別に気にしていないんだが……リンゴは貰っとく。 ":
        return "卫生方面我倒不在意……苹果我收下了。 "
    if value == "ジョージ5世さんが承知しているなら、 あたしもこれ以上何も言わないよ。 \n合格だ、 これを持ってて。 私の試練をクリアした賞品だ。 ":
        return "既然乔治五世小姐同意了，我也不再多说。\n合格，拿着这个。这是通过我的试炼的奖品。 "
    if value == "これは……ロウソク？ \nうちにも非常用照明器具として用意してあるが、 どうして今これを？ ":
        return "这是……蜡烛？\n我们这里也准备了应急照明，为什么现在给这个？ "
    if value == "持っていればそのうち分かりますよ。 さあ、 これで最初の試練はクリアしました！ \nおめでとう！ さっそくパートナーと一緒に、 次の試練に向かって出発しましょう~ ！ ":
        return "拿着它之后你自然会明白。好了，这样就通过第一个试炼了！\n恭喜！马上和搭档一起出发前往下一个试炼吧～！ "
    if value == "だからこんなことをするために来たんじゃないって言ってたのに……\nはあ、 この茶番に付き合いながら、 あの子を探すしかないか。 ":
        return "都说了我不是来做这种事的……\n唉，只能一边陪这个闹剧，一边寻找那孩子了。 "
    if value == "……確認したらあなたに話すかもしれない。 \nほら、 さっさといくぞ。 ":
        return "……确认之后或许会告诉你。\n好了，快走吧。 "
    if value == "えっ？ \nあたしはゼーっちと一緒に行きませんよ？ ":
        return "诶？\n我不会和泽泽一起走哦？ "
    if value == "グリッドレイ様を感謝することだね！ ":
        return "你该感谢格里德利大人！ "
    if value == "うん、 確かに二人のおかげで助かったよ。 \nでも、 二人だけじゃないよ。 ":
        return "嗯，确实多亏你们两位帮忙。\n但不只是你们两位。 "
    if value == "実はね、 基地のみんなに協力してもらったんだよ。 ":
        return "其实，我请基地里的大家一起帮忙了。 "
    if value == "みんなはみんなだよ。 \n基地にいる全員のことだ！ ":
        return "大家就是大家。\n指基地里的所有人！ "
    if value == "ぜ、 全員……！ ？ ":
        return "所、所有人……！？ "
    if value == "おや、 それはまた規模の大きい話だな。 ":
        return "哦，这还真是规模很大的事。 "
    if value == "だって、 最初からあたしたちだけですべての仕事をなんとかする必要はないでしょう？ \n団結は力なり！ って言うじゃん。 ":
        return "因为一开始就没必要只靠我们来完成所有工作吧？\n不是说团结就是力量嘛。 "
    if value == "例えばパーティー会場の飾り付けは島風ちゃんとビスマルクちゃん、 \nそしてフッドさんにやってもらったの~":
        return "比如派对会场的装饰，就是让岛风、俾斯麦和胡德小姐负责的～"
    if value == "倉庫にはまだすこしクリスマスオーナメントが残っているからね。 自分の部屋を飾り付けるために\n装飾品を購入している人もいるから、 パーティーの間だけ貸してもらっているの。 ":
        return "仓库里还剩一些圣诞装饰品。有人买了装饰品布置自己的房间，\n所以只是借来派对期间使用。 "
    if value == "お化けのイタズラも許されるなら、 \n私のわがままだって許されるに違いない！ ":
        return "既然连幽灵的恶作剧都能被允许，\n我的任性也一定能被原谅！ "
    if value == "さあさあ、 あのバカはもう行っちゃったし、 \n私たちも早く追いつきましょう！ ":
        return "好了好了，那个笨蛋已经走了，\n我们也快点追上去吧！ "
    if value == "……はあ？ \n今パートナーとか言ってたじゃないか。 ":
        return "……哈？\n你刚才不是说搭档什么的吗。 "
    if value == "待て、 よく考えてみれば、 パートナーとは何の話だ？ \nそんなこと聞いていないぞ？ ":
        return "等等，仔细想想，搭档是怎么回事？\n我可没听说这件事。 "
    if value == "あれれ？ おかしいね、 言い忘れましたかね？ \n実は今回のイベントは試行ルールとして、 ペアでしか正式的に参加できません。 ":
        return "咦咦？奇怪，我忘记说了吗？\n其实这次活动作为试行规则，只有组队才能正式参加。 "
    if value == "でもあたしは主催者側なので、 誰かと組むことができません。 ゼーっちのパートナーはこちらの—— \nジャンジャン！ ちょうどひとりぼっちの島風ちゃんでーす！ ":
        return "但我是主办方，不能和别人组队。泽泽的搭档就是这位——\n锵锵！正好孤零零的岛风！ "
    if value == "えっ！ ？ \nそ、 それは……確かに私にはまだパートナーの方がいませんが……":
        return "诶！？\n这、这个……确实我还没有搭档……"
    if value == "島風……ああ、 あんたはあの時の。 ":
        return "岛风……啊，你就是那时候的那个。 "
    if value == "島風ちゃんもパートナーを探したいから、 くろっちのところにいるのでしょう？ \nもしよければ、 こちらのこわ～い顔しているお姉さんと組みませんか？ ":
        return "岛风也是想找搭档，所以才在黑黑这里吧？\n如果愿意，要不要和这位表情可怕的大姐姐组队？ "
    if value == "勝手に決めるな。 確かに私はこっちのルールを一応従うと言ったが、 \n何でも押し付けられると思うな。 私には、 パートナーなんていらない。 ":
        return "别擅自决定。我的确说过暂时遵守这里的规则，\n但别以为可以把什么都强加给我。我不需要什么搭档。 "
    if value == "それにこいつも、 私みたいな得体の知れないよそ者より、 もっと親しい友達と一緒にやりたいだろう。 \nたとえば、 この前の……ル・マランだっけ？ あいつでいいじゃないか。 ":
        return "而且她也应该想和更亲近的朋友一起，而不是和我这种来历不明的外人。\n比如上次那个……路·马兰？和她组队不就好了。 "
    if value == "ル・マランさんはちょっとむずかしいかもしれませんね。 先ほど怖い顔して、 \nル・ファンタスクさんとル・テリブルさんを追いかけて、 どこかに行っちゃいましたよ。 ":
        return "路·马兰小姐可能有点难找。她刚才板着可怕的脸，\n追着空想小姐和恶毒小姐跑到某处去了。 "
    if value == "それなら、 ほかのやつでもいいだろう。 \nいるだろう、 他の友達。 ":
        return "那其他人也可以吧。\n你还有别的朋友吧。 "
    if value == "えっ？ \nあの……えっと……":
        return "诶？\n那个……呃……"
    if value == "（グローウォームさんとヒーアマンさんと組んでいますし、 今日はリリさんもバンカー・ヒルさんも\n遊びに来ていません。 さすがにシグマさんの名前を出すわけにも……）":
        return "（我已经和萤火虫小姐、希尔曼小姐组队了，而且今天莉莉小姐和邦克山小姐\n也没有来玩。更不能提西格玛小姐的名字……）"
    if value == "えっと……えっと……………………":
        return "呃……呃……………………"
    if value == "……いないのか！ ？ ":
        return "……没有吗！？ "
    if value == "うわっ……ゼーっちひどいですよ。 友達に置いて行かれて、 ひとりぼっちになった可哀想な\n女の子の傷口をえぐっては容赦なく断るなんて……あたしですらここまでしませんよ。 ":
        return "呜哇……泽泽太过分了。朋友把她丢下，让她成了孤零零的可怜女孩，\n你还无情地揭她伤疤并拒绝……连我都不会做到这种程度。 "
    if value == "……しょうがないな。 こっちのルールを守ると決めたからには、 \n今日は最后まで徹底しよう。 私があんたと組む、 いいな？ ":
        return "……真拿你没办法。既然决定遵守这里的规则，\n今天就贯彻到底吧。我和你组队，明白吗？ "
    if value == "（えっ？ いいんですか……？ \nわかりました……）":
        return "（诶？可以吗……？\n我明白了……）"
    if value == "……ふふ、 どうやらどっちもうまくやっているようですね。 ":
        return "……呵呵，看来两边都相处得很好呢。 "
    if value == "ほっとしたような表情だね。 \nしばらく会わないうちに、 あなたもずいぶん変わったな。 ":
        return "你露出了如释重负的表情。\n没见面这段时间，你也变了很多。 "
    if value == "……なんのことでしょうか。 ":
        return "……您指什么？ "
    if value == "新しい環境に自分についてきたような後輩が、 ちゃんと古株たちとうまくやっているのか気にかける……\n昔の誰かさんは、 ここまで親切じゃない。 ":
        return "关心跟随自己来到新环境的后辈是否和老成员相处融洽……\n以前的某个人可没这么亲切。 "
    if value == "あるいは……そうだな。 \n「親切心を行動に移すだけの素直さが欠けている」と言うべきか。 ":
        return "或者……该怎么说呢。\n应该说是“缺少把好意付诸行动的坦率”吧。 "
    if value == "おっと、 失礼。 \nただの独り言だ、 気にしないでくれ。 ":
        return "哎呀，失礼了。\n只是自言自语，不用在意。 "
    if value == "？ なんか言ったのか？ ":
        return "？你刚才说什么了吗？ "
    if value == "何でもありません！ \nあの、 ではよろしくお願いします、 ゼータさん。 ":
        return "没什么！\n那个，今后请多关照，泽塔小姐。 "
    if value == "……おお。 じゃあ、 さっさと行くぞ。 ":
        return "……哦。那就快走吧。 "
    if value == "ふん……":
        return "哼……"
    if value == "なあ、 大丈夫なの、 島风？ あいつはムーバーの者じゃないか。 どこかの誰かさんみたいに\nクズの匂いを漂わせているわけではないけど、 なんだか怖そうですし……":
        return "那个，岛风，你没事吧？她不是搬运者的人吗。虽然不像某个人那样散发着人渣气息，\n但看起来还是有点可怕……"
    if value == "あの人と一緒にいると、 島風ちゃんがいじめられるかもしれません。 \nやっぱり私たちと一緒に回りましょう？ ":
        return "和那个人在一起的话，岛风可能会被欺负。\n还是和我们一起逛吧？ "
    if value == "いいえ、 大丈夫です。 ゼータさんはおっかないように見えて、 \n実はただ不器用なだけですから。 私のことは心配しないでください。 ":
        return "不，没关系。泽塔小姐看起来很吓人，\n其实只是有点笨拙而已。请不用担心我。 "
    if value == "つまり、 夜になってからみんなに配るのね？ \nう～ん、 じゃあトナカイとそりはどうする？ ":
        return "也就是说，要到晚上再发给大家？\n嗯～那驯鹿和雪橇怎么办？ "
    if value == "私か？私は…そうね。\n他人の話よりも、自分の判断を信じるわ。":
        return "我吗？我……是啊。\n比起别人的话，我更相信自己的判断。"
    if value == "ごめんなさい…":
        return "对不起……"
    if value == "…迷惑をかけたくないから、みんなを傷つけたくないからって、\n一人でどうこうしようとするのは、もうしない。":
        return "……我不会再因为不想添麻烦、不想伤害大家，\n就试图独自解决一切。"
    if value == "指揮官とみんながあんなにしてくれたのに、\n当の私がみんなのことを信じないなんて…おかしいよね。":
        return "指挥官和大家都为我做了那么多，\n可我却不相信大家……很奇怪吧。"
    if value == "この命は、指揮官とみんなのおかげで拾ったもので、\n私はこれからも指揮官とみんなと一緒にいたい。":
        return "这条命是指挥官和大家帮我捡回来的，\n我今后也想和指挥官、大家一起。"
    if value == "指揮官…どうか、聞いてください！\n今まで言えなかったこと…今なら言える。":
        return "指挥官……请听我说！\n以前无法说出口的话……现在我能说了。"
    if value == "——もう二度と逃げたりはしない。\nだから…指揮官とみんなの力を、もう一度貸してほしい…！":
        return "——我再也不会逃跑了。\n所以……请再次借给我指挥官和大家的力量……！"
    if value == "それに、諦めるのはまだ早いです！\nオークランドさんは、そんな弱気な人間ではないはず…！":
        return "而且，现在放弃还太早！\n奥克兰小姐不该是这么软弱的人……！"
    if value == "ラフィーちゃん、オークランドさん、こちらです！":
        return "拉菲、奥克兰小姐，这边！"
    if value == "うっ…！":
        return "呜……！"
    if value == "ラフィーちゃん、傷はまだ…っ。\nごめん、もう少し我慢して！":
        return "拉菲，伤口还没……\n对不起，再忍耐一下！"
    if value == "あのムーバーの子…\n一人で残って本当に大丈夫なの？":
        return "那个搬运者的孩子……\n一个人留下真的没问题吗？"
    if value == "侵入者はここだ！\nみんな、来てくれ！！":
        return "入侵者在这里！\n大家快来！！"
    if value == "おいらたちのシマに入った以上、\n五体満足で出られると思うなよ！":
        return "既然踏入我们的地盘，\n就别想完好无损地出去！"
    if value == "…！\n人の心配をしている場合じゃなさそうですね…":
        return "……！\n现在似乎不是担心别人的时候……"
    if value == "わかったわ。敵と周辺の状況に詳しいムーバーの二人は…\nそれぞれ別のチームに分けた方が良さそうね。":
        return "知道了。熟悉敌人和周边情况的两名搬运者……\n看来最好分别分到不同队伍。"
    if value == "うっ…！MCCの戦姫がどんどん集まってきます！\nこのままでは…！！":
        return "呜……！MCC的战姬不断聚集过来了！\n这样下去……！！"
    if value == "ラフィーも戦うの…！":
        return "拉菲也要战斗……！"
    if value == "ブルースフィアのワルモノが、ラフィーたちのために…？\n怪しいの。":
        return "蓝星的坏蛋竟然为了拉菲她们……？\n很可疑。"
    if value == "ああ…やっぱりこうなっちゃうよね。\nうーん〜〜どうしようかな。":
        return "啊……果然变成这样了。\n嗯～～该怎么办呢。"
    if value == "でも、割と本気だから。\nイータ…いいえ——":
        return "不过我是认真的。\n伊塔……不——"
    if value == "…………………………わかりました。\nお言葉に甘えさせていただきます。":
        return "…………………………明白了。\n那我们就承蒙你的好意了。"
    if value == "泣き虫の姉ちゃん…？":
        return "爱哭的姐姐……？"
    if value == "行きましょう、ラフィーちゃん。\nパールベイのオークランドさん…ご武運を。":
        return "我们走吧，拉菲。\n珍珠湾的奥克兰小姐……祝你武运昌隆。"
    if value == "わ、分かった！もうそんな呼び方はしないから！\nごめんなさい、サンディエゴ姉さん…！！":
        return "好、好吧！我不会再那样称呼你了！\n对不起，圣地亚哥姐姐……！！"
    if value == "ふふん〜\nそれでよし！":
        return "哼哼～\n这样就好！"
    if value == "——ふ。こっちに来てから、随分の間にちゃんとした食事を取れていないわね。\nご馳走に感謝するわ。本当に、やっと文明世界に戻った感じね。":
        return "——哼。来到这里后，我们已经很久没好好吃饭了。\n感谢这顿盛宴。真的有种终于回到文明世界的感觉。"
    if value == "ごちそうさまでした〜！ポートランドさんの料理の腕は素晴らしいものですね。\n高級レストランのシェフに比べても少しも遜色がありませんよ！":
        return "多谢款待～！波特兰小姐的厨艺真了不起。\n即使和高级餐厅的厨师相比也毫不逊色！"
    if value == "…その辺にしとけ。\nこっちはおままごとに付き合ってる暇はないんだ。":
        return "……适可而止吧。\n我没时间陪你们玩过家家。"
    if value == "こいつらは何だ？聞いていないんだけど？\n協力関係だから、もっと誠意を見せる方が道理じゃないのか？":
        return "这些家伙是什么？我怎么没听说？\n既然是合作关系，展现更多诚意不是理所当然的吗？"
    if value == "まあまあ、そう急かさないでよ、オミクロンさん。\n朝日たちにとっても、彼女たちの出現は予想外だったのよ。":
        return "好啦好啦，别这么着急嘛，奥米克戎小姐。\n对朝日她们来说，她们的出现也是意料之外。"
    if value == "それに、こうしてあなたたちをテーテューアスの内部に迎え入ることは、\n既に十分な誠意を示したと思うけど？":
        return "而且，像这样把你们迎入特修斯内部，\n我觉得已经展现了充分的诚意吧？"
    if value == "「武装解除」の前提で、だろ？":
        return "前提是“解除武装”，对吧？"
    if value == "お互い信頼を深めた証拠だろ？":
        return "这是彼此加深信任的证明吧？"
    if value == "丸腰でお前たちの領域に足を踏み入れた僕たちこそが、\n誠意を見せてるほうじゃないか。":
        return "毫无武装踏入你们领域的我们，\n才是展现诚意的一方吧。"
    if value == "はいはい、ケンカはよくないぞ":
        return "好啦好啦，吵架可不好。"
    if value == "いざという時、みんなが俺を守ってくれるから":
        return "因为关键时刻大家都会保护我。"
    if value == "…ふん。無力な人間の身で\n僕たちの前に立つその勇気に免じて…顔を立ててやるよ。":
        return "……哼。看在你以无力人类之身站在我们面前的勇气上……给你个面子。"
    if value == "ありがとう":
        return "谢谢"
    if value == "あなたね、なんの挨拶もなしにブルースフィアから出たなんてひどいよ！\nお姉さんは傷ついたよ？":
        return "你真是的，竟然一声招呼都不打就离开蓝星，太过分了！\n姐姐很受伤哦？"
    if value == "サンディエゴ…姉さん、\nごめんなさい。":
        return "圣地亚哥……姐姐，\n对不起。"
    if value == "またしょんぼり顔？\n激射の道を進む人間がするような顔じゃないな！":
        return "又是一副垂头丧气的脸？\n走在猛烈射击之路上的人可不该露出这种表情！"
    if value == "まさかあの指揮官にいじめられた？いいやつのフリしてて、実は薄情者だったりとか？\nよし！今から懲らしめてやるぞ！":
        return "难道被那个指挥官欺负了？她装作好人，其实是个薄情的人？\n好！我现在就去教训她！"
    if value == "ちちちち、違うって！":
        return "不不不不，不是那样！"
    if value == "指揮官はいつもよくしてくれてるから、全然悪くないよ！":
        return "指挥官一直对我很好，完全不是她的错！"
    if value == "じゃあ、悪いのがあなたってこと？":
        return "那就是说，错的是你？"
    if value == "あのさ〜！\n私たちって、いとこ同士だろ？":
        return "那个～！\n我们是表亲吧？"
    if value == "悩みがあるのに教えてくれないと、\n「自分がそんなに頼れないの？」って、私だって傷つくよ。":
        return "你明明有烦恼却不告诉我，\n我也会因为“我就这么不值得依靠吗？”而受伤哦。"
    if value == "…その、サンディエゴ…さん。\nあなたはまだ知らないかもしれないけど…":
        return "……那个，圣地亚哥……小姐。\n你可能还不知道……"
    if value == "私たちは多分…いとこ同士でもなんでもないから、\nそんなに私のことを気に掛ける必要は、ないんだよ。":
        return "我们大概……根本不是表亲，\n所以你没必要这么在意我。"
    if value == "…………………………あぁ？\n何言ってんの。":
        return "…………………………啊？\n你在说什么。"
    if value == "うむ〜〜〜　つまり、あなたは実はブルースフィアの戦姫ではなく、\nもともとはこっちの世界の、しかもあの恐ろしい赤い戦鬼たちの仲間だった。":
        return "嗯～～～也就是说，你其实不是蓝星的战姬，\n原本是这个世界的人，而且还是那些可怕红色战鬼的同伴。"
    if value == "しかし何らかの理由でブルースフィアに行って、安全保障会議のやつに利用されて、\nそして手が負えなくなると、やつらはあなたを排除するって決めた。":
        return "但因为某种原因你去了蓝星，被安全保障会议的家伙利用，\n当他们无法控制你后，就决定除掉你。"
    if value == "そんなあなたは昔、す～んごい悪者で、\nすました顔でパパパ～っと敵対の戦姫をバラバラに潰したりする。":
        return "而你过去是个超级大坏蛋，\n会面不改色地啪啪啪把敌对战姬一个个碾碎。"
    if value == "そう…そういうこと、です。\nわかりましたか？サンディエゴ…さん。":
        return "是……就是这样。\n明白了吗？圣地亚哥……小姐。"
    if value == "全然わからないな。":
        return "完全不明白。"
    if value == "えっ？\nど、どこがわからないの…？":
        return "诶？\n哪、哪里不明白……？"
    if value == "戦姫同士間で、あなたに関する噂はずっと密かに流されているわ。":
        return "战姬之间一直在私下流传关于你的传闻。"
    if value == "どこもかしこもわからないってば！！":
        return "我说了哪里都不明白！！"
    if value == "えっ、えええええっ？？":
        return "诶、诶诶诶诶？？"
    if value == "大体！\nあなたは私のいとこで、妹だと思っているの！":
        return "总之！\n我把你当作我的表亲和妹妹！"
    if value == "いきなりそんなこと言われたって、\n「なるほどわかりました」って答えるはずもないでしょ！":
        return "突然被这样告知，\n我也不可能回答“原来如此，我明白了”吧！"
    if value == "しかし、サンディエゴさん…":
        return "但是，圣地亚哥小姐……"
    if value == "もう一度その「サンディエゴさん」って呼んだら、\n今すぐお尻ぺんぺんしてやる！":
        return "你再叫一次“圣地亚哥小姐”，\n我现在就打你屁股！"
    if value == "し…！？":
        return "什……！？"
    if value == "戦姫を差別することなく、\n平等に接してくれる奇妙な人間がいると。":
        return "听说有个奇怪的人类，不歧视战姬，\n会平等地对待她们。"
    if value == "一体誰がそんな噂を…？":
        return "到底是谁传出这种传闻的……？"
    if value == "ブルースフィアから亡命したのも、\n配下の戦姫を守るためだとか。":
        return "据说你从蓝星逃亡，\n也是为了保护麾下的战姬。"
    if value == "その噂を信じるのか？":
        return "你相信那种传闻吗？"
    if value == "…どうせあなたは自分の中のあの「超悪いもうひとりの自分！」がまた蘇って、\n周りの人たちを傷つけると心配しているんでしょ？":
        return "……反正你是在担心自己体内那个“超级坏的另一个自己！”再次苏醒，\n伤害身边的人吧？"
    if value == "でもさ、この様子だと、あなたの仲間たちとあの指揮官は、\nとっくにそんなことを承知してるんでしょ？ブルースフィアを出る前に。":
        return "但是看现在这样，你的伙伴们和那位指挥官，\n早在离开蓝星前就已经知道这些了吧？"
    if value == "あなたの中には「きゃ！抑えきれない残虐な力！」が存在するとわかっていながら、\nなおあなたと一緒にいることを選んだ。":
        return "明知你体内存在“呀！无法抑制的残虐力量！”，\n却仍然选择和你在一起。"
    if value == "つまり、\n彼らはすでに「万が一」って覚悟ができていることだよね。":
        return "也就是说，\n他们已经做好了“万一发生意外”的觉悟。"
    if value == "——だとしたら、いつまでも彼らを傷つけちゃう心配をするってのは、\nその覚悟と気持ちを無視していることにならない〜？":
        return "——既然如此，你一直担心会伤害他们，\n不就等于无视了他们的觉悟和心意吗～？"
    if value == "……！\nそれは…":
        return "……！\n那是……"
    if value == "それは俺の言いたいことでもある":
        return "这也是我想说的。"
    if value == "指揮官！？\nいつからそこに！？":
        return "指挥官！？\n你什么时候到的！？"
    if value == "おう、来てくれたか！\nオークランドの指揮官殿〜":
        return "哦，你来啦！\n奥克兰的指挥官阁下～"
    if value == "あなたも、このお馬鹿さんに何か言ってあげたらどう？\nでないと、安心して妹を託せ無いよね〜":
        return "你也对这个笨蛋说些什么吧？\n不然我可无法放心把妹妹托付给她呢～"
    if value == "た、託すって…まるで結婚するみたいに言わないでよ…！":
        return "托付……别说得像要结婚一样啊……！"
    if value == "は、はい…！":
        return "是、是的……！"
    if value == "すごく心配してた":
        return "我非常担心你"
    if value == "指揮官…ごめんなさい。\n私はあの時…":
        return "指挥官……对不起。\n我那时候……"
    if value == "自分を犠牲にするなんて考えるな":
        return "别想着牺牲自己。"
    if value == "この…！\nこいつをくらいなさい！":
        return "你这家伙……！\n接招吧！"
    if value == "…もう、無理かな。":
        return "……已经不行了吗。"
    if value == "貴様、情けないな。":
        return "你真没出息。"
    if value == "…私は今まで、あなたのことを「私たちの敵」で、\nオミクロンの言う「冷酷なフェニーの殺戮機械」としか、思っていませんでした。":
        return "……直到现在，我一直只把你当作“我们的敌人”，\n以及奥米克戎所说的“冷酷的菲妮杀戮机器”。"
    if value == "でも事実は違います。オークランドさん…\nあなたはオミクロンの言うような人ではないと思ってしまいました。":
        return "但事实并非如此。奥克兰小姐……\n我觉得你并不是奥米克戎所说的那种人。"
    if value == "では、そろそろ本題に入りましょう。":
        return "那么，差不多该进入正题了。"
    if value == "…元ブルースフィア所属のあなたたちだから、言わなくてもわかると思うが——\n軍事機密なので、無闇に口外するわけにはいかないわ。":
        return "……你们曾属于蓝星，我想即使不说也明白——\n这是军事机密，不能随意泄露。"
    if value == "あら、それは予想外だわ。":
        return "哎呀，这倒出乎意料。"
    if value == "——ボルチモアさんは、もっと話の分かる人だと思ってた。":
        return "——我还以为巴尔的摩小姐是个更通情达理的人。"
    if value == "最大の機密、すでにバレてしまったのに。\n——「あなたたちがこっちの世界にいる」ということがね。":
        return "最大的机密已经暴露了。\n——那就是“你们在这个世界”。"
    if value == "理由はいうまでもなく、この世界を偵察するためでしょう。\n安全保障会議はかねてからそのつもりだったわ。":
        return "理由不言而喻，是为了侦察这个世界吧。\n安全保障会议早就有这个打算了。"
    if value == "ただ自らゲートを開く技術を持っていないから、\nなかなか実行に移せないだけ。":
        return "只是因为他们没有自行打开网关的技术，\n所以一直难以付诸行动。"
    if value == "つまり最大の「機密」は、あなたたちの存在そのもの——\nブルースフィアはすでにゲートの技術を手に入れたのね。違う？":
        return "也就是说，最大的“机密”就是你们的存在本身——\n蓝星已经获得了网关技术，对吧？"
    if value == "…やれやれ、やはりごまかせないね～\n相変わらず、鋭いね。":
        return "……真是的，果然瞒不过你～\n你还是一如既往地敏锐。"
    if value == "そんな、ありえない…\nムーバーだって長年の研究の末にやっと掌握した技術なのに…":
        return "怎么可能……\n这可是搬运者经过多年研究才终于掌握的技术……"
    if value == "ブルースフィア側はこんな短い期間で…！？":
        return "蓝星方面竟然在这么短的时间内……！？"
    if value == "技術のことは現場の私たちに聞いてもしょうがないわ。\nただ…":
        return "技术方面问我们这些现场人员也没用。\n只是……"
    if value == "ただ？":
        return "只是？"
    if value == "私が知っている情報によれば、たぶんそれは——\n「ヴァムステック」の手柄じゃないかと。":
        return "根据我掌握的信息，那大概是——\n“VAMSTECH”的功劳。"
    if value == "…だと思ってるわ。":
        return "……我是这么认为的。"
    if value == "平凡な研究者が束になっても、技術の差を埋めて、\n異世界に繋ぐ扉を開くことなんて、出来やしないでしょう。":
        return "就算一群普通研究者聚在一起，也不可能弥补技术差距，\n打开连接异世界的大门吧。"
    if value == "「ヴァムステック」…って、なぁに？\n変な名前ね。":
        return "“VAMSTECH”……是什么？\n名字真奇怪。"
    if value == "そういえばここ数年、あまり表に出ていないわね。\n戦姫の訓練も全部安全保障会議に引き渡したし、オークランドのような子が知らないのも無理はないわ。":
        return "说起来，这几年她确实很少公开露面。\n战姬训练也全部交给了安全保障会议，像奥克兰这样的孩子不知道也很正常。"
    if value == "ふふ、ラフィーちゃんはいつも私のことを泣き虫って言うけど…\n今はラフィーちゃんの方こそ泣き虫だね。":
        return "呵呵，拉菲总是说我是爱哭鬼……\n现在看来拉菲才是爱哭鬼呢。"
    if value == "むっ！？":
        return "唔！？"
    if value == "な、泣いてない！\nラフィーは泣いてないもん。":
        return "没、没有哭！\n拉菲才没有哭呢。"
    if value == "自ら抜け出すとは、\nさすがはラフィー様でございます。":
        return "竟然自己逃出来了，\n不愧是拉菲大人。"
    if value == "あれ、モーリエは？":
        return "咦，莫利呢？"
    if value == "モーリエ様ですか？\nあの方なら、こちらにはおりませんよ？":
        return "莫利大人吗？\n那位不在这里哦？"
    if value == "…なんですって！？":
        return "……你说什么！？"
    if value == "拙者たちは最初からラフィー様の移送しか担当しておりません。\nフッド様はご存じではないのですか？":
        return "在下一开始负责的就只有运送拉菲大人。\n胡德大人不知道吗？"
    if value == "だとしたら、彼女は一体どこへ…！？":
        return "如果是这样，她到底去了哪里……！？"
    if value == "申し訳ございませんが、\nモーリエ様の居場所について、拙者もよくわかりません。":
        return "非常抱歉，\n在下也不清楚莫利大人的所在。"
    if value == "——あなたは本当に…！":
        return "——你真是……！"
    if value == "…………………………\n…いいえ、ごめんなさい。":
        return "…………………………\n……不，对不起。"
    if value == "ここまでしてくれたのに、あなたを疑うべきではありませんね。\n謝罪します。":
        return "你都为我们做到这种地步了，我不该怀疑你。\n我向你道歉。"
    if value == "いいえ、どうかお気になさらず。\nフッド様の立場からすれば、慎重になるのは当然なので。":
        return "不，请不要放在心上。\n站在胡德大人的立场上，谨慎是理所当然的。"
    if value == "（…なんか睨まれた！）":
        return "（……好像被瞪了！）"
    if value == "え？\nどうしてブルースフィアのワルモノもいるの？":
        return "诶？\n为什么蓝星的坏蛋也在这里？"
    if value == "あっ…詳しくは後で説明しますから、\nとにかく、今この人は私たちの敵ではありません。ラフィーちゃん。":
        return "啊……详情之后再解释，\n总之现在这个人不是我们的敌人。拉菲。"
    if value == "？？？\nわかったの…":
        return "？？？\n明白了……"
    if value == "でも、どうしよう。\nモーリエが…":
        return "但是，该怎么办。\n莫利她……"
    if value == "ラフィーはお姉さんとして、モーリエを守らなきゃなのに…":
        return "拉菲作为姐姐，明明必须保护莫利……"
    if value == "仕方ありません。モーリエはここにいない以上、私たちも早く撤退しなければなりません。\n少なくともラフィーちゃんの救出ができたから、無駄足ではありません。":
        return "没办法。既然莫利不在这里，我们也必须尽快撤退。\n至少成功救出了拉菲，并不是白跑一趟。"
    if value == "…心配しないで、ラフィーちゃん。\n絶対、モーリエを見つけ出しましょう。":
        return "……别担心，拉菲。\n我们一定会找到莫利。"
    if value == "お二方、早くここを離れましょう。\n拙者が案内を——":
        return "两位，请快点离开这里。\n在下来带路——"
    if value == "隊長…何をしてんすか？":
        return "队长……你在做什么？"
    if value == "はい！ ":
        return "是！ "
    if value == "ちっ！":
        return "啧！"
    if value == "——こっちだ！\n早く来い！":
        return "——这边！\n快过来！"
    if value == "ムーバーの包囲網がだんだん狭くなってきたわ…\n今度は突破するのに一苦労するでしょう。いつまでもてるかしら…":
        return "搬运者的包围网逐渐收紧了……\n这次突破恐怕要费很大力气。不知道还能撑多久……"
    if value == "オークランドたちはまだなのか…！？":
        return "奥克兰她们还没到吗……！？"
    if value == "応答がないわ…ムーバーの支配圏だから、\n通信が干渉されたかもしれない。":
        return "没有回应……这里是搬运者的控制区域，\n通信可能受到了干扰。"
    if value == "信号弾を持たせてよかった…信号を確認できるまで、\n私たちはもうひと踏ん張りするしかなさそうね…！":
        return "幸好让她们带了信号弹……在确认信号前，\n我们似乎只能再坚持一下了……！"
    if value == "ついてきたのか？全く気づきませんでした…\nうむ、そなたもだいぶ成長したな。":
        return "你跟过来了吗？我完全没注意到……\n嗯，你也成长了不少。"
    if value == "隊長…一体、何をしてるんすか！？\nこれじゃ、まるで…！！":
        return "队长……你到底在做什么！？\n这简直就像……！！"
    if value == "まるで、なんですか？":
        return "简直像什么？"
    if value == "…他の小隊にはすでに連絡済み。\nまもなくみんながここに来る。":
        return "……已经联系过其他小队了。\n大家很快就会到这里。"
    if value == "隊長、今でも遅くないっす！\n一緒にこいつらを始末すれば、ことは穏便に済ませるはず…！！":
        return "队长，现在还来得及！\n只要一起解决掉这些家伙，事情应该能和平解决……！！"
    if value == "…驚きましたね。\nまさかあの真っ直ぐなそなたが、このような言葉をかけてくれるとは。":
        return "……真让人惊讶。\n没想到率直的你竟然会对我说出这样的话。"
    if value == "だめですよ。\n誰かに聞かれたら、そなたも巻き込まれてしまいます。":
        return "不行。\n被别人听到的话，你也会被卷进来。"
    if value == "隊長…！\nなぜ…！！":
        return "队长……！\n为什么……！！"
    if value == "<color=#f14949>ムーバーの増援</color>":
        return "<color=#f14949>搬运者增援</color>"
    if value == "見つけた！\nそこだ！":
        return "找到了！\n在那里！"
    if value == "フッド様、ラフィー様、先に進んでください。\nここは拙者が引き受けます！":
        return "胡德大人、拉菲大人，请先走。\n这里交给在下！"
    if value == "3時方向は海への最短ルートになっております。\nそちらに向かってください！":
        return "3点钟方向是前往海边的最短路线。\n请朝那里前进！"
    if value == "フッド様、ラフィー様…\nご武運をお祈りしております。":
        return "胡德大人、拉菲大人……\n祝你们武运昌隆。"
    if value == "あなた…一人で残るつもりですか！？\nダメです、そんなことしたらあなたが…！":
        return "你……打算一个人留下吗！？\n不行，如果那样的话你会……！"
    if value == "フッド様——\n今度こそ、正しい選択をさせてください。":
        return "胡德大人——\n这一次，请让我做出正确的选择。"
    if value == "こんなこと言ってしまえば、逆に負担をかけるかもしませんが…それでも。\nフッド様たちの行動が、転機をもたらしてくれることを信じています。":
        return "说这种话或许反而会给你们增加负担……但即便如此，\n我仍相信胡德大人她们的行动会带来转机。"
    if value == "——早く行ってください！！":
        return "——快走！！"
    heuristic = heuristic_local_translation(value)
    if heuristic is not None:
        return heuristic
    compact_mapping = getattr(direct_pattern_translation, "_compact_mapping", None)
    if compact_mapping is None:
        compact_mapping = {}
        for source, target in list(exact.items()) + list(common_event.items()):
            key = re.sub(r"\s+", "", normalized_text(source))
            if key not in compact_mapping:
                compact_mapping[key] = target
            elif compact_mapping[key] != target:
                compact_mapping[key] = None
        direct_pattern_translation._compact_mapping = compact_mapping
    return compact_mapping.get(compact_value)


def collect_reference_mapping(
    source: Path, reference: Path
) -> tuple[dict[str, str], dict[tuple[str, str, str, tuple[str, ...]], str], int, int, int]:
    """Extract Chinese values from matching reference rows and JSON paths."""
    candidates: dict[str, collections.Counter[str]] = collections.defaultdict(collections.Counter)
    compatible_candidates: dict[str, collections.Counter[str]] = collections.defaultdict(collections.Counter)
    entries: dict[tuple[str, str, str, tuple[str, ...]], str] = {}
    matched_occurrences = 0
    matched_files = 0
    conflicting_sources: set[str] = set()

    for source_db in sorted(source.glob("config_*.db")):
        reference_db = reference / source_db.name
        if not reference_db.exists():
            continue
        matched_files += 1
        with sqlite3.connect(f"file:{source_db}?mode=ro", uri=True) as source_conn:
            source_rows = {
                (str(row_id), str(index_id)): json.loads(xor_bytes(encoded).decode("utf-8"))
                for row_id, index_id, encoded in source_conn.execute(
                    "SELECT id,indexid,jsonbytes FROM DBObject WHERE id <> 'nill'"
                )
            }
        with sqlite3.connect(f"file:{reference_db}?mode=ro", uri=True) as reference_conn:
            reference_rows = {
                (str(row_id), str(index_id)): json.loads(xor_bytes(encoded).decode("utf-8"))
                for row_id, index_id, encoded in reference_conn.execute(
                    "SELECT id,indexid,jsonbytes FROM DBObject WHERE id <> 'nill'"
                )
            }

        for (row_id, index_id), source_data in source_rows.items():
            reference_data = reference_rows.get((row_id, index_id))
            if reference_data is None:
                continue
            for path, source_value, reference_value in walk_aligned(source_data, reference_data):
                if not JP_RE.search(source_value):
                    continue
                if not isinstance(reference_value, str):
                    continue
                if reference_value == source_value or not CN_RE.search(reference_value):
                    continue
                if JP_KANA_RE.search(reference_value):
                    continue
                candidates[source_value][reference_value] += 1
                # Exact token-compatible rows retain path-specific priority.
                # Rows with only formatting drift remain global candidates; a
                # single unambiguous CN candidate is safe to reuse even when
                # the regional client omitted size/color markup.
                if compatible_reference_tokens(source_value, reference_value):
                    compatible_candidates[source_value][reference_value] += 1
                    entries[(source_db.name, row_id, index_id, path)] = reference_value
                matched_occurrences += 1

    mapping: dict[str, str] = {}
    for source_value, options in compatible_candidates.items():
        if options:
            # Prefer most frequent CN wording as global fallback. Exact row/path
            # entries still take priority during output, preserving context when
            # one Japanese source line has multiple regional translations.
            mapping[source_value] = options.most_common(1)[0][0]
        if len(options) > 1:
            conflicting_sources.add(source_value)
    return mapping, entries, matched_occurrences, matched_files, len(conflicting_sources)


def translate_all(
    indexed: dict[int, str],
    cache_path: Path,
    workers: int,
    batch_size: int,
    seed_mapping: dict[str, str] | None = None,
) -> dict[str, str]:
    cache: dict[str, str] = {}
    if cache_path.exists():
        cache = json.loads(cache_path.read_text(encoding="utf-8"))
    if seed_mapping:
        cache.update(seed_mapping)
    pending = [(i, s) for i, s in indexed.items() if s not in cache]
    print(f"strings={len(indexed)} cached={len(indexed)-len(pending)} pending={len(pending)}", flush=True)
    if not pending:
        return cache

    batches: list[list[tuple[int, str]]] = []
    current: list[tuple[int, str]] = []
    chars = 0
    for item in pending:
        size = len(item[1]) + 24
        if current and chars + size > batch_size:
            batches.append(current)
            current, chars = [], 0
        current.append(item)
        chars += size
    if current:
        batches.append(current)
    print(f"batches={len(batches)} workers={workers}", flush=True)

    def run(batch: list[tuple[int, str]]) -> dict[int, str]:
        session = requests.Session()
        session.headers.update({"User-Agent": "BlueOath-Rebirth-localizer/1.0"})

        def resilient(items: list[tuple[int, str]]) -> dict[int, str]:
            for attempt in range(4):
                try:
                    return translate_batch(items, session)
                except Exception:
                    if attempt == 3:
                        break
                    time.sleep(2.0 * (attempt + 1))
            if len(items) == 1:
                try:
                    index, value = translate_plain(items[0], session)
                    return {index: value}
                except Exception:
                    raise
            middle = len(items) // 2
            result = resilient(items[:middle])
            result.update(resilient(items[middle:]))
            return result

        return resilient(batch)

    done = 0
    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
        futures = [pool.submit(run, batch) for batch in batches]
        for future in concurrent.futures.as_completed(futures):
            for idx, value in future.result().items():
                cache[indexed[idx]] = value
            done += 1
            if done % 10 == 0 or done == len(futures):
                cache_path.write_text(json.dumps(cache, ensure_ascii=False), encoding="utf-8")
                print(f"translated_batches={done}/{len(futures)} cached_strings={len(cache)}", flush=True)
    return cache


def translate_value(
    value: Any,
    mapping: dict[str, str],
    reference: dict[tuple[str, ...], str] | None = None,
    path: tuple[str, ...] = (),
) -> Any:
    if isinstance(value, str):
        if reference and path in reference:
            translated = preserve_source_tokens(value, reference[path])
            if JP_KANA_RE.search(translated):
                cleaned = heuristic_local_translation(translated)
                if cleaned is not None:
                    translated = cleaned
            return translated
        translated = mapping.get(value, mapping.get(normalized_text(value)))
        if isinstance(translated, str):
            translated = translated.replace("\\n", "\n")
            if JP_KANA_RE.search(translated):
                cleaned = heuristic_local_translation(translated)
                if cleaned is not None:
                    translated = cleaned
            translated = preserve_source_tokens(value, translated)
            if JP_KANA_RE.search(translated):
                cleaned = heuristic_local_translation(translated)
                if cleaned is not None:
                    translated = cleaned
            return align_symbols(value, translated)
        patterned = direct_pattern_translation(value)
        if patterned is not None:
            return patterned
        return value
    if isinstance(value, dict):
        return {
            key: translate_value(child, mapping, reference, path + (str(key),))
            for key, child in value.items()
        }
    if isinstance(value, list):
        return [
            translate_value(child, mapping, reference, path + (str(index),))
            for index, child in enumerate(value)
        ]
    return value


def write_databases(
    source: Path,
    output: Path,
    mapping: dict[str, str],
    reference_entries: dict[tuple[str, str, str, tuple[str, ...]], str] | None = None,
) -> tuple[int, int]:
    if output.exists():
        shutil.rmtree(output)
    shutil.copytree(source, output)
    changed_rows = 0
    changed_tables = 0
    reference_by_row: dict[tuple[str, str, str], dict[tuple[str, ...], str]] = {}
    for (db_name, row_id, index_id, path), value in (reference_entries or {}).items():
        reference_by_row.setdefault((db_name, row_id, index_id), {})[path] = value
    for db_path in sorted(output.glob("config_*.db")):
        changed = 0
        with sqlite3.connect(db_path) as db:
            rows = list(db.execute("SELECT id,indexid,jsonbytes FROM DBObject WHERE id <> 'nill'"))
            for rid, indexid, encoded in rows:
                data = json.loads(xor_bytes(encoded).decode("utf-8"))
                if not any(JP_RE.search(value) for _, value in walk_strings(data)):
                    continue
                reference = reference_by_row.get((db_path.name, str(rid), str(indexid)))
                translated = translate_value(data, mapping, reference)
                if translated == data:
                    continue
                payload = json.dumps(translated, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
                db.execute(
                    "UPDATE DBObject SET jsonbytes=? WHERE id=? AND (indexid IS ? OR indexid=?)",
                    (xor_bytes(payload), rid, indexid, indexid),
                )
                changed += 1
            db.commit()
        if changed:
            changed_tables += 1
            changed_rows += changed
    return changed_tables, changed_rows


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--reference", type=Path)
    parser.add_argument("--direct-cache", type=Path)
    parser.add_argument("--workers", type=int, default=4)
    parser.add_argument("--batch-size", type=int, default=850)
    args = parser.parse_args()

    values, indexed = collect_strings(args.source)
    print(f"unique={len(values)} occurrences={sum(values.values())}", flush=True)
    reference_mapping: dict[str, str] = {}
    reference_entries: dict[tuple[str, str, str, tuple[str, ...]], str] = {}
    if args.reference:
        (
            reference_mapping,
            reference_entries,
            reference_occurrences,
            reference_files,
            reference_conflicts,
        ) = collect_reference_mapping(args.source, args.reference)
        print(
            f"reference_files={reference_files} reference_occurrences={reference_occurrences} "
            f"reference_unique={len(reference_mapping)} reference_conflicts={reference_conflicts}",
            flush=True,
        )
    if args.direct_cache:
        direct_mapping = (
            json.loads(args.direct_cache.read_text(encoding="utf-8"))
            if args.direct_cache.exists()
            else {}
        )
        translated = dict(reference_mapping)
        translated.update(expand_normalized_mapping(reference_mapping))
        translated.update(expand_normalized_mapping(direct_mapping))
        translated.update(expand_normalized_mapping(INTERNAL_DIRECT))
        translated.update({value: value for value in indexed.values() if is_existing_chinese(value)})
        patterned_direct = {
            value: patterned
            for value in indexed.values()
            if value not in translated
            and normalized_text(value) not in translated
            and (patterned := direct_pattern_translation(value)) is not None
        }
        translated.update(patterned_direct)
        translated.update(expand_normalized_mapping(patterned_direct))
        pending_direct = [
            value
            for value in indexed.values()
            if value not in translated and normalized_text(value) not in translated
        ]
        print(
            f"direct_cache={len(direct_mapping)} direct_pending={len(pending_direct)}",
            flush=True,
        )
        if pending_direct:
            print("direct_translation_incomplete=true; output_not_written=true", flush=True)
            for value in pending_direct[:100]:
                print(json.dumps(value, ensure_ascii=False), flush=True)
            return 2
    else:
        translated = translate_all(
            indexed,
            args.cache,
            max(1, args.workers),
            args.batch_size,
            reference_mapping,
        )
    tables, rows = write_databases(args.source, args.output, translated, reference_entries)
    print(f"written_output={args.output}", flush=True)
    print(f"changed_tables={tables} changed_rows={rows}", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
