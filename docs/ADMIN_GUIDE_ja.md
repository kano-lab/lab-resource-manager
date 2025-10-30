# 管理者ガイド

このガイドは、研究室に lab-resource-manager を導入する管理者向けのドキュメントです。

## セットアップ

### 1. 環境変数の設定

```bash
cp .env.example .env
```

`.env`を編集して以下を設定:

```env
# リポジトリ設定（デフォルト実装: Google Calendar）
GOOGLE_SERVICE_ACCOUNT_KEY=secrets/service-account.json

# リソース設定
RESOURCE_CONFIG=config/resources.toml

# Slackボット設定（slackbotバイナリ用）
SLACK_BOT_TOKEN=xoxb-your-bot-token-here
SLACK_APP_TOKEN=xapp-your-app-token-here
```

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
webhook_url = "https://hooks.slack.com/services/YOUR/WEBHOOK/URL"

# オプション: テスト用にMock通知を追加
# [[servers.notifications]]
# type = "mock"

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
webhook_url = "https://hooks.slack.com/services/YOUR/ROOM/WEBHOOK"
```

各リソースに複数の通知実装を設定でき、異なるリソースで異なる通知先を指定できます。

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
# ボットの起動
cargo run --bin slackbot
```

## ビルド

### 開発ビルド

```bash
cargo build
```

### リリースビルド

```bash
cargo build --release
```

### バイナリの配置

リリースビルド後、バイナリは `target/release/` に生成されます:

- `target/release/watcher` - リソース監視プログラム
- `target/release/slackbot` - Slackボット

## トラブルシューティング

### Google Calendar API のエラー

- サービスアカウントのJSONキーが正しく配置されているか確認
- カレンダーがサービスアカウントに共有されているか確認
- Google Calendar APIが有効化されているか確認

### Slackボットが応答しない

- `SLACK_BOT_TOKEN` と `SLACK_APP_TOKEN` が正しく設定されているか確認
- Slack AppがSocket Modeで有効化されているか確認
- 必要な権限（`chat:write`, `commands`, `users:read`, `users:read.email`）が付与されているか確認

### 通知が送信されない

- `config/resources.toml` でWebhook URLが正しく設定されているか確認
- Slack Webhook URLが有効であるか確認
