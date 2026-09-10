#!/usr/bin/env python3
"""Offline Simplified-Chinese localization for tasks, achievements, and stage names."""

from __future__ import annotations

import importlib.util
import json
import re
import shutil
import sqlite3
from pathlib import Path
from typing import Any


TARGET_FIELDS = {
    "config_achievement.db": {"title", "desc"},
    "config_awaken_task_common.db": {"desc"},
    "config_awaken_task_milestone.db": {"desc"},
    "config_battlepass_task.db": {"task_name"},
    "config_battlepass_task_activity.db": {"task_name"},
    "config_guildactivitytask.db": {"desc"},
    "config_sportsmeet_achieve.db": {"desc"},
    "config_task_activity.db": {"title", "desc"},
    "config_task_daily.db": {"title", "desc"},
    "config_task_grow.db": {"title", "desc"},
    "config_task_guild.db": {"title", "desc", "extra_reward_desc", "extra_require_desc"},
    "config_task_guildoffer.db": {"title", "desc"},
    "config_task_magazine.db": {"title", "desc"},
    "config_task_main.db": {"title", "desc"},
    "config_task_return.db": {"title", "desc"},
    "config_task_teaching.db": {"title", "desc"},
    "config_task_treaty.db": {"title", "desc"},
    "config_task_weekly.db": {"title", "desc"},
    "config_teaching_achievement.db": {"name", "desc"},
    "config_world_event_task.db": {"title", "desc"},
    "config_chapter.db": {"chapter_openname", "class_name", "name", "name2", "show_name", "title"},
    "config_chapter_plot_type.db": {"name", "plot_enter_name"},
    "config_chapter_type_info.db": {"desc", "title"},
    "config_copy_display.db": {"full_name", "name", "title"},
    "config_daily_chapter.db": {"name", "title"},
    "config_big_activity_chapter.db": {"name", "show_name", "title"},
    "config_big_activity_copy_display.db": {"full_name", "name", "title"},
}

JP_KANA_RE = re.compile(r"[ぁ-ゟァ-ヿ]")
TEXT_TOKEN_RE = re.compile(r"[0-9０-９]+|%[sd]|\{[^}]+\}")


def has_kana(value: str) -> bool:
    return bool(JP_KANA_RE.search(value.replace("・", "").replace("ー", "")))

# Japanese/traditional forms that can survive phonetic conversion.
SIMPLIFIED_MAP = {
    "イベントシナリオ": "活动剧情", "イベント海域": "活动海域", "チャプター": "第",
    "プロローグ": "序章", "出撃": "出击", "毎日": "每日", "毎週": "每周", "週間": "每周",
    "一週間": "一周", "任務": "任务", "ミッション": "任务", "クエスト": "任务", "クリア": "通关",
    "勝利": "胜利", "敗北": "失败", "報酬": "奖励", "獲得": "获得", "戦姫": "战姬",
    "艦隊": "舰队", "編成": "编队", "戦闘": "战斗", "攻撃": "攻击", "敵艦": "敌舰",
    "海域調査": "海域调查", "海域": "海域", "装備": "装备", "強化": "强化", "改造": "改造",
    "建造": "建造", "探索": "探索", "消費": "消耗", "達成": "达成", "解除": "解除",
    "開放": "开放", "解放": "解锁", "登場": "登场", "限定": "限定", "特別": "特别",
    "高級": "高级", "優先": "优先", "難易度": "难度", "選択": "选择", "選別": "精选",
    "推薦": "推荐", "資源": "资源", "物資": "物资", "燃料": "燃料", "弾薬": "弹药",
    "経験値": "经验值", "レベル": "等级", "個": "个",
    "時": "时", "間": "间", "毎": "每", "週": "周", "増": "增", "錬": "炼", "願": "愿",
    "資": "资", "別": "别", "補": "补", "給": "给", "厳": "严", "後": "后", "長": "长",
    "門": "门", "強": "强", "將": "将", "達": "达", "機": "机", "艦": "舰", "戦": "战",
    "姫": "姬", "鶴": "鹤", "鴎": "鸥", "選": "选", "擇": "择", "獲": "获", "報": "报",
    "會": "会", "來": "来", "與": "与", "為": "为", "這": "这", "們": "们", "國": "国",
    "體": "体", "實": "实", "務": "务", "還": "还", "點": "点", "對": "对", "應": "应",
    "於": "于", "樂": "乐", "學": "学", "業": "业", "書": "书", "車": "车", "東": "东",
    "兩": "两", "裡": "里", "現": "现", "號": "号", "總": "总", "廣": "广", "確": "确",
    "經": "经", "種": "种", "節": "节", "買": "买", "賣": "卖", "屬": "属", "臺": "台",
    "無": "无", "優": "优", "萬": "万", "進": "进", "見": "见", "從": "从", "開": "开",
    "說": "说", "彈": "弹", "砲": "炮", "驅": "驱", "輕": "轻", "敵": "敌", "納": "纳",
    "際": "际", "邊": "边", "傳": "传", "續": "续", "認": "认", "讓": "让", "頁": "页",
    "貨": "货", "價": "价", "貴": "贵", "賽": "赛", "贈": "赠", "購": "购", "費": "费",
    "產": "产", "預": "预", "備": "备", "錄": "录", "釋": "释", "獻": "献", "類": "类",
    "庫": "库", "單": "单", "組": "组", "編": "编", "製": "制", "裝": "装", "飾": "饰",
    "發": "发", "証": "证", "憶": "忆", "揚": "扬", "勲": "勋", "薬": "药", "撃": "击",
    "専": "专", "許": "许", "結": "结", "鋭": "锐", "銀": "银", "氣": "气", "勝": "胜",
    "負": "负", "隊": "队", "護": "护", "錬金": "炼金", "祈願": "祈愿", "供給": "补给",
}

LOCAL_EXACT = {
    "初稼ぎ": "第一桶金", "石油備蓄": "石油储备", "戦姫パラダイス": "战姬乐园", "戦術研修": "战术训练",
    "温泉体験": "温泉体验", "サルベージ": "打捞", "装備品獲得": "获得装备", "装備品調達": "装备采购",
    "勢力拡大": "势力扩张", "海域を目指して": "目标海域", "救出計画": "救援计划", "セルフチェック": "自我检查",
    "パンドラの装備箱": "潘多拉的装备箱", "備えあれば憂いなし": "有备无患", "リサイクル": "回收",
    "初心者の始まり": "新手启程", "賢者の石": "贤者之石", "買い物": "购物", "技の精進": "技艺精进",
    "もっと高みへと": "向更高处", "小さな四天王": "小小四天王", "スキル強化": "技能强化",
    "効率の高い学習法": "高效学习法", "火力が足りないなら装備で補え": "火力不足就用装备弥补",
    "様になっているじゃないか": "这不是有模有样嘛", "技の真髄": "技艺真髓", "束ね役": "统领者",
    "専売特許": "专利", "地獄モード": "地狱模式", "己を超えろ": "超越自我", "八門遁甲": "八门遁甲",
    "追撃訓練": "追击训练", "装備収集訓練": "装备收集训练", "デイリークエスト訓練": "每日任务训练",
    "戦姫強化物資整備": "战姬强化物资整备", "建造物資整備": "建造物资整备", "戦姫建造": "战姬建造",
    "戦姫除隊": "战姬退役", "装備強化": "装备强化", "初登場": "首次登场", "祈願壁": "祈愿墙",
    "イベントシナリオ": "活动剧情", "プロローグ": "序章", "思い出話": "回忆往事", "お散歩": "散步",
    "スキンシップ": "亲密互动", "先輩の時間": "前辈的时间", "秘密にして？": "替我保密？", "暇つぶし": "消磨时间",
    "宝箱の行方": "宝箱的去向", "雨宿り編": "避雨篇", "花咲き物語·前編": "花开物语·前篇",
    "花咲き物語·後編": "花开物语·后篇", "アイス＆スパイ": "冰淇淋与间谍", "風に吹かれて": "迎风而行",
    "チャプター1": "第1章", "チャプター2": "第2章", "特別許可": "特别许可",
}

SOURCE_TERM_MAP = {
    "パラダイス": "乐园", "セルフチェック": "自我检查", "サルベージ": "打捞", "リサイクル": "回收",
    "ボトムレス": "无底", "アビス": "深渊",
    "アンブラ進軍": "安布拉进军", "オート戦闘": "自动战斗", "早送り": "快进", "ファラガット": "法拉格特", "マハン": "马汉",
    "ブルックリン": "布鲁克林", "強化効果": "强化效果", "効果": "效果", "1つ": "1个", "つ": "个",
    "オースソーダ": "奥斯苏打", "または": "或", "から": "从", "にて": "在", "ランダムに": "随机",
    "累計": "累计", "または": "或", "入荷": "获取", "オースソーダ": "奥斯苏打", "ランダムに": "随机",
    "キャンディーポイント": "糖果点数", "キャンディ": "糖果", "ラッキーボール": "幸运球", "ティータイムチップ": "下午茶筹码",
    "金祝ハンコ": "金色印章", "モテモテ": "人气", "ステルスコーティング材": "隐形涂层材料", "赤い結晶エキス": "红色结晶精华",
    "瞬足の道": "瞬足之道", "瞬時大破薬": "瞬时大破药", "アンノウン": "未知", "シェフの暴走": "厨师暴走",
    "チーム": "团队", "デイリー": "每日", "ディリー": "每日", "ヴィットリオ・ヴェネト": "维托里奥·维内托",
    "ヴィットリオ": "维托里奥", "ヴェネト": "维内托", "グラム": "克", "蕾綻ぶ季節": "花蕾绽放的季节",
    "すべて": "全部", "除く": "不包括", "にて": "在", "派遣して": "派遣后", "隻": "艘", "連装": "联装", "三連装": "三联装", "級": "级",
    "クリスマス": "圣诞", "ハロウィン": "万圣节", "ブリキ": "锡制", "アンコウ": "鮟鱇", "パールベイ": "珍珠湾",
    "パトロル": "巡逻", "パトロール任務": "巡逻任务", "うわぁ！何あれ！": "哇！那是什么！", "オフィスレベル": "办公室等级",
    "神通の気持ち": "神通的心情", "三人の誓い": "三人的誓约", "怒鳴": "怒吼", "暗雲迫る": "乌云逼近",
    "黒雲立ち込める": "乌云密布", "開いた": "开启的", "オフィス": "办公室", "ゴールド": "金色",
    "クレムソン": "克莱姆森", "ムーバー": "移动者", "嵐": "暴风雨", "静けさ": "宁静", "高波": "巨浪",
    "大規模作戦": "大型作战", "起動": "启动", "除く": "不包括", "行う": "进行", "使用する": "使用",
    "消費する": "消耗", "獲得する": "获得", "所有": "拥有", "大破": "大破", "させる": "",
    "満耐久値": "满耐久值", "未満": "以下", "試合": "比赛", "登録": "登记", "変更": "调整", "調整": "调整",
    "手動": "手动", "受取": "领取", "受け取る": "领取", "生産品": "产出物", "防衛": "防卫",
    "届く": "抵达", "星々": "群星", "始まり": "起点", "輝く": "闪耀", "手紙": "信件", "決意": "决意",
    "誓約": "誓约", "絆": "羁绊", "海香る": "海风拂面", "宿命": "宿命", "極密": "绝密", "水平線": "地平线",
    "彼方からの来客": "远方来客", "敵襲": "敌袭", "苦戦": "苦战", "攻防": "攻防", "装備品": "装备",
    "勢力": "势力", "拡大": "扩大", "調達": "采购", "計画": "计划", "入門": "入门", "初級": "初级",
    "中級": "中级", "上級": "高级", "極": "极", "初心者": "新手", "整備": "整备", "復刻版": "复刻版",
    "デイリークエスト": "每日任务", "クエスト": "任务", "ミッション": "任务", "クリアする": "通关",
    "イベント": "活动", "シナリオ": "剧情", "デモ海域": "演示海域", "追撃戦": "追击战",
    "救難信号": "救援信号", "防衛線": "防卫线", "戰姫": "战姬", "戦い": "战斗", "姉妹": "姐妹",
    "戦艦": "战列舰", "空母": "航空母舰", "重巡洋艦": "重巡洋舰", "軽巡洋艦": "轻巡洋舰", "駆逐艦": "驱逐舰",
    "巡洋艦": "巡洋舰", "指揮官": "指挥官", "レベル": "等级", "到達": "达到", "戦術": "战术",
    "騎士団": "骑士团", "蒼藍": "苍蓝", "蒼": "苍", "藍": "蓝", "黒": "黑", "闇": "暗",
    "風": "风", "雪": "雪", "着飾り": "装扮", "遅し": "迟", "幕間": "幕间", "終幕": "终幕",
    "奇襲": "奇袭", "火力偵察": "火力侦察", "偵察": "侦察", "時空ゲート": "时空之门", "ゲート": "大门",
    "目指せ": "目标", "ソロモン": "所罗门", "向こう": "另一边", "強化物資": "强化物资", "収集": "收集",
    "整備": "整备", "訓練": "训练", "級": "级", "第Ⅲフェイズ": "第III阶段", "火力": "火力",
    "タイトル": "标题", "女王様": "女王大人", "金欠": "缺钱", "生死時速": "生死时速", "生死时速": "生死时速",
    "大艦隊": "大舰队", "連装": "联装", "三連装": "三联装", "単装": "单装", "寄贈": "捐赠",
    "全て": "全部", "EXモード": "EX模式", "総評価": "总评价", "或いは": "或者", "設立": "创建", "加入": "加入", "所有": "拥有",
    "強化効果": "强化效果", "クエストドロップ": "任务掉落", "スキル学習": "技能学习", "から": "从",
    "する": "", "を": "", "の": "的", "と": "与", "へ": "至", "で": "在", "に": "于",
}

MODEL_TASK_EXACT = {
    "累計で燃料100000を獲得する": "累计获得燃料100000", "累計で燃料200000を獲得する": "累计获得燃料200000",
    "累計で燃料300000を獲得する": "累计获得燃料300000", "累計で燃料400000を獲得する": "累计获得燃料400000",
    "累計で燃料500000を獲得する": "累计获得燃料500000", "累計で燃料50000を消費する": "累计消耗燃料50000",
    "戦姫30000名を獲得する": "获得30000名战姬", "任意の駆逐艦がレベル80に到達する": "任意驱逐舰等级达到80级",
    "任意の巡洋戦艦がレベル80に到達する": "任意战列巡洋舰等级达到80级", "任意の戦艦がレベル80に到達する": "任意战列舰等级达到80级",
    "任意の空母がレベル80に到達する": "任意航空母舰等级达到80级", "戦姫６名がレベル20に到達する": "6名战姬等级达到20级",
    "戦姫６名がレベル30に到達する": "6名战姬等级达到30级", "戦姫６名がレベル40に到達する": "6名战姬等级达到40级",
    "戦姫６名がレベル50に到達する": "6名战姬等级达到50级", "戦姫６名がレベル60に到達する": "6名战姬等级达到60级",
    "戦姫６名がレベル70に到達する": "6名战姬等级达到70级", "任意のスキル３つがレベル4に到達する": "任意3个技能等级达到4级",
    "任意のスキル6個がレベル7に到達する": "任意6个技能等级达到7级",
}

MODEL_TASK_TERMS = {
    "戦姫": "战姬", "戦艦": "战列舰", "巡洋戦艦": "战列巡洋舰", "重巡洋艦": "重巡洋舰", "軽巡洋艦": "轻巡洋舰",
    "駆逐艦": "驱逐舰", "空母": "航空母舰", "スキル": "技能", "燃料": "燃料", "物資": "物资",
    "レベル": "等级", "到達": "达到", "獲得": "获得", "消費": "消耗", "累計": "累计", "クエスト": "任务",
    "デイリー": "每日", "任務": "任务", "装備": "装备", "強化": "强化", "戦術": "战术", "派遣": "派遣",
    "ダイヤ": "钻石",
    "任意の": "任意", "異なる": "不同的", "すべての": "全部", "全ての": "全部", "の": "的",
}


def model_task_terms(value: str) -> str:
    for source, target in sorted(MODEL_TASK_TERMS.items(), key=lambda item: -len(item[0])):
        value = value.replace(source, target)
    return value.replace("・", "·").replace("ー", "—")


def model_task_translation(value: str) -> str | None:
    quoted = value.startswith('"') and value.endswith('"')
    core = value[1:-1] if quoted else value
    translated = MODEL_TASK_EXACT.get(core)
    if translated is None:
        match = re.fullmatch(r"累計で(.+?)([0-9０-９]+)を(獲得|消費)する", core)
        if match:
            verb = "获得" if match.group(3) == "獲得" else "消耗"
            translated = f"累计{verb}{model_task_terms(match.group(1))}{match.group(2)}"
        match = re.fullmatch(r"任意の(.+?)がレベル([0-9０-９]+)に到達する", core)
        if match:
            subject = model_task_terms(match.group(1))
            translated = f"任意{subject}等级达到{match.group(2)}级"
        match = re.fullmatch(r"戦姫([0-9０-９]+)名がレベル([0-9０-９]+)に到達する", core)
        if match:
            translated = f"{match.group(1)}名战姬等级达到{match.group(2)}级"
        match = re.fullmatch(r"任意のスキル([0-9０-９]+)(?:つ|個)がレベル([0-9０-９]+)に到達する", core)
        if match:
            translated = f"任意{match.group(1)}个技能等级达到{match.group(2)}级"
    if translated is None:
        return None
    translated = model_task_terms(translated)
    return f'"{translated}"' if quoted else translated


def map_source_terms(value: str, translator: Any) -> str:
    maps = (LOCAL_EXACT, translator.INTERNAL_DIRECT, SOURCE_TERM_MAP, SIMPLIFIED_MAP, translator.GLOSSARY)
    pairs = []
    seen = set()
    for mapping in maps:
        for source, target in mapping.items():
            if source and source not in seen:
                pairs.append((source, target))
                seen.add(source)
    protected = {}
    for index, (source, target) in enumerate(sorted(pairs, key=lambda item: -len(item[0]))):
        marker = f"ZXLOCAL{index:05d}ZX"
        if source in value:
            value = value.replace(source, marker)
            protected[marker] = target
    for marker, target in protected.items():
        value = value.replace(marker, target)
    return value


def translate_pattern(value: str, translator: Any) -> str | None:
    direct = LOCAL_EXACT.get(value)
    if direct is not None:
        return direct
    match = re.fullmatch(r"チャプター(\d+)[・·](.*)", value)
    if match:
        suffix = map_source_terms(match.group(2), translator)
        suffix = simplify_text(suffix)
        return f"第{match.group(1)}章·{suffix}"
    patterns = [
        (r"温泉で強化効果1つを獲得する", lambda _m: "在温泉获得1个强化效果"),
        (r"クエストドロップから戦姫(\d+)名を獲得する", lambda m: f"从任务掉落中获得{m.group(1)}名战姬"),
        (r"スキル学習のときに学習加速を1回行う", lambda _m: "在技能学习时使用1次学习加速"),
        (r"オート戦闘で任意のクエストを1回クリアする", lambda _m: "使用自动战斗通关任意任务1次"),
        (r"3倍速の早送りで任意のクエストを1回クリアする", lambda _m: "使用3倍速快进通关任意任务1次"),
        (r"安全レベルが「安全」になった海域クエスト(\d+-\d+)を1回クリアする", lambda m: f"通关安全等级变为“安全”的海域任务{m.group(1)}1次"),
        (r"海域の安全レベルが「安全」になったあと、任意のデイリークエストを1回クリアする", lambda _m: "海域安全等级变为“安全”后，通关任意每日任务1次"),
        (r"この段階の全ての目標を達成する", lambda _m: "完成当前阶段全部目标"),
        (r"異なる戦姫(\d+)名を獲得する", lambda m: f"获得{m.group(1)}名不同的战姬"),
        (r"ムーバー防衛線(.+?)級をクリアする", lambda m: f"通关移动者防卫线{map_source_terms(m.group(1), translator)}级"),
        (r"最低(\d+)隻(.+?)を派遣して、任意の海域クエストを1回クリアする", lambda m: f"派遣至少{m.group(1)}艘{map_source_terms(m.group(2), translator)}，并通关任意海域任务1次"),
        (r"(.+?)の([A-Z0-9-]+)を(\d+)回クリアする", lambda m: f"通关{map_source_terms(m.group(1), translator)}的{m.group(2)}{m.group(3)}次"),
        (r"【毎日任務】ランダムにイベント海域を(\d+)回クリアする", lambda m: f"通关【每日任务】随机活动海域{m.group(1)}次"),
        (r"累計ハロウィンのキャンディ(\d+)個を獲得する", lambda m: f"累计获得万圣节糖果{m.group(1)}个"),
        (r"累計ラッキーボール(\d+)点を獲得する", lambda m: f"累计获得幸运球{m.group(1)}点"),
        (r"累計キャンディーポイント(\d+)点を獲得する", lambda m: f"累计获得糖果点数{m.group(1)}点"),
        (r"累計ティータイムチップ(\d+)点を獲得する", lambda m: f"累计获得下午茶筹码{m.group(1)}点"),
        (r"累計金祝ハンコ(\d+)点を獲得する", lambda m: f"累计获得金色印章{m.group(1)}点"),
        (r"アンコウ(\d+)グラムを獲得する", lambda m: f"获得鮟鱇{m.group(1)}克"),
        (r"すべてのディリー任務をクリアする", lambda _m: "通关全部每日任务"),
        (r"全てのEXモードクエストの総評価★([0-9０-９]+)を獲得する", lambda m: f"获得全部EX模式任务的总评价★{m.group(1)}"),
        (r"指揮官レベル(\d+)に到達(?:する)?", lambda m: f"达到指挥官等级{m.group(1)}"),
        (r"基地のオフィスレベルが(\d+)に到達する", lambda m: f"基地办公室等级达到{m.group(1)}"),
        (r"(\d+)レベル以上のゴールド装備を1つ所有", lambda m: f"拥有1件等级{m.group(1)}以上的金色装备"),
        (r"海域(\d+-\d+)をクリアする", lambda m: f"通关海域{m.group(1)}"),
        (r"海域(\d+-\d+)をクリアする\s+(.+)", lambda m: f"通关海域{m.group(1)} {map_source_terms(m.group(2), translator)}"),
        (r"(.+?)をレベル([0-9０-９]+)に到達させる", lambda m: f"将{map_source_terms(m.group(1), translator)}等级提升至{m.group(2)}级"),
        (r"(.+?)を([0-9０-９]+)回クリアする", lambda m: f"通关{map_source_terms(m.group(1), translator)}{m.group(2)}次"),
        (r"(.+?)を([0-9０-９]+)回行う", lambda m: f"进行{map_source_terms(m.group(1), translator)}{m.group(2)}次"),
        (r"(.+?)を([0-9０-９]+)回使用する", lambda m: f"使用{map_source_terms(m.group(1), translator)}{m.group(2)}次"),
        (r"(.+?)を([0-9０-９]+)回消費する", lambda m: f"消耗{map_source_terms(m.group(1), translator)}{m.group(2)}次"),
        (r"(.+?)を([0-9０-９]+)を獲得する", lambda m: f"获得{map_source_terms(m.group(1), translator)}{m.group(2)}"),
        (r"(.+?)を([0-9０-９]+)個を獲得する", lambda m: f"获得{map_source_terms(m.group(1), translator)}{m.group(2)}个"),
        (r"(.+?)を([0-9０-９]+)体大破させる", lambda m: f"使{map_source_terms(m.group(1), translator)}{m.group(2)}艘大破"),
        (r"(.+?)を1つ所有", lambda m: f"拥有1件{map_source_terms(m.group(1), translator)}"),
        (r"(.+?)を所有", lambda m: f"拥有{map_source_terms(m.group(1), translator)}"),
        (r"全てのEXモードクエストの総評価★([0-9０-９]+)を獲得する", lambda m: f"获得全部EX模式任务的总评价★{m.group(1)}"),
        (r"(.+?)をクリアする", lambda m: f"通关{map_source_terms(m.group(1), translator)}"),
        (r"(.+?)に参加する", lambda m: f"参加{map_source_terms(m.group(1), translator)}"),
        (r"(.+?)に登録する", lambda m: f"登记{map_source_terms(m.group(1), translator)}"),
        (r"(.+?)を達成する", lambda m: f"达成{map_source_terms(m.group(1), translator)}"),
        (r"(.+?)を獲得する", lambda m: f"获得{map_source_terms(m.group(1), translator)}"),
        (r"([0-9０-９]+)日目のデイリー任務をクリアする", lambda m: f"通关第{m.group(1)}天的每日任务"),
        (r"任意のデイリークエスト難度([0-9０-９]+)をクリアする", lambda m: f"通关任意每日任务难度{m.group(1)}"),
        (r"祈願でSSR戦姫(\d+)名獲得", lambda m: f"通过祈愿获得{m.group(1)}名SSR战姬"),
        (r"戦姫(\d+)(?:名|人)を☆([0-9０-９]+)まで突破させる", lambda m: f"将{m.group(1)}名战姬突破至☆{m.group(2)}"),
        (r"戦姫(\d+)名のスキルをレベル([0-9０-９]+)に到達させる", lambda m: f"将{m.group(1)}名战姬的技能提升至{m.group(2)}级"),
        (r"艦隊の戦闘力が(\d+)を超える", lambda m: f"舰队战斗力超过{m.group(1)}"),
        (r"大艦隊に加入或いは設立する", lambda _m: "加入或创建大舰队"),
        (r"N級戦姫収集·([一二三四五六七八九十]+)", lambda m: f"N级战姬收集·{m.group(1)}"),
        (r"R級戦姫収集·([一二三四五六七八九十]+)", lambda m: f"R级战姬收集·{m.group(1)}"),
    ]
    for pattern, replace in patterns:
        match = re.fullmatch(pattern, value)
        if match:
            return replace(match)
    return None


def simplify_text(value: str) -> str:
    for source, target in sorted(SIMPLIFIED_MAP.items(), key=lambda item: -len(item[0])):
        value = value.replace(source, target)
    return value.replace("ー", "").replace("・", "·")


def load_helpers(repo_root: Path):
    path = repo_root / "tools" / "localize-ship-names.py"
    spec = importlib.util.spec_from_file_location("blueoath_task_helpers", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_cn_reference(reference_root: Path, file_name: str) -> dict[str, Any]:
    """Load optional CN catalog JSON keyed by DB row id.

    CN catalog is a partial reference: only non-empty values without Japanese
    kana are eligible. Missing or still-Japanese fields fall back to local
    translation below.
    """
    json_path = reference_root / f"{Path(file_name).stem}.json"
    if not json_path.exists():
        return {}
    try:
        document = json.loads(json_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    rows = None
    if isinstance(document, dict):
        rows = document.get("Rows", document.get("rows"))
    if not isinstance(rows, list):
        return {}
    result: dict[str, Any] = {}
    for row in rows:
        if not isinstance(row, dict):
            continue
        row_id = row.get("Id", row.get("id"))
        if row_id is None:
            continue
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


def translate_text(value: str, translator: Any) -> str:
    model_translated = model_task_translation(value)
    if model_translated is not None:
        return simplify_text(model_translated)
    translated = translate_pattern(value, translator)
    if translated is not None:
        return simplify_text(translated)
    translated = translator.INTERNAL_DIRECT.get(value)
    if translated is not None:
        return simplify_text(translated)
    source_mapped = map_source_terms(value, translator)
    if not has_kana(source_mapped):
        return simplify_text(source_mapped)
    translated = translator.direct_pattern_translation(value)
    if translated is None:
        translated = translator.heuristic_local_translation(source_mapped)
    if translated is None:
        translated = translator.heuristic_local_translation(value)
    if translated is None:
        translated = source_mapped
    if has_kana(translated):
        cleaned = translator.heuristic_local_translation(translated)
        if cleaned is not None:
            translated = cleaned
    return simplify_text(translated)


def translate_value(value: Any, translator: Any, reference: Any = None) -> tuple[Any, int, int, int, int]:
    if isinstance(value, str):
        if usable_cn_reference(reference, value):
            return reference, 1, 1, 0, 1
        result = translate_text(value, translator)
        return result, 1, int(result != value), int(has_kana(result)), 0
    if isinstance(value, list):
        out = []
        counts = [0, 0, 0, 0]
        references = reference if isinstance(reference, list) else []
        for index, item in enumerate(value):
            item_reference = references[index] if index < len(references) else None
            mapped, seen, changed, residual, ref_used = translate_value(item, translator, item_reference)
            out.append(mapped)
            counts[0] += seen; counts[1] += changed; counts[2] += residual; counts[3] += ref_used
        return out, *counts
    if isinstance(value, dict):
        out = {}
        counts = [0, 0, 0, 0]
        for key, item in value.items():
            item_reference = reference.get(key) if isinstance(reference, dict) else None
            mapped, seen, changed, residual, ref_used = translate_value(item, translator, item_reference)
            out[key] = mapped
            counts[0] += seen; counts[1] += changed; counts[2] += residual; counts[3] += ref_used
        return out, *counts
    return value, 0, 0, 0, 0


def main() -> int:
    repo_root = Path(__file__).resolve().parents[1]
    client_root = Path(r"E:\BlueOath Rebirth")
    config_root = client_root / "blueoath" / "blueoath_Data" / "StreamingAssets" / "config"
    server_root = client_root / "客户端补丁" / "server" / "catalog" / "config"
    cn_reference_root = client_root / "国服config"
    work_root = client_root / "任务成就关卡汉化"
    source_dir = work_root / "source-db"
    decoded_dir = work_root / "decoded-json"
    translated_dir = work_root / "translated-json"
    compiled_dir = work_root / "compiled-db"
    backup_dir = work_root / "backup" / "client-config"
    for directory in (source_dir, decoded_dir, translated_dir, compiled_dir, backup_dir):
        directory.mkdir(parents=True, exist_ok=True)

    helpers = load_helpers(repo_root)
    translator = helpers.load_local_translator(repo_root)
    cn_reference = {file_name: load_cn_reference(cn_reference_root, file_name) for file_name in TARGET_FIELDS}
    summary: dict[str, dict[str, int]] = {}
    mappings: list[dict[str, str]] = []

    for file_name, fields in TARGET_FIELDS.items():
        client_db = config_root / file_name
        candidate_source = server_root / file_name
        preserved_source = source_dir / file_name
        if usable_db(candidate_source):
            source_db = candidate_source
        elif usable_db(preserved_source):
            source_db = preserved_source
        else:
            source_db = client_db
        if not client_db.exists() or not source_db.exists():
            raise FileNotFoundError(file_name)
        if source_db.resolve() != (source_dir / file_name).resolve():
            shutil.copy2(source_db, source_dir / file_name)
        shutil.copy2(source_db, compiled_dir / file_name)
        if client_db.resolve() != (backup_dir / file_name).resolve():
            shutil.copy2(client_db, backup_dir / file_name)

        con = sqlite3.connect(source_db)
        rows = con.execute("select id,indexid,jsonbytes from DBObject order by id").fetchall()
        con.close()
        decoded = []
        translated_rows = []
        changes = []
        stats = {"rows": 0, "valid_json": 0, "text_fields": 0, "changed": 0, "cn_reference": 0, "jp_after": 0}
        references = cn_reference.get(file_name, {})
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
                for field in fields:
                    if field not in payload:
                        continue
                    row_reference = references.get(row_id)
                    field_reference = row_reference.get(field) if isinstance(row_reference, dict) else None
                    mapped, seen, changed, residual, ref_used = translate_value(payload[field], translator, field_reference)
                    stats["text_fields"] += seen
                    stats["changed"] += changed
                    stats["cn_reference"] += ref_used
                    stats["jp_after"] += residual
                    if mapped != payload[field]:
                        if new_payload is payload:
                            new_payload = dict(payload)
                        new_payload[field] = mapped
                        mappings.append({"file": file_name, "id": row_id, "field": field, "source": "cn-reference" if ref_used else "internal", "original": json.dumps(payload[field], ensure_ascii=False), "translated": json.dumps(mapped, ensure_ascii=False)})
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
