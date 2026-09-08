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
`shopId` 对应下表文件；修改商品时直接编辑对应 JSON。

| 标签 | 文件 | shopId | 商品数 |
| --- | --- | ---: | ---: |
| shop-0001 | [shop-0001.json](shop-0001.json) | 1 | 127 |
| shop-0003 | [shop-0003.json](shop-0003.json) | 3 | 14 |
| shop-0005 | [shop-0005.json](shop-0005.json) | 5 | 179 |
| shop-0006 | [shop-0006.json](shop-0006.json) | 6 | 4 |
| shop-0007 | [shop-0007.json](shop-0007.json) | 7 | 20 |
| shop-0008 | [shop-0008.json](shop-0008.json) | 8 | 7 |
| shop-0009 | [shop-0009.json](shop-0009.json) | 9 | 28 |
| shop-0015 | [shop-0015.json](shop-0015.json) | 15 | 20 |
| shop-0016 | [shop-0016.json](shop-0016.json) | 16 | 19 |
| shop-0017 | [shop-0017.json](shop-0017.json) | 17 | 19 |
| shop-0018 | [shop-0018.json](shop-0018.json) | 18 | 4 |
| shop-0019 | [shop-0019.json](shop-0019.json) | 19 | 3 |
| shop-0020 | [shop-0020.json](shop-0020.json) | 20 | 21 |
| shop-0021 | [shop-0021.json](shop-0021.json) | 21 | 37 |
| shop-0026 | [shop-0026.json](shop-0026.json) | 26 | 12 |
| shop-0027 | [shop-0027.json](shop-0027.json) | 27 | 15 |
| shop-0102 | [shop-0102.json](shop-0102.json) | 102 | 19 |
| shop-0104 | [shop-0104.json](shop-0104.json) | 104 | 8 |
| shop-0106 | [shop-0106.json](shop-0106.json) | 106 | 9 |
| shop-0107 | [shop-0107.json](shop-0107.json) | 107 | 17 |
| shop-0110 | [shop-0110.json](shop-0110.json) | 110 | 26 |
| shop-0111 | [shop-0111.json](shop-0111.json) | 111 | 66 |
| shop-0200 | [shop-0200.json](shop-0200.json) | 200 | 19 |
| shop-0201 | [shop-0201.json](shop-0201.json) | 201 | 9 |
| shop-0202 | [shop-0202.json](shop-0202.json) | 202 | 13 |
| shop-0206 | [shop-0206.json](shop-0206.json) | 206 | 10 |
| shop-0207 | [shop-0207.json](shop-0207.json) | 207 | 11 |
| shop-0208 | [shop-0208.json](shop-0208.json) | 208 | 13 |
| shop-0302 | [shop-0302.json](shop-0302.json) | 302 | 19 |
| shop-0305 | [shop-0305.json](shop-0305.json) | 305 | 22 |
| shop-0306 | [shop-0306.json](shop-0306.json) | 306 | 22 |
| shop-0401 | [shop-0401.json](shop-0401.json) | 401 | 23 |
| shop-0911 | [shop-0911.json](shop-0911.json) | 911 | 18 |
| shop-0912 | [shop-0912.json](shop-0912.json) | 912 | 18 |
| shop-0915 | [shop-0915.json](shop-0915.json) | 915 | 28 |
| shop-0916 | [shop-0916.json](shop-0916.json) | 916 | 18 |
| shop-0917 | [shop-0917.json](shop-0917.json) | 917 | 13 |
| shop-0919 | [shop-0919.json](shop-0919.json) | 919 | 28 |
| shop-0920 | [shop-0920.json](shop-0920.json) | 920 | 20 |
| shop-0924 | [shop-0924.json](shop-0924.json) | 924 | 12 |
| shop-0931 | [shop-0931.json](shop-0931.json) | 931 | 28 |
| shop-0934 | [shop-0934.json](shop-0934.json) | 934 | 17 |
| shop-0935 | [shop-0935.json](shop-0935.json) | 935 | 12 |
| shop-0936 | [shop-0936.json](shop-0936.json) | 936 | 12 |
| shop-0940 | [shop-0940.json](shop-0940.json) | 940 | 8 |
| shop-0951 | [shop-0951.json](shop-0951.json) | 951 | 28 |
| shop-0955 | [shop-0955.json](shop-0955.json) | 955 | 13 |
| shop-0956 | [shop-0956.json](shop-0956.json) | 956 | 18 |
| shop-0957 | [shop-0957.json](shop-0957.json) | 957 | 24 |
| shop-1011 | [shop-1011.json](shop-1011.json) | 1011 | 28 |
| shop-1012 | [shop-1012.json](shop-1012.json) | 1012 | 11 |
| shop-1014 | [shop-1014.json](shop-1014.json) | 1014 | 2 |
| shop-1015 | [shop-1015.json](shop-1015.json) | 1015 | 4 |
| shop-1021 | [shop-1021.json](shop-1021.json) | 1021 | 28 |
| shop-1022 | [shop-1022.json](shop-1022.json) | 1022 | 18 |
| shop-1023 | [shop-1023.json](shop-1023.json) | 1023 | 17 |
| shop-1025 | [shop-1025.json](shop-1025.json) | 1025 | 18 |
| shop-1026 | [shop-1026.json](shop-1026.json) | 1026 | 15 |
| shop-1030 | [shop-1030.json](shop-1030.json) | 1030 | 3 |
| shop-1041 | [shop-1041.json](shop-1041.json) | 1041 | 30 |
| shop-1043 | [shop-1043.json](shop-1043.json) | 1043 | 19 |
| shop-1044 | [shop-1044.json](shop-1044.json) | 1044 | 24 |
| shop-1052 | [shop-1052.json](shop-1052.json) | 1052 | 31 |
| shop-1072 | [shop-1072.json](shop-1072.json) | 1072 | 15 |
| shop-1073 | [shop-1073.json](shop-1073.json) | 1073 | 18 |
| shop-1074 | [shop-1074.json](shop-1074.json) | 1074 | 3 |

商品类型常用值：`type=2` 装备，`type=3` 舰船，`type=5` 货币，`type=18`
时装。SR 装备通常为 `type=2` 且在 `config_equip.json` 中对应
`quality=3`。
