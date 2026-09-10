#!/usr/bin/env python3
"""Offline Simplified-Chinese localization for second safe text batch."""

from __future__ import annotations

import importlib.util
import json
import re
import shutil
import sqlite3
from pathlib import Path
from typing import Any


TARGET_FIELDS = {
    "config_activity.db": {"activity_top", "name", *(f"p{i}" for i in range(1, 15))},
    "config_activity_extract.db": {"name"},
    "config_buildinginfo.db": {"name", "desc"},
    "config_challenge.db": {"name", "message1", "message2"},
    "config_daily_group.db": {"shop_name", "copy_dropinfo"},
    "config_home_activity_enter.db": {"activity_name"},
    "config_player_head_frame.db": {"description", "name"},
    "config_strategy.db": {"strategy_dec1", "strategy_dec2", "strategy_dec3", "strategy_name", "tip", "tip1", "tip2", "tip3"},
    "config_interaction_item.db": {
        "effect_item_name", "interaction_event", "interaction_item_desc", "interaction_item_detail",
        "interaction_item_name", "item_name", "item_name_dawn", "item_name_night",
    },
    "config_magazine_page.db": {"contents_desc"},
    "config_magazine_tag.db": {"name"},
    "config_tower_topic.db": {"area_final_copy", "copy_list", "copy_list_name", "features_list", "name"},
    "config_treaty_buff.db": {"desc", "name"},
}

JP_KANA_RE = re.compile(r"[ぁ-ゟァ-ヿ]")
TEXT_TOKEN_RE = re.compile(r"[0-9０-９]+|%[sd]|\{[^}]+\}")
JP_KANJI_HINTS = ("戦", "艦", "姫", "獲得", "消費", "任務", "報酬", "装備", "素材", "祈願", "短縮", "使用", "時間", "魚雷", "砲撃", "雷撃", "耐久", "敵艦", "指揮官")

MODEL_EXACT = {
    "イベント": "活动",
    "覚ませ!\n赤い悪夢!": "醒来！\n红色噩梦！",
    "無限ガチャ": "无限扭蛋",
    "ガチャ④": "扭蛋④",
    "オースエネルギー室": "奥斯能源室",
    "パールベイ食堂": "珍珠湾食堂",
    "ミニゲーム": "小游戏",
    "ムーバー防衛線": "移动者防卫线",
    "今すぐみんなと一緒に": "现在就和大家一起",
    "戦闘に参加しましょう！": "一起来参加战斗吧！",
    "戦姫ショップ": "战姬商店",
    "スキル強化": "技能强化",
    "指令イベント\n(スペシャル)": "指令活动\n（特别）",
    "黄金の秋フレーム": "金秋头像框",
    "「赤と青」フレーム": "“红与蓝”头像框",
    "雷撃バリア戦術": "雷击屏障战术",
    "砲撃バリア戦術": "炮击屏障战术",
    "編成条件に満たない": "未满足编队条件",
    "聖誕ツリー": "圣诞树",
    "ポジティブ": "积极",
    "為将の力": "为将之力",
    "格納庫の故障Ⅰ": "机库故障Ⅰ",
    "美食マスターの道": "美食大师之道",
    "弥生の宝箱": "弥生宝箱", "卯月の宝箱": "卯月宝箱", "皐月の宝箱": "皋月宝箱", "睦月の宝箱": "睦月宝箱",
    "水無月の宝箱": "水无月宝箱", "文月の宝箱": "文月宝箱", "師走の宝箱": "师走宝箱", "如月の宝物": "如月宝物",
    "葉月の宝物": "叶月宝物", "長月の宝物": "长月宝物", "神無月の宝物": "神无月宝物", "霜月の宝物": "霜月宝物",
    "皐月の宝物": "皋月宝物", "水無月の宝物": "水无月宝物", "文月の宝物": "文月宝物",
    "<size=16>御礼ログイン</size>": "<size=16>答谢登录</size>", "御礼ログイン": "答谢登录",
    "<size=16>コラボイベント</size>": "<size=16>联动活动</size>", "訓練イベント": "训练活动",
    "アラスカ期間限定出現率アップキャンペーン": "阿拉斯加限时概率提升活动",
    "Ryza-期間限定出現率UPガチャ": "Ryza-限时概率提升扭蛋",
    "伊168-期間限定出現率UPガチャ": "伊168-限时概率提升扭蛋",
    "新年に迎え!雪の玉を集めましょう！": "迎接新年！一起收集雪球吧！",
    "タフィ‐3サインイン": "塔菲-3签到", "タフィ3ビッグイベント": "塔菲3大型活动",
    "新装備テスト": "新装备测试", "クリスマス家具": "圣诞家具", "クリスマス物語": "圣诞故事",
    "少女の着飾り": "少女换装", "春遅し": "春迟", "大艦巨砲はロマンである": "大舰巨炮就是浪漫",
    "雪降り編": "降雪篇", "イベント交換所": "活动兑换所", "空母専属-装備ガチャ": "航空母舰专属-装备扭蛋",
    "重巡専属-装備ガチャ": "重巡洋舰专属-装备扭蛋", "ぶらり交換所": "漫游兑换所", "轻巡専属-装備ガチャ": "轻巡洋舰专属-装备扭蛋",
    "ケーキ祭り": "蛋糕祭典", "人気戦姫": "人气战姬", "声優の祝福": "声优的祝福", "朝日の思い出": "朝阳的回忆",
    "瑞鶴パネル": "瑞鹤面板", "GWログイン": "黄金周登录", "雨宿り編": "避雨篇", "祈願ログイン": "祈愿登录",
    "戦姫の休日": "战姬的假日", "流鏑馬": "流镝马", "舞踊": "舞蹈", "デコボコ探索調査": "高低探索调查",
    "デコボコ探索ご褒美": "高低探索奖励", "バカンス計画": "度假计划", "任務イベント": "任务活动", "正統編": "正统篇",
    "着せ替え": "换装", "ミニ戦姫": "迷你战姬", "ガチャイベント": "扭蛋活动", "夏海の輝き": "夏海之光",
    "ログイン": "登录", "祝100日": "庆祝100天", "星屑ドレス": "星尘礼服", "青の世界": "蓝色世界", "赤の世界": "红色世界",
    "アンコウ革命": "鮟鱇革命", "ぶらり武装巡り": "漫游武装巡礼", "戦域の友": "战域之友", "カウントダウン": "倒计时",
    "春の大運動会": "春季大运动会", "賞品交換所": "奖品兑换所", "キャンデー大作戦": "糖果大作战",
    "幸運はダイスから！": "幸运来自骰子！", "冬のログインボーナス": "冬季登录奖励", "ヴェネトお嬢様の奮闘伝": "维内托小姐的奋斗传",
    "ヴェネトお嬢様の大願成就": "维内托小姐的大愿得成", "浮生夢の如し": "浮生若梦", "春浅し": "春浅",
    "つぼみ芽吹き": "花蕾萌发", "花咲き物語": "花开物语", "桜が舞う季節": "樱花飞舞的季节", "謎の開発機": "神秘开发机",
    "蕾綻ぶ季節に": "花蕾绽放的季节", "島々の間を吹き抜ける風": "穿过群岛的风", "旅人たちと耳元の囁き": "旅人耳畔的低语",
    "夏姫祭": "夏姬祭", "人間万事 塞翁が馬": "人间万事，塞翁失马", "咲き誇り花のように": "如盛放的花朵",
    "海鶴の宴": "海鹤之宴", "約束のアイスクリーム": "约定的冰淇淋", "アイス＆スパイ": "冰淇淋与间谍",
    "七夕期間限定ギフトバック": "七夕限时礼包", "復刻版！ムーバーゼータ！": "复刻版！移动者Zeta！",
    "悪霊退散！ハロウィン大冒険": "恶灵退散！万圣节大冒险", "辺境の戦い": "边境之战", "辺境防衛": "边境防卫",
    "ゴブ戦の報酬": "哥布林战奖励", "朝日の占い屋": "朝阳占卜屋", "双星の姉妹": "双星姐妹", "除夜の鐘は響き渡る": "除夕钟声回荡",
    "毎日おみくじ": "每日抽签", "コラボ": "联动", "ドロップ率UP": "掉落率提升", "ソロモンの戦い": "所罗门之战",
    "ブルースフィアの来訪者": "蓝色星球的访客", "最強艦隊": "最强舰队", "宝物の行方": "宝物的去向",
    "指令イベント\n(スペシャル)": "指令活动\n（特别）", "ときパル": "心跳珍珠湾",
    "異世界電子レンジ": "异世界微波炉", "水風船プール": "水球泳池", "華奢な生け花": "精致插花",
    "ハロウィンのかぼちゃ": "万圣节南瓜", "ビスマルクの愛猫": "俾斯麦的爱猫", "聖夜のトナカイ": "圣夜驯鹿",
    "クリスマスツリー": "圣诞树", "闘神トロフィー": "斗神奖杯", "菓子立て": "糖果架", "チョコレートのサプライズ": "巧克力惊喜",
    "走れオトメ": "奔跑吧，少女！", "瓶の中の船": "瓶中之船", "謎のマスク": "神秘面具",
    "キング・ジョージ5世": "乔治五世国王", "クリティカルⅠ": "暴击Ⅰ", "ダメージ軽減Ⅰ": "伤害减免Ⅰ", "諸刃の剣Ⅰ": "双刃剑Ⅰ",
    "クセ者Ⅰ": "棘手之敌Ⅰ", "スローダウンⅠ": "减速Ⅰ", "魚雷の故障Ⅰ": "鱼雷故障Ⅰ", "砲撃優先": "炮击优先",
}

MODEL_TERMS = {
    "イベント": "活动", "シナリオ": "剧情", "スペシャル": "特别", "限定": "限定", "獲得": "获得",
    "戦姫": "战姬", "艦隊": "舰队", "指揮官": "指挥官", "戦闘": "战斗", "参加": "参加",
    "防衛": "防卫", "駆逐艦": "驱逐舰", "軽巡洋艦": "轻巡洋舰", "重巡洋艦": "重巡洋舰",
    "巡洋艦": "巡洋舰", "戦艦": "战列舰", "空母": "航空母舰", "艦種": "舰种", "敵艦": "敌舰",
    "装備": "装备", "強化": "强化", "改造": "改造", "消費": "消耗", "回復": "恢复",
    "耐久値": "耐久值", "魚雷": "鱼雷", "雷撃": "雷击", "砲撃": "炮击", "ダメージ": "伤害",
    "効果": "效果", "全て": "全部", "すべて": "全部", "又は": "或", "以上": "以上", "未満": "未满",
    "条件": "条件", "編成": "编队", "毎": "每", "秒": "秒", "受ける": "受到", "使う": "使用",
    "クリア": "通关", "任務": "任务", "ミッション": "任务", "ショップ": "商店", "交換": "兑换",
    "聖誕": "圣诞", "クリスマス": "圣诞", "ハロウィン": "万圣节", "フレーム": "头像框",
    "来訪者": "访客", "イベント": "活动", "タイトル": "标题", "名前": "名称", "故障": "故障",
    "格納庫": "机库", "戦術": "战术", "バリア": "屏障", "ガチャ": "扭蛋", "食堂": "食堂",
    "道": "之道", "層": "层", "第二段階": "第二阶段", "解放予定": "预计解锁",
    "マスター": "大师", "宝箱": "宝箱", "宝物": "宝物", "コラボ": "联动", "ログイン": "登录",
    "訓練": "训练", "期間限定": "限时", "出現率アップ": "概率提升", "キャンペーン": "活动", "ガチャ": "扭蛋",
    "サインイン": "签到", "迎え": "迎接", "集めましょう": "一起收集吧", "ビッグイベント": "大型活动", "テスト": "测试",
    "シーズン": "赛季", "家具": "家具", "物語": "故事", "着飾り": "换装", "ロマン": "浪漫", "交換所": "兑换所",
    "専属": "专属", "周年記念": "周年纪念", "祭り": "祭典", "声優": "声优", "祝福": "祝福", "思い出": "回忆",
    "パネル": "面板", "休日": "假日", "探索": "探索", "調査": "调查", "ご褒美": "奖励", "計画": "计划",
    "正統編": "正统篇", "ミニ": "迷你", "夏海": "夏海", "輝き": "光辉", "祝": "庆祝", "星屑": "星尘", "世界": "世界",
    "革命": "革命", "武装巡り": "武装巡礼", "戦域": "战域", "友": "之友", "カウントダウン": "倒计时", "大運動会": "大运动会",
    "賞品": "奖品", "オーク": "橡树", "見守られながら": "在守护下", "キャンデー": "糖果", "幸運": "幸运", "ダイス": "骰子",
    "冬": "冬季", "奮闘伝": "奋斗传", "大願成就": "大愿得成", "春浅し": "春浅", "つぼみ": "花蕾", "芽吹き": "萌发",
    "桜": "樱花", "謎": "神秘", "開発機": "开发机", "蕾綻ぶ": "花蕾绽放", "群島": "群岛", "旅人": "旅人", "囁き": "低语",
    "七夕": "七夕", "ギフトバック": "礼包", "復刻版": "复刻版", "悪霊退散": "恶灵退散", "大冒険": "大冒险", "辺境": "边境",
    "報酬": "奖励", "占い屋": "占卜屋", "双星": "双星", "姉妹": "姐妹", "除夜の鐘": "除夕钟声", "毎日": "每日", "おみくじ": "抽签",
    "電子レンジ": "微波炉", "水風船": "水球", "プール": "泳池", "華奢": "精致", "生け花": "插花", "かぼちゃ": "南瓜",
    "愛猫": "爱猫", "聖夜": "圣夜", "トナカイ": "驯鹿", "トロフィー": "奖杯", "菓子立て": "糖果架", "チョコレート": "巧克力",
    "サプライズ": "惊喜", "走れ": "奔跑吧", "オトメ": "少女", "瓶の中": "瓶中", "マスク": "面具", "フレーム": "头像框",
    "月始め": "月初", "月末": "月末", "配布": "发放", "授与": "授予", "証し": "证明", "これからもよろしく": "今后也请多关照",
    "大成": "成就", "モチーフ": "主题", "デザイン": "设计", "獲得できる": "可以获得", "入手可能": "可以获得", "イベントにて": "在活动中",
    "にて": "中", "獲得する事ができる": "可以获得", "獲得できます": "可以获得", "交換して獲得": "兑换获得", "交換で獲得": "兑换获得",
    "クリアする事で": "通关后", "配布いたします": "将发放", "いたします": "", "しましょう": "吧", "楽しもう": "一起享受吧",
    "一緒に": "一起", "みんなと一緒に": "和大家一起", "指揮官": "指挥官", "装飾品": "装饰品", "企画": "企划",
    "雪花": "雪花", "計": "共", "個": "个", "飾り": "装饰", "揃えて": "集齐", "存分に": "尽情地",
    "なんでも": "无论什么", "温めた状態で取り出せる": "可以加热后取出", "噂になっている": "据说", "外はジメジメしてる": "外面又闷又湿",
    "遊びながら": "一边玩一边", "作っちゃう": "做出来", "一生懸命に": "努力地", "華道": "花道", "習って": "学习后",
    "やっと出来上がった": "终于完成的", "奇妙な夜": "奇妙的夜晚", "過ごした": "度过的", "変身して": "变身后",
    "暖かく": "温暖地", "満喫しよう": "尽情享受吧", "雪の玉": "雪球", "できます": "可以", "武闘会": "武斗会",
    "チャンピオン": "冠军", "十周年": "十周年", "十一回目": "第十一次", "斬新なデザイン": "新颖设计",
    "限定コイン": "限定硬币", "コイン": "硬币", "クリスマス前夜": "圣诞前夜", "クリスマス": "圣诞", "バレンタイン": "情人节",
    "全て": "全部", "又は": "或", "隻": "艘", "錬金術士": "炼金术士", "がある場合": "时", "場合": "时",
    "さらに": "此外", "更に": "此外", "味方": "我方", "敵からの": "来自敌方的", "受ける": "受到", "受けた後": "受到后",
    "発射する毎に": "每次发射", "発射した": "发射后", "免疫できる": "可免疫", "バリアを展開する": "展开屏障",
    "一度だけ発動できる": "每次只能发动一次", "攻撃を受けると": "受到攻击时", "次の": "下次", "航空攻撃": "航空攻击",
    "索敵段階": "索敌阶段", "大幅に増加する": "大幅增加", "主砲射程": "主炮射程", "全体": "全体", "率": "率",
    "速力": "航速", "T字不利の場合": "处于T字不利时", "減少しない": "不会降低", "中距離以内": "中距离以内",
    "発動可能": "可以发动", "毎に": "每次", "含まれていない場合": "不包含时", "次回": "下次", "全員": "全员",
    "大幅に増加": "大幅增加", "増加量": "增加量", "今回砲撃をした戦姫": "本次进行炮击的战姬", "主砲攻撃": "主炮攻击",
    "消耗されません": "不会消耗", "命中時": "命中时", "装甲": "装甲", "下げる": "降低", "減少値": "减少值",
    "雷装": "雷装", "魚雷搭載数": "鱼雷搭载数", "最大計算値": "最大计算值", "重複不可": "不可叠加", "上書きされる": "覆盖",
    "夜戦時": "夜战时", "毎": "每", "頃": "左右", "予備機": "备用机", "対象に": "以…为对象", "必中砲撃": "必中炮击",
    "使用しない": "不使用", "船速": "航速", "被ダメージ軽減": "所受伤害减免", "クリティカル率": "暴击率", "諸刃の剣": "双刃剑",
    "最も火力の高い": "火力最高的", "上限耐久値": "最大耐久值", "失う": "失去", "ただし": "但是", "回復する": "恢复",
    "旗艦": "旗舰", "中破以前の状態時": "处于中破以前状态时", "非旗艦": "非旗舰", "現在の残り耐久値": "当前剩余耐久值",
    "スローダウン": "减速", "ダメージ": "伤害", "軽減": "减免", "クリティカル": "暴击", "故障": "故障", "優先": "优先",
    "月始め": "月初", "月締め": "月末", "日の": "日的", "の": "的", "にて": "中", "に": "在", "で": "在",
    "が": "，", "を": "", "と": "与", "は": "是", "も": "也", "へ": "向", "から": "从", "まで": "为止",
    "する事で": "后", "する事が": "可以", "する": "", "できる": "可以", "できます": "可以", "なった": "成为了",
    "なる": "成为", "ました": "了", "ます": "", "です": "是", "だ。": "。", "だった": "曾经是",
    "ましょう": "吧", "ください": "请", "だけでなく": "不仅", "だけ": "仅", "しか": "只", "ない": "不",
    "される": "被", "されて": "被", "れた": "了", "れている": "着", "一足お先に": "提前",
    "リリース": "发布", "イラコン": "插画比赛", "コンテスト": "比赛", "ハッピー": "快乐", "おめでとう": "恭喜",
    "誰もが": "所有人都", "楽しい": "快乐的", "作るべき": "应该创造", "友達": "朋友", "短い時間": "短暂时光",
    "忘れられない": "难忘的", "忘れ難い": "难忘的", "かつては": "曾经", "不器用だった": "笨拙的",
    "一人前になった": "成长为独当一面", "引く橇": "拉着的雪橇", "たくさんの贈り物": "许多礼物", "積まれています": "装满了",
    "入りの料理": "制作的料理", "征服した勇者": "征服料理的勇者", "授ける": "授予", "輝かしき": "闪耀的", "頂きを競う": "争夺顶峰",
    "惜しくも": "虽遗憾但", "強者": "强者", "手本": "榜样", "守ってくれ": "守护", "送られる": "送给",
    "繰り返し": "反复", "競い合い": "相互竞争", "狙う": "瞄准", "犇めく": "云集", "挑め": "挑战",
    "まわりに合わせて": "配合周围", "ペースを落とす": "放慢脚步", "人のためだけでなく": "不只是为了别人",
    "自分のためだけではありません": "也不只是为了自己", "お先に": "提前", "基調": "基调", "散りばめられ": "点缀着",
    "堪らない": "令人无法抗拒", "一品": "佳品", "雰囲気たっぷり": "充满氛围", "独特な": "独特的",
    "長時間見つめていると": "长时间注视的话", "副作用": "副作用", "執務中": "工作期间", "症状": "症状",
    "疑われる際は": "被怀疑时", "言い訳": "借口", "誤魔化してください": "请用来搪塞",
    "映している映像": "播放的影像", "満開の桜": "盛开的樱花", "ひらひら舞い散る花びら": "纷纷飘落的花瓣",
    "季節問わず": "不分季节", "年中楽しめる": "全年都能欣赏", "花見ではない": "不是赏花", "苦情": "抱怨",
    "片づけをしなくて済む": "不用收拾", "発行した": "发行的", "ありがとうございます": "谢谢", "よろしくお願いします": "请多关照",
    "旅しよう": "一起旅行吧", "大好きなあの人": "最喜欢的那个人", "奇妙な": "奇妙的", "暖かく": "温暖地",
    "採用しました": "采用了", "さえあれば": "只要有这个", "一員": "一员", "授与した": "授予了",
}


def has_kana(value: str) -> bool:
    return bool(JP_KANA_RE.search(value.replace("・", "").replace("ー", "")))


def looks_japanese(value: str) -> bool:
    return has_kana(value) or any(term in value for term in JP_KANJI_HINTS)


def load_helpers(repo_root: Path):
    path = repo_root / "tools" / "localize-ship-names.py"
    spec = importlib.util.spec_from_file_location("blueoath_second_helpers", path)
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
    result: dict[str, Any] = {}
    if isinstance(rows, list):
        for row in rows:
            if isinstance(row, dict) and row.get("Id", row.get("id")) is not None:
                result[str(row.get("Id", row.get("id")))] = row.get("Data", row.get("value"))
    return result


def usable_reference(reference: Any, original: str) -> bool:
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


def model_translate(value: str, translator: Any) -> str:
    if value in MODEL_EXACT:
        return MODEL_EXACT[value]
    jp_hint = getattr(translator, "JP_HAN_HINT_RE", None)
    if not looks_japanese(value) and not (jp_hint and jp_hint.search(value) and any(term in value for term in JP_KANJI_HINTS)):
        return value.replace("・", "·").replace("ー", "—")
    result = value
    exact_replaced = False
    for source, target in sorted(MODEL_EXACT.items(), key=lambda item: -len(item[0])):
        if source in result:
            result = result.replace(source, target)
            exact_replaced = True
    if exact_replaced and not has_kana(result):
        return result.replace("・", "·").replace("ー", "—")
    value = result
    frame = re.fullmatch(r"(\d+)月(\d+)日の月始め記念として、限定フレームを配布いたします。?", value)
    if frame:
        return f"{frame.group(1)}月{frame.group(2)}日月初纪念，将发放限定头像框。"
    frame = re.fullmatch(r"(\d+)月(\d+)日の月末記念として、限定フレームを配布いたします。?", value)
    if frame:
        return f"{frame.group(1)}月{frame.group(2)}日月末纪念，将发放限定头像框。"
    frame = re.fullmatch(r"(\d+)年新年限定フレームです。今年もよろしくお願いします!", value)
    if frame:
        return f"{frame.group(1)}年新年限定头像框。今年也请多关照！"
    if value == "日本リリース100日記念フレーム":
        return "日本上线100天纪念头像框"
    result = value
    for source, target in sorted(MODEL_TERMS.items(), key=lambda item: -len(item[0])):
        result = result.replace(source, target)
    if not has_kana(result):
        return result.replace("・", "·").replace("ー", "—")
    direct = translator.direct_pattern_translation(result)
    if direct is None:
        direct = translator.heuristic_local_translation(result)
    if direct is not None:
        result = direct
    for source, target in sorted(MODEL_TERMS.items(), key=lambda item: -len(item[0])):
        result = result.replace(source, target)
    return result.replace("ー", "").replace("・", "·")


def translate_value(value: Any, translator: Any, reference: Any = None) -> tuple[Any, int, int, int, int]:
    if isinstance(value, str):
        if usable_reference(reference, value):
            normalized_reference = reference.replace("・", "·").replace("ー", "—")
            return normalized_reference, 1, int(normalized_reference != value), int(has_kana(normalized_reference)), 1
        result = model_translate(value, translator)
        return result, 1, int(result != value), int(has_kana(result)), 0
    if isinstance(value, list):
        refs = reference if isinstance(reference, list) else []
        out, counts = [], [0, 0, 0, 0]
        for i, item in enumerate(value):
            mapped, seen, changed, residual, ref_used = translate_value(item, translator, refs[i] if i < len(refs) else None)
            out.append(mapped)
            counts = [a + b for a, b in zip(counts, [seen, changed, residual, ref_used])]
        return out, *counts
    if isinstance(value, dict):
        out, counts = {}, [0, 0, 0, 0]
        for key, item in value.items():
            mapped, seen, changed, residual, ref_used = translate_value(item, translator, reference.get(key) if isinstance(reference, dict) else None)
            out[key] = mapped
            counts = [a + b for a, b in zip(counts, [seen, changed, residual, ref_used])]
        return out, *counts
    return value, 0, 0, 0, 0


def main() -> int:
    repo_root = Path(__file__).resolve().parents[1]
    client_root = Path(r"E:\BlueOath Rebirth")
    config_root = client_root / "blueoath" / "blueoath_Data" / "StreamingAssets" / "config"
    server_root = client_root / "客户端补丁" / "server" / "catalog" / "config"
    cn_root = client_root / "国服config"
    work_root = client_root / "第二批功能汉化"
    source_dir, decoded_dir = work_root / "source-db", work_root / "decoded-json"
    translated_dir, compiled_dir = work_root / "translated-json", work_root / "compiled-db"
    backup_dir = work_root / "backup" / "client-config"
    for directory in (source_dir, decoded_dir, translated_dir, compiled_dir, backup_dir):
        directory.mkdir(parents=True, exist_ok=True)

    helpers = load_helpers(repo_root)
    translator = helpers.load_local_translator(repo_root)
    summaries: dict[str, dict[str, int]] = {}
    mappings: list[dict[str, Any]] = []
    for file_name, fields in TARGET_FIELDS.items():
        client_db = config_root / file_name
        candidate = server_root / file_name
        preserved = source_dir / file_name
        source = candidate if usable_db(candidate) else preserved if usable_db(preserved) else client_db
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
        references = load_cn_reference(cn_root, file_name)
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
                    if isinstance(original, str):
                        stats["jp_before"] += int(has_kana(original))
                    ref_row = references.get(row_id)
                    ref = ref_row.get(field) if isinstance(ref_row, dict) else None
                    mapped, seen, changed, residual, ref_used = translate_value(original, translator, ref)
                    stats["text_fields"] += seen
                    stats["changed"] += changed
                    stats["cn_reference"] += ref_used
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


if __name__ == "__main__":
    raise SystemExit(main())
