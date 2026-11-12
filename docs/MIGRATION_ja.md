# マイグレーションガイド

## 予約の言葉の見直し (v3.0.0)

v3.0.0 では、予約を「予約（単発予約・定期予約）」と「予約枠」の言葉で扱うように作り直しました。
カレンダーに保存されている予約、`resources.toml`、環境変数はそのまま使えます。
利用者・管理者に影響があるのは次の3点です。

### MCPツール名の変更

エージェントは接続のたびにツール一覧を読み直すため、通常は何もしなくて構いません。
ツール名を固定で書いた設定やプロンプトがある場合は書き換えてください。

| 以前 | v3.0.0 |
| --- | --- |
| `list_all_reservations` | `list_slots` |
| `list_my_reservations` | `list_my_slots` |
| `get_reservation` | `get_slot`（予約枠の取得） |
| `update_reservation` | `update_slot` |
| `release_reservation_early` | `end_slot_early` |
| `cancel_reservation` | `cancel_slot`（予約枠1つの取り消し） |

`get_reservation`と`cancel_reservation`は、予約（単発予約・定期予約）を丸ごと扱うツールとして
新しく用意しました。以前の`cancel_reservation`と同じ「1つだけ取り消す」動作が必要なら`cancel_slot`を使ってください。
予約枠の出力には、属する予約のID（`reservation_id`）と種類（`reservation_kind`）が加わりました。

### 通知テンプレートの設定キー

単発予約の取り消しのテンプレートは`deleted`から`cancelled`に名前が変わりました。
以前の`deleted`も引き続き読み込まれますが、`cancelled`への書き換えをおすすめします。
定期予約の予約枠1つについての通知用に、`slot_updated`と`slot_cancelled`が加わりました。

### ログのフィールド名

ログのフィールド`usage_id`は`slot_id`（予約枠）または`reservation_id`（予約）に、
`owner`は`holder`になりました。変更検知の要約行は次の形になります。

```text
change detection pass finished
  slots=23 appeared=0 disappeared=0 notifications=0 elapsed_ms=380
```

journalctlでこれらのフィールドを絞り込みに使っている場合は書き換えてください。

## ストレージ設定スキーマの変更 (v2.0.0)

v2.0.0 では `resources.toml` 設定ファイルの形式に破壊的変更が入ります。カレンダーID
マッピングがリソース定義から分離され、関心の分離が改善されました。

### 設定形式の変更

予約データの永続化先の指定（ストレージマッピング）がリソース定義
（「何が予約できるか」）から分離されました。

**変更前（v1.x）:**

```toml
[[servers]]
name = "gpu-server-1"
calendar_id = "xxx@group.calendar.google.com"

[[servers.devices]]
id = 0
model = "A100 80GB PCIe"

[[rooms]]
name = "Meeting Room A"
calendar_id = "yyy@group.calendar.google.com"
```

**変更後（v2.0.0）:**

```toml
[[servers]]
name = "gpu-server-1"

[[servers.devices]]
id = 0
model = "A100 80GB PCIe"

[[rooms]]
name = "Meeting Room A"

[storage]
type = "google_calendar"

[storage.calendars]
"gpu-server-1" = "xxx@group.calendar.google.com"
"Meeting Room A" = "yyy@group.calendar.google.com"
```

### v2.0.0 移行手順

1. **現在の設定をバックアップ**

   ```bash
   cp /etc/lab-resource-manager/resources.toml ~/resources.toml.backup
   ```

2. **resources.toml を更新**

   設定内の各サーバーと部屋に対して：
   - `[[servers]]` または `[[rooms]]` から `calendar_id` 行を削除
   - ファイルの末尾に新しい `[storage]` セクションを追加
   - `[storage.calendars]` にリソース名 → カレンダーID マッピングを記述

   **移行例:**

   以下を：

   ```toml
   [[servers]]
   name = "gpu-server-1"
   calendar_id = "xxx@group.calendar.google.com"
   ```

   このように書き換えます：

   ```toml
   [[servers]]
   name = "gpu-server-1"

   # ... 他のサーバー設定 ...

   [storage]
   type = "google_calendar"

   [storage.calendars]
   "gpu-server-1" = "xxx@group.calendar.google.com"
   ```

3. **設定を検証**

   サービスを再起動して新しい設定を検証します：

   ```bash
   sudo systemctl restart lab-resource-manager
   sudo journalctl -u lab-resource-manager -f
   ```

   写像に不一致がある場合、どのリソースがカレンダーID マッピングを欠いているかを
   列挙したエラーメッセージが表示されます。

4. **検証チェックリスト**

   - [ ] `[[servers]]` 内のすべてのサーバー名が `[storage.calendars]` にある
   - [ ] `[[rooms]]` 内のすべての部屋名が `[storage.calendars]` にある
   - [ ] サービスがエラーなく起動する: `sudo systemctl status lab-resource-manager`
   - [ ] Slack ボットがコマンドに応答する
   - [ ] カレンダー連携が動作する（ログで確認）

### v2.0.0 トラブルシューティング

#### エラー: "resources.toml の以下のリソースが storage.calendars に写像を持ちません"

これは、一部のサーバーまたは部屋がカレンダーID マッピングを欠いていることを意味します。
確認してください：

```bash
# 設定されているリソースを確認
grep '^name = ' /etc/lab-resource-manager/resources.toml

# すべてが storage.calendars セクションにあることを確認
grep -A 100 '\[storage.calendars\]' /etc/lab-resource-manager/resources.toml
```

欠けているエントリを `[storage.calendars]` に追加してください。

### v2.0.0 注釈

- `storage` セクションは `type = "google_calendar"` を指定する必要があります
  （現在唯一のサポートされたバックエンド）
- すべてのリソース名（サーバーと部屋）はちょうど 1 つのカレンダーID マッピングを
  持つ必要があります
- 写像が不完全な場合、アプリケーション起動時に明確なエラーで失敗します
- 定義されているが使用されていないカレンダーに対して警告ログが表示されます
  （今後リソースを追加予定の場合は無視できます）

### google_calendar_mappings.json の廃止 (v2.0.0)

v2.0.0 では `google_calendar_mappings.json` ファイルと `GOOGLE_CALENDAR_MAPPINGS_FILE`
環境変数を削除しました。v1.5.1 以降、カレンダーイベントIDは予約IDから導出されるため、
このファイルは **v1.5.1 より前**に作成された予約（イベントIDがGoogle採番で、予約IDから
導出できないもの）の解決だけを担っていました。

そうした古い予約がどうなるか: 運用上壊れることはありませんが、**番号が振り直されます**。
以後は対応表のIDではなくイベントIDで解決されるため、古いイベント説明からコピーした
予約IDは解決できなくなります。説明は予約を更新するたびに再生成されるので、これは
自然に収束します。v1.5.1 より前に作成された予約がまだ進行中・予定にあり、そのIDを
安定させたい場合は、アップグレード前に一度その予約を更新してください。

手順:

1. `/etc/default/lab-resource-manager` から `GOOGLE_CALENDAR_MAPPINGS_FILE` の行を削除します

2. ファイルを削除します（残したければバックアップを取ってから）:

   ```bash
   cp /var/lib/lab-resource-manager/google_calendar_mappings.json ~/google_calendar_mappings.json.backup
   rm /var/lib/lab-resource-manager/google_calendar_mappings.json
   ```

ファイルや環境変数を誤って残しても、どちらも単に無視されます。

---

## Docker からバイナリリリースへ (v1.0.0)

このガイドでは、Docker ベースのデプロイから新しいバイナリリリース + systemd
への移行方法を説明します。

### 概要

v1.0.0 ではデプロイ方式が大きく変更されました：

| 項目 | 変更前 (Docker) | 変更後 (v1.0.0) |
| --- | --- | --- |
| デプロイ方式 | Docker Compose | バイナリ + systemd |
| 設定ファイル | `config/` | `/etc/lab-resource-manager/` |
| データ | `data/` | `/var/lib/lab-resource-manager/` |
| 環境変数 | `.env` | `/etc/default/lab-resource-manager` |
| バイナリ | コンテナ内 | `/usr/local/bin/lab-resource-manager` |

### 前提条件

- サーバーへのroot権限
- 既存データのバックアップ

### v1.0.0 移行手順

#### 1. Dockerコンテナの停止

```bash
cd /path/to/lab-resource-manager
docker compose down
```

#### 2. 既存データのバックアップ

```bash
# バックアップディレクトリを作成
mkdir -p ~/lrm-backup

# データファイルをバックアップ
cp data/identity_links.json ~/lrm-backup/
cp data/google_calendar_mappings.json ~/lrm-backup/

# 設定をバックアップ
cp config/resources.toml ~/lrm-backup/

# 環境変数をバックアップ
cp .env ~/lrm-backup/
```

#### 3. 新バージョンのダウンロードとインストール

```bash
# リリースをダウンロード
curl -LO https://github.com/kano-lab/lab-resource-manager/releases/download/v1.0.0/lab-resource-manager-x86_64-unknown-linux-gnu.tar.gz

# 展開
tar -xzf lab-resource-manager-x86_64-unknown-linux-gnu.tar.gz

# インストール実行（rootとして）
sudo bash deploy/install.sh
```

#### 4. データファイルの移行

```bash
# データファイルを新しい場所にコピー
sudo cp ~/lrm-backup/identity_links.json /var/lib/lab-resource-manager/
sudo cp ~/lrm-backup/google_calendar_mappings.json /var/lib/lab-resource-manager/

# 設定ファイルをコピー
sudo cp ~/lrm-backup/resources.toml /etc/lab-resource-manager/

# サービスアカウントキーをコピー（必要に応じてパスを調整）
sudo cp /path/to/service-account.json /etc/lab-resource-manager/

# 所有権を設定
sudo chown -R lrm:lrm /var/lib/lab-resource-manager/
sudo chown -R lrm:lrm /etc/lab-resource-manager/
```

#### 5. 環境ファイルの作成

`.env` を新しい形式に変換：

```bash
sudo tee /etc/default/lab-resource-manager << 'EOF'
SLACK_BOT_TOKEN=xoxb-your-token
SLACK_APP_TOKEN=xapp-your-token
GOOGLE_SERVICE_ACCOUNT_KEY=/etc/lab-resource-manager/service-account.json
RESOURCE_CONFIG=/etc/lab-resource-manager/resources.toml
IDENTITY_LINKS_FILE=/var/lib/lab-resource-manager/identity_links.json
GOOGLE_CALENDAR_MAPPINGS_FILE=/var/lib/lab-resource-manager/google_calendar_mappings.json
RUST_LOG=info
EOF

# ファイルを保護
sudo chmod 600 /etc/default/lab-resource-manager
```

#### 6. サービスの起動

```bash
# サービスを起動
sudo systemctl start lab-resource-manager

# 状態を確認
sudo systemctl status lab-resource-manager

# ログを表示
sudo journalctl -u lab-resource-manager -f

# ブート時に起動を有効化
sudo systemctl enable lab-resource-manager
```

#### 7. 動作確認

1. Slack ボットがコマンドに応答することを確認
2. カレンダー連携が動作することを確認
3. ログを監視してエラーがないか確認

#### 8. クリーンアップ（オプション）

すべてが動作することを確認した後：

```bash
# Docker リソースを削除
docker compose down --rmi all --volumes

# 古いファイルを削除
rm -rf /path/to/lab-resource-manager  # 旧 Docker デプロイディレクトリ
```

### ロールバック

問題が発生した場合、Docker にロールバックできます：

```bash
# systemd サービスを停止
sudo systemctl stop lab-resource-manager
sudo systemctl disable lab-resource-manager

# Docker デプロイを復元
cd /path/to/lab-resource-manager-backup
docker compose up -d
```

### v1.0.0 トラブルシューティング

#### サービスの起動に失敗する

ログを確認：

```bash
sudo journalctl -u lab-resource-manager -e
```

一般的な問題：

- `/etc/default/lab-resource-manager` に環境変数がない
- ファイルパーミッションが正しくない
- データファイルがない

#### パーミッション拒否エラー

正しい所有権を確認：

```bash
sudo chown -R lrm:lrm /var/lib/lab-resource-manager/
sudo chown -R lrm:lrm /etc/lab-resource-manager/
```

#### 設定が見つからない

`/etc/default/lab-resource-manager` のパスが絶対パスであり、ファイルが存在することを
確認してください。

### サポート

問題が発生した場合は、以下で issue を開いてください：
<https://github.com/kano-lab/lab-resource-manager/issues>
