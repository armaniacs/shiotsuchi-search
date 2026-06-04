# PBI-52: ONNX Runtime バイナリ SHA 検証

**発端:** Supply Chain & Dependency Sentinel (スコア70)
**影響:** `ort` の `download-binaries` feature によりビルド時にONNX Runtimeバイナリが自動ダウンロードされるが、SHA-256等の整合性検証がない
**対処:** `build.rs` にダウンロードバイナリのチェックサム検証機構を追加。ort crate の標準的な検証方法を調査して実装
**工数:** 1日
