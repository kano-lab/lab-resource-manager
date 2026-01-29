#!/bin/bash
# LRM ローカル実行スクリプト
# 使用方法: ./scripts/run-local.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_DIR"

# 環境変数ファイルの存在確認
if [ ! -f "secrets/.env" ]; then
    echo "❌ secrets/.env が見つかりません"
    echo "   .env.example をコピーして設定してください:"
    echo "   cp .env.example secrets/.env"
    exit 1
fi

# 環境変数を読み込み
set -a
source secrets/.env
set +a

# Slackトークンの確認
if [[ "$SLACK_BOT_TOKEN" == "xoxb-YOUR-BOT-TOKEN" ]]; then
    echo "⚠️  SLACK_BOT_TOKEN がデフォルト値のままです"
    echo "   secrets/.env で実際のトークンを設定してください"
fi

if [[ "$SLACK_APP_TOKEN" == "xapp-YOUR-APP-TOKEN" ]]; then
    echo "⚠️  SLACK_APP_TOKEN がデフォルト値のままです"
    echo "   secrets/.env で実際のトークンを設定してください"
fi

echo "🚀 LRM を起動します..."
echo "   RESOURCE_CONFIG: $RESOURCE_CONFIG"
echo "   IDENTITY_LINKS_FILE: $IDENTITY_LINKS_FILE"
echo ""

cargo run --release
