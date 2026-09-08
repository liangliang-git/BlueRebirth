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
