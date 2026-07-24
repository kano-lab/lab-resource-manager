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

### 4. 通知メッセージのカスタマイズ（オプション）

通知メッセージのテンプレートとフォーマットをカスタマイズできます:

```toml
[[servers.notifications]]
type = "slack"
bot_token = "xoxb-YOUR-BOT-TOKEN"
channel_id = "C01234567..."
timezone = "Asia/Tokyo"

# メッセージテンプレート（オプション）
[servers.notifications.templates]
created = "{user}が{resource}を{time}使います"
updated = "{user}が予約を変更: {resource} {time}"
deleted = "{user}が予約をキャンセル: {resource}"
conflict = "{user}が既に{resource}を{time}で予約済みです"

# フォーマット設定（オプション）
[servers.notifications.format]
resource_style = "compact"   # リソース表示スタイル
time_style = "smart"         # 時刻表示スタイル
date_format = "md"           # 日付フォーマット
```

`conflict`は予約リクエストが競合により拒否された際に使われます。
`created`/`updated`/`deleted`とは異なり、プレースホルダーは拒否された
リクエストではなく、競合の原因となった**既存の**予約を指します。
`{user}`は既存予約の所有者、`{time}`はその期間、`{resource}`は競合した
リソースそのものです。予約を試みたユーザー本人にのみ表示され、
競合したリソースに設定された通知設定（テンプレート・フォーマット）が
使われます（リソース全体への通知配信ではありません）。

1回の予約リクエストで複数リソース（例: サーバー全体）を指定し、
そのうち複数件が（場合によっては異なる既存予約と）競合した場合は、
競合したリソースごとにテンプレートを1回ずつレンダリングして
まとめて表示するため、一部の競合が黙って握りつぶされることはありません。

**プレースホルダー:**

| プレースホルダー | 説明 |
|------------------|------|
| `{user}` | ユーザー名/Slackメンション |
| `{resource}` | リソース情報 |
| `{time}` | 期間 |
| `{notes}` | 備考セクション（`\n\n📝 備考\n...`形式で展開、なければ空文字） |
| `{resource_label}` | リソースラベル（例: 💻 予約GPU） |

**resource_style オプション:**

| 値 | 出力例 |
|----|--------|
| `full`（デフォルト） | Thalys / A100 80GB PCIe / GPU:0 |
| `compact` | Thalys 0,1,2 |
| `server_only` | Thalys |

**time_style オプション:**

| 値 | 出力例 |
|----|--------|
| `full`（デフォルト） | 2024-01-15 19:00 - 2024-01-15 21:00 (Asia/Tokyo) |
| `smart` | 1/15 19:00-21:00（同日なら終了日省略） |
| `relative` | 今日 19:00-21:00、明日 10:00-12:00 |

**date_format オプション:**

| 値 | 出力例 |
|----|--------|
| `ymd`（デフォルト） | 2024-01-15 |
| `md` | 1/15 |
| `md_japanese` | 1月15日 |

### 5. GPU実利用状況監視アダプタの設定（オプション・実験的機能）

実際のGPU利用状況を予約と突き合わせる機能（未予約利用の検知等）の監視部分は、
`ResourceUsageObserver`ポートの実装を差し替えることで研究室のインフラに合わせられます。
現時点で提供している実装は以下の2つです。

| 実装 | 用途 |
|------|------|
| Mock | テスト・開発用（常に空の結果を返す） |
| `SharedFileResourceUsageObserver` | 共有ファイルシステム経由で各サーバーのGPU利用状況を読み取る |

`SharedFileResourceUsageObserver`を使う場合、各GPUサーバー側で`gpu-usage-reporter`バイナリを
cron登録する必要があります。このバイナリはリリースアーカイブ
（`lab-resource-manager-x86_64-unknown-linux-musl.tar.gz`等）に`lab-resource-manager`本体と
一緒に同梱されています。

**前提条件:**

- 監視対象の全サーバー（例: Thalys, Freccia, Lyria）から読み書きできる共有ディレクトリ（NFS等）
- 各サーバーに`nvidia-smi`・`getconf`・`getent`（いずれも標準的なLinux環境に含まれる）があること。
  `gpu-usage-reporter`自体はmuslスタティックビルドのため、これ以外の実行時依存はない

**セットアップ手順:**

このバイナリは「送り出す側」の設定です。LRM本体が動くサーバー（例: Thalys）だけでなく、
**監視対象の全サーバー（Thalys・Freccia・Lyriaそれぞれ）に個別にデプロイ**し、
各サーバー自身の`--server-name`を指定してcron登録してください。

1. `gpu-usage-reporter`を監視対象の全サーバーに配置する
2. 各サーバーのcrontabに、そのサーバー自身の名前で登録する（1分間隔の例）:

```cron
# Thalys側のcrontab
* * * * * /usr/local/bin/gpu-usage-reporter \
    --server-name Thalys --output-dir /mnt/shared/lrm-gpu-status

# Freccia側のcrontab
* * * * * /usr/local/bin/gpu-usage-reporter \
    --server-name Freccia --output-dir /mnt/shared/lrm-gpu-status

# Lyria側のcrontab
* * * * * /usr/local/bin/gpu-usage-reporter \
    --server-name Lyria --output-dir /mnt/shared/lrm-gpu-status
```

`--output-dir`はどのサーバーからも同じ共有ディレクトリを指す必要があります（前提条件参照）。
`--server-name`には`config/resources.toml`の`servers[].name`と一致する値を、
**そのサーバー自身の名前で**指定してください（他サーバーの名前を指定すると誤ったサーバーの
レポートとして上書きされます）。

3. 出力される`{server名を小文字化}.json`のスキーマ:

```json
{
  "server": "Thalys",
  "generated_at": "2026-07-24T12:00:00+00:00",
  "processes": [
    {"device_number": 0, "os_user": "kkawaguchi", "started_at": "2026-07-24T10:00:00+00:00"}
  ]
}
```

`generated_at`から一定時間（既定5分想定）経過したファイルは古いデータとみなされ無視されます。
cronが停止した場合に古い利用状況を「今も使用中」と誤判定しないための仕組みです。

**現時点での制限**: このスクリプトによるJSON出力までが動作確認済みです。
LRM本体がこのファイルを定期的に読み取って予約と突き合わせる処理、および検知結果をSlackへ
通知する部分は開発中のため、`AppConfig`にはまだこの監視機能向けの環境変数はありません。

## システムの起動

### サービス管理

```bash
# サービスを起動
sudo systemctl start lab-resource-manager

# サービスを停止
sudo systemctl stop lab-resource-manager

# ステータスを確認
sudo systemctl status lab-resource-manager

# ログを確認
sudo journalctl -u lab-resource-manager -f

# 自動起動を有効化
sudo systemctl enable lab-resource-manager
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

## インストール

[GitHub Releases](https://github.com/kano-lab/lab-resource-manager/releases)から最新版をダウンロードして実行:

```bash
# 展開してインストール
tar -xzf lab-resource-manager-x86_64-unknown-linux-gnu.tar.gz
sudo bash deploy/install.sh
```

インストールされるもの:

- `/usr/local/bin/lab-resource-manager` - メインバイナリ
- `/etc/lab-resource-manager/` - 設定ディレクトリ
- `/var/lib/lab-resource-manager/` - データディレクトリ
- `/etc/systemd/system/lab-resource-manager.service` - systemdサービス

Dockerデプロイからアップグレードする場合は、[マイグレーションガイド](MIGRATION_ja.md)を参照してください。
