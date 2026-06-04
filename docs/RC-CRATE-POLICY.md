# RC Crate Policy

## 現状

| Crate | Version | Type | Reason |
|-------|---------|------|--------|
| notify | 9.0.0-rc.4 | RC | v9 API が必要だが stable 未リリース |
| ort | 2.0.0-rc.12 | RC | ONNX Runtime 2.0 API が必要だが stable 未リリース |

## Upgrade 条件

- notify: stable 9.x リリース → 即座にアップグレード
- ort: stable 2.x リリース → 1週間の様子見後にアップグレード
- Security advisory: 該当 crate に影響する脆弱性 → 即座に対処

## 確認方法

```bash
cargo search notify
cargo search ort
```

## 責任者

@iron
