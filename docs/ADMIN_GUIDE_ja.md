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
name = "gpu-server-1"
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
| `full`（デフォルト） | gpu-server-1 / A100 80GB PCIe / GPU:0 |
| `compact` | gpu-server-1 0,1,2 |
| `server_only` | gpu-server-1 |

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

- 監視対象の全サーバーから読み書きできる共有ディレクトリ（NFS等）
- 各サーバーに`nvidia-smi`・`getconf`・`getent`（いずれも標準的なLinux環境に含まれる）があること。
  `gpu-usage-reporter`自体はmuslスタティックビルドのため、これ以外の実行時依存はない

**セットアップ手順:**

このバイナリは「送り出す側」の設定です。LRM本体が動くサーバー自身も含め、
**監視対象の全サーバーに個別にデプロイ**し、
各サーバー自身の`--server-name`を指定してcron登録してください。

1. `gpu-usage-reporter`を監視対象の全サーバーに配置する
2. 各サーバーのcrontabに、そのサーバー自身の名前で登録する（1分間隔の例）:

   ```cron
   # gpu-server-1側のcrontab
   * * * * * /usr/local/bin/gpu-usage-reporter \
       --server-name gpu-server-1 --output-dir /mnt/shared/lrm-gpu-status

   # gpu-server-2側のcrontab
   * * * * * /usr/local/bin/gpu-usage-reporter \
       --server-name gpu-server-2 --output-dir /mnt/shared/lrm-gpu-status
   ```

   `--output-dir`はどのサーバーからも同じ共有ディレクトリを指す必要があります（前提条件参照）。
   `--server-name`には`config/resources.toml`の`servers[].name`と一致する値を、
   **そのサーバー自身の名前で**指定してください（他サーバーの名前を指定すると誤ったサーバーの
   レポートとして上書きされます）。

3. 出力される`{server名を小文字化}.json`のスキーマ:

```json
{
  "server": "gpu-server-1",
  "generated_at": "2026-07-24T12:00:00+00:00",
  "processes": [
    {"device_number": 0, "os_user": "alice", "started_at": "2026-07-24T10:00:00+00:00"}
  ]
}
```

`generated_at`から一定時間（既定5分想定）経過したファイルは古いデータとみなされ無視されます。
cronが停止した場合に古い利用状況を「今も使用中」と誤判定しないための仕組みです。

**LRM本体側での機能有効化:** 実利用と予約の突合処理（未予約利用の事後予約提案をSlack DMで送る、
無断使用を通知する）はデフォルトでは無効です。上記の`gpu-usage-reporter`と同じ共有ディレクトリを
`GPU_USAGE_REPORTS_DIR`に指定すると有効になります。

| 環境変数 | デフォルト値 | 用途 |
|---------|-------------|------|
| `GPU_USAGE_REPORTS_DIR` | (未設定=機能無効) | `SharedFileResourceUsageObserver`が読み取る共有ディレクトリ |
| `GPU_USAGE_MAX_STALENESS_SECS` | `300` | レポートを無視し始める経過時間（秒） |
| `UNRESERVED_USAGE_THRESHOLD_SECS` | `600` | 未予約利用を提案対象とみなす継続時間の閾値（秒） |
| `RESERVATION_PROPOSAL_DURATION_CANDIDATES_HOURS` | `1,2,3,5,8` | Slack DMで提示する利用時間候補（時間、カンマ区切り） |

有効化すると、OSアカウントが紐付け済み（`/link-user`）かつSlackアカウントも紐付け済みの利用者に、
利用時間候補ごとのボタン付きDMが送られます。ボタンを押すと、実際に利用が始まった時刻から
遡って予約が作成されます。

他人の既存予約と競合する利用（予約者と異なる人物がリソースを使用している）を検知した場合は、
リソースの通知チャンネルへブロードキャストするのではなく、**実際に利用している本人へ直接DM**を
送り、利用の停止または自分自身での予約を促します。このDMは、実際の利用者のOSアカウントが
Slackアカウントも紐付いたメールアドレスにリンクされている場合にのみ送信できます。サーバー上で
観測されたOSアカウントが誰にもリンクされていない場合、それが無断使用かどうか原理的に判定
できないため通知自体をスキップします。この検知機能を機能させるには、サーバー利用者全員を
`/link-user`で紐付けておく必要があります。

**現時点での制限**: この配線はモックアダプタを使ったユニット/統合テストで検証済みですが、
実際のSlack DM送信（`conversations.open`/`chat.postMessage`）と、実GPUサーバーでの
`gpu-usage-reporter`の動作確認は、実運用環境ではまだ行われていません。Slack DMの送信には
Slackアプリに`im:write`・`chat:write`スコープが必要です。

### 6. MCPサーバーの設定（オプション）

Claude Codeなどのエージェントから、LRM本体プロセス内に組み込まれたMCP（Model Context
Protocol）サーバー経由で予約の閲覧・作成・更新・キャンセルができます。新しいバイナリや
systemdサービスは不要で、既存の`lab-resource-manager`プロセス内にHTTP/SSEリスナーが
追加のタスクとして起動します。Googleサービスアカウントキーは引き続きLRM本体のホスト
1箇所にしか存在しません。

**前提**: 研究室のサーバー群とメンバーの端末が同一LAN内で相互に通信できること。インターネット
への公開は範囲外です。

**環境変数:**

| 環境変数 | デフォルト値 | 用途 |
|---------|-------------|------|
| `MCP_LISTEN_ADDR` | (未設定=機能無効) | MCPサーバーのHTTP/SSEリッスンアドレス（例: `0.0.0.0:8787`） |
| `MCP_TOKENS_FILE` | `/var/lib/lab-resource-manager/mcp_tokens.json` | MCPアクセストークンの永続化ファイル |
| `MCP_ALLOWED_HOSTS` | (必須。`MCP_LISTEN_ADDR`設定時に未設定だと起動失敗) | リクエストで許可する`Host`ヘッダの値（カンマ区切り、例: `<LRM本体のホスト>:8787,192.168.1.10:8787`） |
| `MCP_TLS_CERT_FILE` | (未設定=TLS無効) | MCPサーバーのTLS証明書ファイルパス（PEM形式）。`MCP_TLS_KEY_FILE`とセットで指定 |
| `MCP_TLS_KEY_FILE` | (未設定=TLS無効) | MCPサーバーのTLS秘密鍵ファイルパス（PEM形式）。`MCP_TLS_CERT_FILE`とセットで指定 |

`MCP_ALLOWED_HOSTS`は、LRM本体が実際に到達可能なホスト名/IPとポートの組み合わせを、
メンバーがクライアント設定で使う値と一致させて指定してください。`MCP_LISTEN_ADDR`を設定して
MCP機能を有効化する場合、`MCP_ALLOWED_HOSTS`の指定は必須です（未設定だと起動時にエラーで
終了します。設定し忘れたまま検証なしで動いてしまうことを防ぐためのfail-closedな挙動です）。

`MCP_TLS_CERT_FILE`/`MCP_TLS_KEY_FILE`は片方だけ設定すると起動時エラーになります。TLSは
推奨ですが必須ではありません。両方とも未設定ならHTTPで起動し、Bearerトークンが平文で
送信される旨の警告ログを出します。

**TLSの設定（推奨）:**

Bearerトークンを平文でLAN上に流さないため、TLS化を強く推奨します。自己署名の内部CAを
1回だけ作成し、そこから発行したサーバー証明書をLRM本体に設定します:

```bash
# 1. 内部CAを作成（1回限り）
openssl req -x509 -newkey rsa:4096 -keyout ca.key -out ca.crt -days 3650 -nodes \
  -subj "/CN=lab-resource-manager internal CA"

# 2. LRM本体用のサーバー証明書をそのCAで署名して発行
#    （SANに実際のホスト名/IPを含めること）
openssl req -newkey rsa:2048 -keyout mcp.key -out mcp.csr -nodes -subj "/CN=<LRM本体のホスト>"
openssl x509 -req -in mcp.csr -CA ca.crt -CAkey ca.key -CAcreateserial \
  -out mcp.crt -days 825 \
  -extfile <(echo "subjectAltName=DNS:<LRM本体のホスト>,IP:<LRM本体のIP>")
```

```bash
MCP_TLS_CERT_FILE=/etc/lab-resource-manager/mcp.crt
MCP_TLS_KEY_FILE=/etc/lab-resource-manager/mcp.key
```

**クライアント側の信頼設定**: 証明書の信頼はMCPクライアントのアプリ単位の設定ではなく、
**OSの証明書ストア単位**で設定できるため、`ca.crt`を各メンバーの端末に配って回る必要は
ありません。メンバーが共有マシン（例: 各自がSSHでログインして使う共有GPUサーバー）上で
Claude Codeなどのエージェントセッションを動かす運用であれば、そのマシンのシステム証明書
ストアに`ca.crt`を1回登録するだけで、そこで動く全プロセス（MCPクライアントがどんな内部
HTTPライブラリを使っていても）が自動的に証明書を信頼するようになります:

```bash
# エージェントセッションが動く共有マシンそれぞれで実行（Debian/Ubuntu系の例）
sudo cp ca.crt /usr/local/share/ca-certificates/lab-resource-manager.crt
sudo update-ca-certificates
```

CA証明書を信頼させること自体は受動的な設定で、誰にも新しいアクセス権を与えません（実際の
認可は引き続きBearerトークンが担います）。将来、メンバーが個人のノートPC等からも接続したく
なった場合は、その端末にも同じ`ca.crt`を追加してください。

**認証: `/mcp-token`コマンド**

メンバーはSlackで以下を実行し、自分専用のアクセストークンを取得します:

```text
/mcp-token
```

呼び出した本人のメールアドレス（`/link-user`または`/register-calendar`で紐付け済みである
必要があります）宛に、本人にのみ見えるエフェメラルメッセージでトークンが表示されます。
再実行すると新しいトークンが発行され、古いトークンは自動的に失効します（1人1トークン）。

**メンバー側の設定例（`.mcp.json`）:**

```json
{
  "mcpServers": {
    "lab-resource-manager": {
      "url": "https://<LRM本体のホスト>:8787/mcp",
      "headers": {
        "Authorization": "Bearer <発行されたトークン>"
      }
    }
  }
}
```

（TLS未設定の場合は`https://`の代わりに`http://`を使ってください。）

**提供ツール:**

| ツール名 | 内容 |
|---------|------|
| `list_all_reservations` | 今後（進行中含む）の全予約を一覧表示 |
| `list_my_reservations` | 自分が所有する予約を一覧表示 |
| `get_reservation` | IDを指定して予約の詳細を取得 |
| `create_reservation` | 新しい予約を作成（GPUサーバーまたは部屋） |
| `update_reservation` | 自分が所有する予約の時間・備考を更新 |
| `cancel_reservation` | 自分が所有する予約をキャンセル |

書き込み系ツール（作成・更新・キャンセル）は、呼び出し元のBearerトークンに紐づく
メールアドレスを所有者として扱います。他人が所有する予約の更新・キャンセルは、
`/reserve`と同じ既存の認可ルール（所有者本人のみ）により拒否されます。

TLSの動作自体（自己署名証明書でのハンドシェイク成立、CA未信頼時の拒否、認証ミドルウェアが
TLS層とは独立してBearerトークンを要求すること）は開発環境で確認済みです。

**現時点での制限**: LAN内の別端末からのHTTP/SSE接続確認、`/mcp-token`の実Slack動作確認、
およびClaude Code等の実際のMCPクライアントが共有マシンに登録したCA証明書を実際に
信頼して接続できることの確認は、開発環境のDockerサンドボックスでは実行できないため
未検証です。

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

管理者は、他の人物の識別情報をメールアドレスに紐付けできます:

```text
/link-user
```

モーダルが開き、ラジオボタンで紐付け対象を切り替えられます:

- **Slackユーザー**: Slackユーザーを選択し、メールアドレスを入力します。従来どおりGoogle
  Calendarへのアクセス権を付与します。
- **OSユーザー名**: GPUサーバーを選択し、その人物のOSユーザー名とメールアドレスを入力します。
  これにより、上記の未予約利用の事後予約提案・無断使用DM機能が「実際に誰がプロセスを実行して
  いるか」を特定できるようになります。これを行わないと、観測されたOSユーザー名は誰にも
  解決できません。

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
