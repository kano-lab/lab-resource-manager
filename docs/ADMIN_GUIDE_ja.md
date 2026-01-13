# 管理者ガイド

このガイドは、研究室に lab-resource-manager を導入する管理者向けのドキュメントです。

## セットアップ

### 1. 環境変数の設定

systemdデプロイの場合、`/etc/default/lab-resource-manager` を作成:

```env
# リポジトリ設定（デフォルト実装: Google Calendar）
GOOGLE_SERVICE_ACCOUNT_KEY=/etc/lab-resource-manager/service-account.json

# リソース設定
RESOURCE_CONFIG=/etc/lab-resource-manager/resources.toml

# データファイル
IDENTITY_LINKS_FILE=/var/lib/lab-resource-manager/identity_links.json
GOOGLE_CALENDAR_MAPPINGS_FILE=/var/lib/lab-resource-manager/google_calendar_mappings.json

# Slackボット設定
SLACK_BOT_TOKEN=xoxb-your-bot-token-here
SLACK_APP_TOKEN=xapp-your-app-token-here

# ログ設定
RUST_LOG=info
```

開発時はシェルの環境変数として設定できます。

**注意**: 通知設定は `config/resources.toml` でリソースごとに設定します。

### 2. リポジトリ実装の設定（デフォルト: Google Calendar）

Google Calendarリポジトリを使用する場合:

1. [Google Cloud Console](https://console.cloud.google.com/)でプロジェクトを作成
2. Google Calendar APIを有効化
3. サービスアカウントを作成してJSONキーをダウンロード
4. `secrets/service-account.json`として配置
5. カレンダーにサービスアカウントのメールアドレスを共有

### 3. リソース設定

`config/resources.toml`でGPUサーバーと部屋を定義:

```toml
[[servers]]
name = "Thalys"
calendar_id = "your-calendar-id@group.calendar.google.com"  # リポジトリ実装固有のID

# リソースごとに通知先を設定
[[servers.notifications]]
type = "slack"  # 通知実装の選択
bot_token = "xoxb-YOUR-BOT-TOKEN"
channel_id = "C01234567..."
# オプション: 通知のタイムゾーンを設定（IANA形式）
# 指定しない場合はシステムのローカルタイムゾーンで表示されます
# timezone = "Asia/Tokyo"

# オプション: テスト用にMock通知を追加
# [[servers.notifications]]
# type = "mock"
# timezone = "America/New_York"

[[servers.devices]]
id = 0
model = "A100 80GB PCIe"

[[servers.devices]]
id = 1
model = "A100 80GB PCIe"

[[rooms]]
name = "会議室A"
calendar_id = "room-calendar-id@group.calendar.google.com"

[[rooms.notifications]]
type = "slack"
bot_token = "xoxb-YOUR-BOT-TOKEN"
channel_id = "C01234567..."
# timezone = "Europe/London"
```

各リソースに複数の通知実装を設定でき、異なるリソースで異なる通知先を指定できます。

**タイムゾーン設定**: 各通知先にIANA形式のタイムゾーン名（例: `Asia/Tokyo`、
`America/New_York`、`Europe/London`）を指定できます。指定しない場合は、ボットが
動作しているシステムのローカルタイムゾーンで時刻が表示されます。タイムゾーンを
設定すると、時刻がそのタイムゾーンに変換され、タイムゾーン名と共に表示されるため、
ローカル時刻が分かりやすくなります。

## システムの起動

### Watcher（リソース監視）の起動

```bash
# デフォルト（リポジトリ実装 + 設定済み通知先）
cargo run --bin watcher

# Mockリポジトリ使用（テスト用）
cargo run --bin watcher --repository mock

# ポーリング間隔指定（デフォルト60秒）
cargo run --bin watcher --interval 30
```

### CLIオプション

- `--repository <google_calendar|mock>`: リポジトリ実装の選択
- `--interval <秒>`: ポーリング間隔

通知実装は `config/resources.toml` でリソースごとに設定します。

### Slackボットの起動

Slackボットを使うと、ユーザーがメールアドレスを登録して全てのリソースコレクションへのアクセスを取得できます:

```bash
# ボットの起動（開発時）
cargo run --bin lab-resource-manager

# systemdサービスとして起動（本番環境）
sudo systemctl start lab-resource-manager
```

### 管理者用コマンド

管理者は、他のユーザーのメールアドレスを代わりに登録できます:

```text
/link-user <@slack_user> <email@example.com>
```

**例:**

```text
/link-user @bob bob@example.com
```

このコマンドは、指定したSlackユーザーとメールアドレスを連携し、Google Calendarへのアクセス権を付与します。

## ビルド

### 開発ビルド

```bash
cargo build
```

### リリースビルド

```bash
cargo build --release
```

### 本番環境へのデプロイ

本番環境では、バイナリリリースとsystemdを使用します:

```bash
# ダウンロードしてインストール（詳細はREADMEを参照）
sudo bash deploy/install.sh
```

インストールされるもの:

- `/usr/local/bin/lab-resource-manager` - メインバイナリ
- `/etc/lab-resource-manager/` - 設定ディレクトリ
- `/var/lib/lab-resource-manager/` - データディレクトリ
- `/etc/systemd/system/lab-resource-manager.service` - systemdサービス

Dockerデプロイからアップグレードする場合は、[マイグレーションガイド](MIGRATION_ja.md)を参照してください。
