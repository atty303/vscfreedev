# Yuha - Development Summary

## プロジェクト概要

YuhaはRustで書かれたリモート開発ツールで、SSH経由でリモートサーバーと通信し、以下の機能を提供します：

- ポートフォワーディング
- クリップボード同期
- ブラウザ操作
- GUI/CLIインターフェース

## アーキテクチャ

### クレート構成

```
yuha/
├── crates/
│   ├── cli/          # CLIインターフェース
│   ├── client/       # クライアント側実装
│   ├── core/         # 共通機能・プロトコル
│   ├── gui/          # GUIインターフェース  
│   └── remote/       # リモートサーバー実装
```

### 通信プロトコル

#### 新しい簡素化プロトコル（現在の実装）

**設計思想**: 複雑な双方向メッセージングを廃止し、シンプルなリクエスト・レスポンス + Long Polling方式を採用

**プロトコル構造**:
```rust
// リクエスト種別
enum YuhaRequest {
    PollData,                    // データポーリング（擬似双方向通信）
    StartPortForward { .. },     // ポートフォワード開始
    StopPortForward { .. },      // ポートフォワード停止
    PortForwardData { .. },      // ポートフォワードデータ転送
    GetClipboard,                // クリップボード取得
    SetClipboard { .. },         // クリップボード設定
    OpenBrowser { .. },          // ブラウザ起動
}

// レスポンス種別
enum YuhaResponse {
    Success,                     // 成功
    Error { message },           // エラー
    Data { items },              // データ（複数の ResponseItem）
}
```

**通信フロー**:
1. Client → Server: `YuhaRequest`
2. Server → Client: `YuhaResponse`
3. 双方向データは`PollData`リクエストによるLong Pollingで実現

**利点**:
- ✅ シンプルで理解しやすい
- ✅ デバッグが容易
- ✅ エラーハンドリングが予測可能
- ✅ 状態管理が簡単
- ✅ テストしやすい

### 主要コンポーネント

#### クライアント側
- `SimpleYuhaClient<T>`: メインクライアント実装
- `SshChannelAdapter`: SSH接続のAsync Read/Write適応
- `simple_client::connect_ssh()`: SSH接続とクライアント初期化

#### サーバー側  
- `RemoteServer<T>`: メインサーバー実装
- `StdioStream`: stdin/stdoutの双方向ストリーム
- `ResponseBuffer`: サーバー側データバッファリング

#### コア機能
- `MessageChannel<T>`: バイナリメッセージフレーミング + JSONシリアライゼーション
- `protocol.rs`: リクエスト・レスポンス構造体定義
- `clipboard.rs`: クリップボード操作
- `browser.rs`: ブラウザ操作

## 開発履歴

### 主要な変更点

#### 2024年 - プロトコル大幅簡素化
**コミット**: `a7e8aea` - "refactor: simplify communication protocol to request-response pattern"

- 複雑な`PortForwardMessage`ベースの双方向プロトコルを廃止
- 新しい`YuhaRequest`/`YuhaResponse`パターンに統一
- Long Pollingによる擬似双方向通信を実装
- `YuhaClient`（旧実装）を削除し、`SimpleYuhaClient`に一本化
- 1,200行以上のコード削減を実現
- 全テストを新プロトコル対応に更新

### 技術的改善

#### パフォーマンス最適化
- SSH stdio通信のタイムアウト処理改善
- テストの高速化（固定sleep削除、スマートポーリング導入）
- サーバー応答性の向上

#### コード品質向上
- 不要なコード警告の抑制
- エラーハンドリングの改善
- テストの安定性向上

## 開発ガイド

### ビルド・実行

```bash
# 全体ビルド
cargo build

# テスト実行
cargo test

# CLI実行
cargo run -p yuha-cli

# リモートサーバー実行
cargo run -p yuha-remote -- --stdio
```

### テスト戦略

- **単体テスト**: 各クレートで基本機能をテスト
- **統合テスト**: クライアント・サーバー間の通信をテスト
- **プロトコルテスト**: メッセージシリアライゼーション・デシリアライゼーションをテスト

### デバッグ

```bash
# ログレベル設定
RUST_LOG=debug cargo run -p yuha-remote -- --stdio

# リモートサーバーログ確認
tail -f /tmp/remote_startup.txt
tail -f /tmp/remote_stderr.log
```

## 今後の改善点

### 短期的改善
- [ ] エラーメッセージの多言語化
- [ ] 設定ファイル対応
- [ ] パフォーマンスメトリクス収集

### 中長期的改善
- [ ] プラグインシステムの実装
- [ ] 複数同時接続対応
- [ ] Web UIの追加
- [ ] モバイルクライアント対応

## 参考情報

### 依存関係
- `tokio`: 非同期ランタイム
- `russh`: SSH実装
- `serde`: シリアライゼーション
- `bytes`: バイト操作
- `tracing`: ログ出力
- `clap`: CLI引数解析

### 設計原則
1. **シンプル性**: 複雑さより理解しやすさを優先
2. **保守性**: 変更・拡張が容易な構造
3. **信頼性**: エラーハンドリングとテストの充実
4. **パフォーマンス**: 必要十分な性能の確保

---

*最終更新: 2024年*
*Claude Code による自動生成ドキュメント*