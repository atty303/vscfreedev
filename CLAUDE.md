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
│   ├── client/       # クライアント側実装（デーモン機能を含む）
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
1. Client → Server: `ProtocolRequest`
2. Server → Client: `ProtocolResponse`
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

## 開発ガイド

### ビルド・実行

```bash
# 全体ビルド
cargo build

# 高速テスト実行（単体テストのみ）
cargo test

# 全テスト実行（Dockerテストを含む）
cargo test --features docker-tests

# CLI実行
cargo run -p yuha-cli

# デーモン起動
cargo run -p yuha-cli -- daemon start

# リモートサーバー実行
cargo run -p yuha-remote -- --stdio
```

### テスト戦略

テストは機能別に整理され、実行速度と依存関係によって分類されています：

- **単体テスト** (`tests/unit/`): 外部依存なしの高速テスト
  - `local_communication.rs` - ローカルプロセス通信
  - `protocol_serialization.rs` - プロトコルメッセージのシリアライゼーション
  - `local_transport.rs` - ローカル転送実装
  - `comprehensive_local.rs` - 包括的なローカル機能テスト

- **Dockerテスト** (`tests/docker/`): Docker コンテナを使用する統合テスト
  - `ssh_communication.rs` - SSH通信とバイナリ自動アップロード
  - `port_forwarding.rs` - SSH経由のポートフォワーディング
  - `binary_transfer.rs` - バイナリアップロードと実行テスト

**テスト実行方法**:

```bash
# 高速テストのみ（Dockerなし）
cargo test -p yuha-client

# 全テスト（Docker必要）
cargo test -p yuha-client --features docker-tests

# 特定のテストカテゴリ
cargo test -p yuha-client unit::                    # 単体テストのみ
cargo test -p yuha-client --features docker-tests docker::  # Dockerテストのみ
```

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
- [ ] 設定ファイル対応
- [ ] パフォーマンスメトリクス収集

### 中長期的改善
- [ ] 複数同時接続対応

## 参考情報

### 設計原則
1. **シンプル性**: 複雑さより理解しやすさを優先
2. **保守性**: 変更・拡張が容易な構造
3. **信頼性**: エラーハンドリングとテストの充実
4. **パフォーマンス**: 必要十分な性能の確保

### ルール

- テストの修正を指示されたとき、テストを成功させるために勝手に機能を変更したり削除するのは禁止だ。 
  現状の機能は必要があって実装されているので、テストの都合で変更することは禁止だ。
  機能を維持したまま実装を変更するのはいいが、大幅な設計変更が必要な場合はユーザーに確認すること。
- 機能を追加するときは、必ず対応するテストを追加すること。
- タスクが完了したときは、すべてのテストを実行して正常に完了することを必ず確認してください。
- DRY原則を非常に重視して、コードを書いてください。
- 識別子を命名するときは、コードベース全体を通して、意味の通る簡潔な名前を付けてください。 修正内容を表現する名前は禁止です。
- 修正を加えるとき、古い不要になったコードを維持する必要はありません。常に、修正後のコード全体がシンプルになるように、一貫性を持った差分を適用してください。 
- プロトコルのバージョニングは不要です。
- 改修するときに互換性のために余分なコードを残すのは禁止です。新しいアーキテクチャにあわせて既存のコードも修正してください。
  改修により不要となるコードがないか考えて、必要であれば削除してください。
