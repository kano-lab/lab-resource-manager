# lab-resource-manager

研究室のGPU・部屋などの資源をGoogle Calendar経由で管理し、Slackに変更通知を送信するシステム。

## Features

- **Google Calendar統合**: GPUサーバーと部屋の予約をカレンダーで管理
- **マルチ通知先対応**: リソースごとに異なる通知先を設定可能（Slack、Mockなど）
- **柔軟なデバイス指定**: `0-2,5,7-9` 形式での複数デバイス指定に対応
- **クリーンアーキテクチャ**: DDD + ヘキサゴナルアーキテクチャで設計
- **Mock実装**: テスト用のモックリポジトリと通知機能を内蔵

## Architecture

```
src/
├── domain/           # ドメイン層（ビジネスロジック）
│   ├── aggregates/   # 集約（ResourceUsage）
│   └── ports/        # ポート（Repository, Notifier）
├── application/      # アプリケーション層（ユースケース）
├── infrastructure/   # インフラ層（外部システム接続）
│   ├── repositories/ # Google Calendar実装
│   ├── notifier/     # Slack実装
│   └── config/       # 設定管理
└── bin/              # エントリポイント
```

## Setup

### 1. 環境変数の設定

```bash
cp .env.example .env
```

`.env`を編集して以下を設定:

```env
GOOGLE_SERVICE_ACCOUNT_KEY=secrets/service-account.json
CONFIG_PATH=config/resources.toml
```

**注意**: 通知設定（Slack Webhook URLなど）は `config/resources.toml` でリソースごとに設定します。

### 2. Google Calendar API設定

1. [Google Cloud Console](https://console.cloud.google.com/)でプロジェクトを作成
2. Google Calendar APIを有効化
3. サービスアカウントを作成してJSONキーをダウンロード
4. `secrets/service-account.json`として配置
5. カレンダーにサービスアカウントを共有

### 3. リソース設定

`config/resources.toml`でGPUサーバーと部屋を定義:

```toml
[[servers]]
name = "Thalys"
calendar_id = "your-calendar-id@group.calendar.google.com"

# リソースごとに通知先を設定
[[servers.notifications]]
type = "slack"
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

各リソースに複数の通知先を設定でき、異なるリソースで異なるチャンネルに通知できます。

## Usage

### Watcher起動

```bash
# デフォルト（Google Calendar + 設定済み通知先）
cargo run --bin watcher

# Mock repository使用（テスト用）
cargo run --bin watcher --repository mock

# ポーリング間隔指定（デフォルト60秒）
cargo run --bin watcher --interval 30
```

### オプション

- `--repository <google_calendar|mock>`: データソースの選択
- `--interval <秒>`: ポーリング間隔

通知先は `config/resources.toml` でリソースごとに設定します。

## Development

### テスト実行

```bash
# 全テスト
cargo test

# 特定のモジュール
cargo test resource_factory
```

### ビルド

```bash
# 開発ビルド
cargo build

# リリースビルド
cargo build --release
```

### コード検査

```bash
cargo check
cargo clippy
```

## デバイス指定記法

カレンダーのイベントタイトルで、以下の形式でデバイスを指定できます:

- 単一: `0` → デバイス0
- 範囲: `0-2` → デバイス0, 1, 2
- 複数: `0,2,5` → デバイス0, 2, 5
- 混在: `0-1,6-7` → デバイス0, 1, 6, 7

## Project Status

### 実装済み ✅

- [x] リソースベースの通知ルーティング

### Roadmap

- [ ] Slackコマンドでのカレンダー予約作成
- [ ] Slackユーザーとの紐づけ
- [ ] 自然言語でのリソース管理（LLMエージェント）

## ライセンス

このプロジェクトは以下のいずれかのライセンスで利用できます:

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) または http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) または http://opensource.org/licenses/MIT)

どちらか好きな方を選択してください。

### コントリビューション

特に明示しない限り、あなたが投稿する貢献は Apache-2.0 ライセンスで定義される通り、
上記のデュアルライセンスの下で提供されるものとし、追加の条件は付与されません。