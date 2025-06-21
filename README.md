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

- **デーモンモード**: バックグラウンドで動作する永続的なサービス
  - [x] セッション管理と接続プーリング
  - [x] CLI/GUI両方からの利用
  - [x] Unix socket (Linux/macOS) / Named pipe (Windows) 通信
  - [x] 複数クライアントでの接続共有

- **トランスポート**: リモートとの通信経路
  - [x] SSH: SSHでリモートサーバーを起動し標準入出力で通信する
  - [x] ローカル: ローカルでリモートサーバーを起動し標準入出力で通信する
  - [ ] WSL: WSLインスタンス内にリモートサーバーを起動し標準入出力で通信する
  - [ ] TCP: 起動済みのリモートサーバーとTCPで通信する

- **リモートサーバーの環境サポート**
  - [x] linux-x64
  - [ ] linux-aarch64
  - [ ] mac-aarch64
  - [ ] windows-x64

- **接続機能**
  - [x] 複数のリモートと接続できる（デーモン経由）
  - [x] 設定プロファイル対応
  - [x] 接続の再利用とプーリング

- **リモート操作**
  - [x] クリップボード同期: ローカル・リモート間でクリップボードを共有
  - [x] リモートからローカルブラウザを起動
  - [ ] リモートポートフォワーディング: リモートポートをローカルに転送
    - [ ] SSL終端: ローカルではHTTPSでLISTENしてSSLを終端し、リモートをNon-SSLポートに接続する
    - [ ] ローカルHTTPSの自己署名証明書を自動で管理

## 使用方法

### デーモンモード（推奨）

```bash
# デーモンを起動
cargo run -p yuha-cli -- daemon start

# SSH接続（デーモン経由）
cargo run -p yuha-cli -- ssh -H example.com -u user --key-path ~/.ssh/id_rsa

# ローカル接続（デーモン経由）
cargo run -p yuha-cli -- local

# デーモンの状態確認
cargo run -p yuha-cli -- daemon status

# アクティブなセッション一覧
cargo run -p yuha-cli -- daemon sessions
```

### 直接接続（デーモンなし）

```bash
# SSH接続（直接）
cargo run -p yuha-cli -- ssh -H example.com -u user --key-path ~/.ssh/id_rsa --no-daemon

# ローカル接続（直接）
cargo run -p yuha-cli -- local --no-daemon
```

### 設定ファイル

```bash
# デフォルト設定ファイルを生成
cargo run -p yuha-cli -- config init

# 設定を表示
cargo run -p yuha-cli -- config show

# 接続プロファイルを追加
cargo run -p yuha-cli -- config add-profile my-server --host example.com --username user

# プロファイルを使用して接続
cargo run -p yuha-cli -- ssh --profile my-server
```

### 動作の仕組み

#### デーモンモード
1. デーモンがバックグラウンドで起動し、Unix socket/Named pipeで待機
2. CLIクライアントがデーモンに接続要求を送信
3. デーモンがリモートサーバーへの接続を確立・管理
4. 複数クライアントが同一セッションを共有可能

#### 直接接続モード
1. CLIクライアントが直接リモートホストに接続
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

# デーモン起動
cargo run -p yuha-cli -- daemon start

# リモートサーバー実行（通常は自動起動されるため手動実行は不要）
cargo run -p yuha-remote -- --stdio
```

## テスト

### 高速テスト（単体テストのみ）
```bash
# 全体
cargo test

# クライアントのみ
cargo test -p yuha-client
```

### 統合テスト（Dockerテストを含む）
```bash
# 全テスト実行（Docker必要）
cargo test --features docker-tests

# クライアントの統合テスト
cargo test -p yuha-client --features docker-tests
```

### テスト種別
- **単体テスト** (`tests/unit/`): 外部依存なしの高速テスト
- **Dockerテスト** (`tests/docker/`): Docker コンテナを使用する統合テスト

## デバッグ

### ログレベル設定
```bash
# CLI のデバッグ
RUST_LOG=debug cargo run -p yuha-cli -- ssh -H example.com -u user

# リモートサーバーのデバッグ
RUST_LOG=debug cargo run -p yuha-remote -- --stdio

# デーモンのデバッグ
RUST_LOG=debug cargo run -p yuha-cli -- daemon start --foreground
```

### リモートサーバーログ確認
```bash
tail -f /tmp/remote_startup.txt
tail -f /tmp/remote_stderr.log
```
