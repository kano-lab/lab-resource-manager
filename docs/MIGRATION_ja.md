# マイグレーションガイド: Docker からバイナリリリースへ (v1.0.0)

このガイドでは、Docker ベースのデプロイから新しいバイナリリリース + systemd への移行方法を説明します。

## 概要

v1.0.0 ではデプロイ方式が大きく変更されました：

| 項目 | 変更前 (Docker) | 変更後 (v1.0.0) |
|------|-----------------|-----------------|
| デプロイ方式 | Docker Compose | バイナリ + systemd |
| 設定ファイル | `config/` | `/etc/lab-resource-manager/` |
| データ | `data/` | `/var/lib/lab-resource-manager/` |
| 環境変数 | `.env` | `/etc/default/lab-resource-manager` |
| バイナリ | コンテナ内 | `/usr/local/bin/lab-resource-manager` |

## 前提条件

- サーバーへのroot権限
- 既存データのバックアップ

## 移行手順

### 1. Dockerコンテナの停止

```bash
cd /path/to/lab-resource-manager
docker compose down
```

### 2. 既存データのバックアップ

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

### 3. 新バージョンのダウンロードとインストール

```bash
# リリースをダウンロード
curl -LO https://github.com/kano-lab/lab-resource-manager/releases/download/v1.0.0/lab-resource-manager-x86_64-unknown-linux-gnu.tar.gz

# 展開
tar -xzf lab-resource-manager-x86_64-unknown-linux-gnu.tar.gz

# インストーラーを実行（root権限が必要）
sudo bash deploy/install.sh
```

### 4. データファイルの移行

```bash
# データファイルを新しい場所にコピー
sudo cp ~/lrm-backup/identity_links.json /var/lib/lab-resource-manager/
sudo cp ~/lrm-backup/google_calendar_mappings.json /var/lib/lab-resource-manager/

# 設定ファイルをコピー
sudo cp ~/lrm-backup/resources.toml /etc/lab-resource-manager/

# サービスアカウントキーをコピー（パスは適宜調整）
sudo cp /path/to/service-account.json /etc/lab-resource-manager/

# 所有権を設定
sudo chown -R lrm:lrm /var/lib/lab-resource-manager/
sudo chown -R lrm:lrm /etc/lab-resource-manager/
```

### 5. 環境変数ファイルの作成

`.env` を新しい形式に変換します：

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

### 6. サービスの起動

```bash
# サービスを起動
sudo systemctl start lab-resource-manager

# ステータスを確認
sudo systemctl status lab-resource-manager

# ログを確認
sudo journalctl -u lab-resource-manager -f

# 自動起動を有効化
sudo systemctl enable lab-resource-manager
```

### 7. 動作確認

1. Slack botがコマンドに応答することを確認
2. カレンダー連携が動作することを確認
3. ログにエラーがないか監視

### 8. クリーンアップ（任意）

すべてが正常に動作することを確認した後：

```bash
# Dockerリソースを削除
docker compose down --rmi all --volumes

# 古いファイルを削除
rm -rf /path/to/lab-resource-manager  # 古いDockerデプロイディレクトリ
```

## ロールバック

問題が発生した場合、Dockerに戻すことができます：

```bash
# systemdサービスを停止
sudo systemctl stop lab-resource-manager
sudo systemctl disable lab-resource-manager

# Dockerデプロイを復元
cd /path/to/lab-resource-manager-backup
docker compose up -d
```

## トラブルシューティング

### サービスが起動しない

ログを確認：

```bash
sudo journalctl -u lab-resource-manager -e
```

よくある原因：

- `/etc/default/lab-resource-manager` の環境変数が不足
- ファイルのパーミッションが不正
- データファイルが存在しない

### Permission denied エラー

所有権を確認：

```bash
sudo chown -R lrm:lrm /var/lib/lab-resource-manager/
sudo chown -R lrm:lrm /etc/lab-resource-manager/
```

### 設定ファイルが見つからない

`/etc/default/lab-resource-manager` のパスが絶対パスで、ファイルが存在することを確認してください。

## サポート

問題が発生した場合は、Issue を作成してください：
<https://github.com/kano-lab/lab-resource-manager/issues>
