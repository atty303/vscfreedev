# vscfreedev

## crates

- core: クライアントとリモートで共有するライブラリ
- client: クライアント側のライブラリ (SSHサポート含む)
- remote: リモート側のライブラリとチャンネルリスナー実行ファイル
- cli: クライアントのCLIアプリ
- gui: クライアントのGUIアプリ

## SSH機能

クライアントcrateはSSHを使用してリモートホストに接続し、coreのmessage channel接続を確立する機能を提供します。

### 使用方法

1. リモートホストにSSH接続する:

```bash
cargo run --bin vscfreedev -- ssh --host <ホスト名> --port <SSHポート> --username <ユーザー名> [--password <パスワード>] [--key-path <秘密鍵のパス>] [--message <送信メッセージ>]
```

例:
```bash
cargo run --bin vscfreedev -- ssh --host example.com --port 22 --username user --password pass
```

### 動作の仕組み

1. クライアントはリモート実行ファイルをビルドします
2. SSHを使用してリモートホストに接続します
3. リモート実行ファイルをリモートホストに転送します
4. リモートホストでリモート実行ファイルを実行します
5. リモート実行ファイルはTCPポートをリッスンし、message channelを確立します
6. クライアントはリモート実行ファイルに接続し、message channelを確立します
