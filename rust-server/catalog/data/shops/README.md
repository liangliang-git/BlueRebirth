# Server shop pages

Each `shop-*.json` file defines one server-owned shop page. The filename is a
stable page tag; `shopId` is the protocol page ID and `label` is an editable
operator label.

```json
{
  "shopId": 1,
  "label": "shop-0001",
  "goods": [
    {
      "goodId": 1,
      "type": 1,
      "itemId": 10000,
      "num": 1,
      "costs": [
        { "type": 5, "itemId": 1, "amount": 100 }
      ]
    }
  ]
}
```

`goodId`, `type`, `itemId`, `num`, and `costs` are server purchase data.
`costs.type=5` means a currency cost; `costs.itemId` follows server resource
IDs (`1` gold, `2` diamond, `5` supply).

## 标签与文件映射

当前 `label` 使用稳定标签名 `shop-<shopId，4 位补零>`。客户端商城页的
`shopId` 对应下表文件；“客户端显示名”取自客户端 `config_shop.db` 的 `name` 字段，
仅用于文档对照，服务端运行时不读取该数据库。修改商品时直接编辑对应 JSON。

| 服务端标签 | 客户端显示名 | 文件 | shopId | 商品数 |
| --- | --- | --- | ---: | ---: |
| shop-0001 | 物资特辑 | [shop-0001.json](shop-0001.json) | 1 | 127 |
| shop-0003 | 钻石特辑 | [shop-0003.json](shop-0003.json) | 3 | 14 |
| shop-0005 | SR装备 | [shop-0005.json](shop-0005.json) | 5 | 179 |
| shop-0006 | 精锐战姬 | [shop-0006.json](shop-0006.json) | 6 | 4 |
| shop-0007 | 駆逐舰大作战 | [shop-0007.json](shop-0007.json) | 7 | 20 |
| shop-0008 | 非公开限定7日 | [shop-0008.json](shop-0008.json) | 8 | 7 |
| shop-0009 | 福袋 | [shop-0009.json](shop-0009.json) | 9 | 30 |
| shop-0015 | 巡洋舰訓練 | [shop-0015.json](shop-0015.json) | 15 | 20 |
| shop-0016 | 遠距離炮击 | [shop-0016.json](shop-0016.json) | 16 | 19 |
| shop-0017 | 航空队奇襲 | [shop-0017.json](shop-0017.json) | 17 | 19 |
| shop-0018 | SSR装备 | [shop-0018.json](shop-0018.json) | 18 | 194（运行时补齐全部 quality=4） |
| shop-0019 | 定期补给品1 | [shop-0019.json](shop-0019.json) | 19 | 3 |
| shop-0020 | 定期补给品2 | [shop-0020.json](shop-0020.json) | 20 | 21 |
| shop-0021 | 深海的記忆 | [shop-0021.json](shop-0021.json) | 21 | 37 |
| shop-0026 | 特别补给品 | [shop-0026.json](shop-0026.json) | 26 | 12 |
| shop-0027 | 舰队特辑 | [shop-0027.json](shop-0027.json) | 27 | 15 |
| shop-0029 | 红色改造商店 | [shop-0029.json](shop-0029.json) | 29 | 19 |
| shop-0102 | 十字誓約紋章 | [shop-0102.json](shop-0102.json) | 102 | 19 |
| shop-0104 | 紫色纹章 | [shop-0104.json](shop-0104.json) | 104 | 8 |
| shop-0106 | 太阳的化身 | [shop-0106.json](shop-0106.json) | 106 | 9 |
| shop-0107 | 月鹤的化身 | [shop-0107.json](shop-0107.json) | 107 | 17 |
| shop-0110 | 活动 | [shop-0110.json](shop-0110.json) | 110 | 26 |
| shop-0111 | 活动 | [shop-0111.json](shop-0111.json) | 111 | 66 |
| shop-0200 | 活动商店 | [shop-0200.json](shop-0200.json) | 200 | 19 |
| shop-0201 | 深海<size=18>商店</size> | [shop-0201.json](shop-0201.json) | 201 | 9 |
| shop-0202 | 深海<size=18>商店</size> | [shop-0202.json](shop-0202.json) | 202 | 13 |
| shop-0206 | 兑换<size=18>①</size> | [shop-0206.json](shop-0206.json) | 206 | 10 |
| shop-0207 | 兑换<size=18>②</size> | [shop-0207.json](shop-0207.json) | 207 | 11 |
| shop-0208 | 兑换<size=18>③</size> | [shop-0208.json](shop-0208.json) | 208 | 13 |
| shop-0302 | 指令商店 | [shop-0302.json](shop-0302.json) | 302 | 19 |
| shop-0305 | 指令商店 | [shop-0305.json](shop-0305.json) | 305 | 22 |
| shop-0306 | 战域<size=18>商店</size> | [shop-0306.json](shop-0306.json) | 306 | 22 |
| shop-0401 | 帰还<size=18>商店</size> | [shop-0401.json](shop-0401.json) | 401 | 23 |
| shop-0911 | 因子·Γ | [shop-0911.json](shop-0911.json) | 911 | 18 |
| shop-0912 | 因子·Ｈ | [shop-0912.json](shop-0912.json) | 912 | 18 |
| shop-0915 | 指令商店 | [shop-0915.json](shop-0915.json) | 915 | 28 |
| shop-0916 | 希埃弗的暴走 | [shop-0916.json](shop-0916.json) | 916 | 18 |
| shop-0917 | 深淵料理 | [shop-0917.json](shop-0917.json) | 917 | 13 |
| shop-0919 | 指令商店 | [shop-0919.json](shop-0919.json) | 919 | 28 |
| shop-0920 | 舰队特辑II | [shop-0920.json](shop-0920.json) | 920 | 20 |
| shop-0924 | 大嫌伊纳缶詰 | [shop-0924.json](shop-0924.json) | 924 | 12 |
| shop-0931 | 指令商店 | [shop-0931.json](shop-0931.json) | 931 | 28 |
| shop-0934 | 策士梅鲁 | [shop-0934.json](shop-0934.json) | 934 | 17 |
| shop-0935 | 相思相愛 | [shop-0935.json](shop-0935.json) | 935 | 12 |
| shop-0936 | 和菓子 | [shop-0936.json](shop-0936.json) | 936 | 12 |
| shop-0940 | UR装备 | [shop-0940.json](shop-0940.json) | 940 | 198（运行时补齐全部 quality=5） |
| shop-0951 | 指令商店 | [shop-0951.json](shop-0951.json) | 951 | 28 |
| shop-0955 | 黑猫纸杯蛋糕 | [shop-0955.json](shop-0955.json) | 955 | 13 |
| shop-0956 | 黑猫软糖 | [shop-0956.json](shop-0956.json) | 956 | 18 |
| shop-0957 | 活动 | [shop-0957.json](shop-0957.json) | 957 | 24 |
| shop-1011 | 指令商店 | [shop-1011.json](shop-1011.json) | 1011 | 28 |
| shop-1012 | 深海<size=18>商店</size> | [shop-1012.json](shop-1012.json) | 1012 | 11 |
| shop-1014 | 新移动者首次登场 | [shop-1014.json](shop-1014.json) | 1014 | 2 |
| shop-1015 | 雪球兑换 | [shop-1015.json](shop-1015.json) | 1015 | 4 |
| shop-1021 | 指令商店 | [shop-1021.json](shop-1021.json) | 1021 | 28 |
| shop-1022 | 因子·M | [shop-1022.json](shop-1022.json) | 1022 | 18 |
| shop-1023 | 埃内托的证 | [shop-1023.json](shop-1023.json) | 1023 | 17 |
| shop-1025 | 休假申请 | [shop-1025.json](shop-1025.json) | 1025 | 18 |
| shop-1026 | 寻找树叶 | [shop-1026.json](shop-1026.json) | 1026 | 15 |
| shop-1030 | 虹色兑换所 | [shop-1030.json](shop-1030.json) | 1030 | 3 |
| shop-1041 | 指令商店 | [shop-1041.json](shop-1041.json) | 1041 | 30 |
| shop-1043 | 樱花花瓣 | [shop-1043.json](shop-1043.json) | 1043 | 19 |
| shop-1044 | 神秘开发机材料 | [shop-1044.json](shop-1044.json) | 1044 | 24 |
| shop-1052 | 指令商店 | [shop-1052.json](shop-1052.json) | 1052 | 31 |
| shop-1072 | 黑猫纸杯蛋糕 | [shop-1072.json](shop-1072.json) | 1072 | 15 |
| shop-1073 | 黑猫软糖 | [shop-1073.json](shop-1073.json) | 1073 | 18 |
| shop-1074 | 异界支援少女 | [shop-1074.json](shop-1074.json) | 1074 | 3 |

商品类型常用值：`type=2` 装备，`type=3` 舰船，`type=5` 货币，`type=18`
时装。SR 装备通常为 `type=2` 且在 `config_equip.json` 中对应
`quality=3`。

SSR/UR 装备页由构建脚本自动补齐：`quality=4` 加入 shop 18，`quality=5`
加入 shop 940。自动商品统一售价 200，分别消耗 SSR 装备币 `9`、UR 装备币
`32`；客户端补丁同步注入对应商品配置。
