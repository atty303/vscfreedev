# yuha
![yuha-removebg-preview](https://github.com/user-attachments/assets/eac30298-b8f3-4d6a-bf8c-a5a46e2ce5dd)

## crates

- core: クライアントとリモートで共有するライブラリ
- client: クライアント側のライブラリ (SSHサポート含む)
- remote: リモート側のライブラリとチャンネルリスナー実行ファイル
- cli: クライアントのCLIアプリ
- gui: クライアントのGUIアプリ
- e2e: エンドツーエンドテスト (Dockerを使用)

## SSH機能

クライアントcrateはSSHを使用してリモートホストに接続し、coreのmessage channel接続を確立する機能を提供します。

### 使用方法

1. リモートホストにSSH接続する:

```bash
cargo run --bin yuha -- ssh --host <ホスト名> --port <SSHポート> --username <ユーザー名> [--password <パスワード>] [--key-path <秘密鍵のパス>] [--message <送信メッセージ>]
```

例:
```bash
cargo run --bin yuha -- ssh --host example.com --port 22 --username user --password pass
```

### 動作の仕組み

1. クライアントはリモート実行ファイルをビルドします
2. SSHを使用してリモートホストに接続します
3. リモート実行ファイルをリモートホストに転送します
4. リモートホストでリモート実行ファイルを実行します
5. リモート実行ファイルはTCPポートをリッスンし、message channelを確立します
6. クライアントはリモート実行ファイルに接続し、message channelを確立します

## E2Eテスト

このプロジェクトにはDockerを使用したエンドツーエンドテストが含まれています。

### 前提条件

- Dockerがインストールされ、実行されていること
- RustとCargoがインストールされていること

### テストの実行

E2Eテストを実行するには、プロジェクトのルートから次のコマンドを使用します：

```bash
cargo test -p yuha_e2e
```

### テストの仕組み

E2Eテストは以下のように動作します：

1. リモートサーバーバイナリをビルドします
2. 以下を含むDockerコンテナを作成します：
   - SSHサーバー
   - ポート9999で実行されるリモートサーバー
3. Dockerコンテナを指すSSH接続パラメータでCLIを実行します
4. CLIが正常に接続してメッセージを交換できることを確認します
