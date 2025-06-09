# yuha
![yuha-removebg-preview](https://github.com/user-attachments/assets/eac30298-b8f3-4d6a-bf8c-a5a46e2ce5dd)

Rustで書かれたリモート開発ツール。SSH経由でリモートサーバーと通信し、ポートフォワーディング、クリップボード同期、ブラウザ操作などの機能を提供します。

## クレート構成

```
yuha/
├── crates/
│   ├── cli/          # CLIインターフェース
│   ├── client/       # クライアント側実装
│   ├── core/         # 共通機能・プロトコル
│   ├── gui/          # GUIインターフェース  
│   └── remote/       # リモートサーバー実装
```

## 機能

- **ポートフォワーディング**: リモートポートをローカルに転送
- **クリップボード同期**: ローカル・リモート間でクリップボードを共有
- **ブラウザ操作**: リモートからローカルブラウザを起動
- **自動バイナリ転送**: リモートバイナリを自動的にアップロード・実行

## 通信プロトコル

シンプルなリクエスト・レスポンス + Long Polling方式を採用：

- `YuhaRequest`: クライアントからのリクエスト
- `YuhaResponse`: サーバーからのレスポンス
- 双方向通信は`PollData`リクエストによるLong Pollingで実現

## 使用方法

### CLIでSSH接続

```bash
cargo run -p yuha-cli -- ssh --host <ホスト名> --port <SSHポート> --username <ユーザー名> [--password <パスワード>] [--key-path <秘密鍵のパス>]
```

例:
```bash
cargo run -p yuha-cli -- ssh --host example.com --port 22 --username user --key-path ~/.ssh/id_rsa
```

### 動作の仕組み

1. クライアントがSSHでリモートホストに接続
2. リモートバイナリを自動的にビルド・転送
3. リモートでバイナリを実行（stdin/stdout経由で通信）
4. メッセージチャネルを確立して各種機能を提供

## ビルド・実行

```bash
# 全体ビルド
cargo build

# テスト実行
cargo test

# CLI実行
cargo run -p yuha-cli

# リモートサーバー実行（通常は自動起動されるため手動実行は不要）
cargo run -p yuha-remote -- --stdio
```

## テスト

### 単体テスト
```bash
cargo test
```

### 統合テスト
クライアントcrateに各種統合テストが含まれています：
- 基本通信テスト
- バイナリ転送テスト
- ポートフォワードテスト
- プロトコルテスト

```bash
cargo test -p yuha-client
```

## デバッグ

ログレベルを設定してデバッグ情報を表示：

```bash
RUST_LOG=debug cargo run -p yuha-cli -- ssh --host example.com --username user
```

リモート側のログ確認：
```bash
tail -f /tmp/remote_startup.txt
tail -f /tmp/remote_stderr.log
```

## 依存関係

- `tokio`: 非同期ランタイム
- `russh`: SSH実装
- `serde`: シリアライゼーション
- `bytes`: バイト操作
- `tracing`: ログ出力
- `clap`: CLI引数解析
