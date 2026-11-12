//! Google Calendar APIへのイベント操作を集約するゲートウェイ (内部実装)

use crate::domain::ports::repositories::RepositoryError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use google_calendar3::{
    CalendarHub, api::Event, hyper_rustls::HttpsConnector,
    hyper_util::client::legacy::connect::HttpConnector,
};

/// 1ページあたりの最大取得件数（Google Calendar APIの上限値）
const MAX_RESULTS_PER_PAGE: i32 = 2500;

/// Google Calendarのイベント操作
///
/// リポジトリは「どの期間を問い合わせるか」「取得したイベントをどう解釈するか」を担い、
/// このゲートウェイは「APIをどう呼ぶか」だけを担う。
#[async_trait]
pub(super) trait CalendarEventGateway: Send + Sync {
    /// 終了時刻が`time_min`より後のイベントを取得する
    ///
    /// `time_max`を指定した場合は、開始時刻が`time_max`より前のイベントに限定される。
    /// すなわち`time_min`〜`time_max`の期間に重なるイベントが返る。
    async fn list_events(
        &self,
        calendar_id: &str,
        time_min: DateTime<Utc>,
        time_max: Option<DateTime<Utc>>,
    ) -> Result<Vec<Event>, RepositoryError>;

    /// IDを指定して単一のイベントを取得する（存在しない場合は`None`）
    async fn get_event(
        &self,
        calendar_id: &str,
        event_id: &str,
    ) -> Result<Option<Event>, RepositoryError>;

    /// イベントを作成し、作成されたイベントを返す
    async fn insert_event(&self, calendar_id: &str, event: Event)
    -> Result<Event, RepositoryError>;

    /// 既存イベントを更新する
    async fn update_event(
        &self,
        calendar_id: &str,
        event_id: &str,
        event: Event,
    ) -> Result<(), RepositoryError>;

    /// イベントを削除する
    async fn delete_event(&self, calendar_id: &str, event_id: &str) -> Result<(), RepositoryError>;
}

/// Google Calendar APIを直接呼び出す実装
pub(super) struct GoogleCalendarEventGateway {
    hub: CalendarHub<HttpsConnector<HttpConnector>>,
}

impl GoogleCalendarEventGateway {
    pub(super) fn new(hub: CalendarHub<HttpsConnector<HttpConnector>>) -> Self {
        Self { hub }
    }
}

#[async_trait]
impl CalendarEventGateway for GoogleCalendarEventGateway {
    async fn list_events(
        &self,
        calendar_id: &str,
        time_min: DateTime<Utc>,
        time_max: Option<DateTime<Utc>>,
    ) -> Result<Vec<Event>, RepositoryError> {
        let mut all_events = Vec::new();
        let mut page_token: Option<String> = None;

        // 1ページに収まらない場合、続きのページを取得しないとイベントを取りこぼす
        loop {
            let mut call = self
                .hub
                .events()
                .list(calendar_id)
                .time_min(time_min)
                // 繰り返しイベントを個々の予定へ展開する。展開しないと繰り返しの
                // 2回目以降が取得できず、予約として扱えない
                .single_events(true)
                .max_results(MAX_RESULTS_PER_PAGE);

            if let Some(time_max) = time_max {
                call = call.time_max(time_max);
            }

            if let Some(token) = &page_token {
                call = call.page_token(token);
            }

            let (_response, events) = call.doit().await.map_err(|e| {
                RepositoryError::ConnectionError(format!("Calendar API error: {}", e))
            })?;

            all_events.extend(events.items.unwrap_or_default());

            match events.next_page_token {
                Some(token) => page_token = Some(token),
                None => break,
            }
        }

        Ok(all_events)
    }

    async fn get_event(
        &self,
        calendar_id: &str,
        event_id: &str,
    ) -> Result<Option<Event>, RepositoryError> {
        match self.hub.events().get(calendar_id, event_id).doit().await {
            Ok((_response, event)) => Ok(Some(event)),
            Err(e) => {
                // HTTPステータスコード404の場合はNoneを返す
                // google_calendar3のエラーは構造化されていないため、
                // エラーメッセージから404を検出する
                // TODO(#41): 文字列マッチングは脆弱。構造化されたエラー型またはHTTPステータスコードを直接チェック
                let error_msg = e.to_string();
                if error_msg.contains("404") || error_msg.contains("Not Found") {
                    Ok(None)
                } else {
                    Err(RepositoryError::ConnectionError(format!(
                        "Calendar API error: {}",
                        e
                    )))
                }
            }
        }
    }

    async fn insert_event(
        &self,
        calendar_id: &str,
        event: Event,
    ) -> Result<Event, RepositoryError> {
        let (_response, created_event) = self
            .hub
            .events()
            .insert(event, calendar_id)
            .doit()
            .await
            .map_err(|e| RepositoryError::ConnectionError(format!("イベント作成に失敗: {}", e)))?;

        Ok(created_event)
    }

    async fn update_event(
        &self,
        calendar_id: &str,
        event_id: &str,
        event: Event,
    ) -> Result<(), RepositoryError> {
        self.hub
            .events()
            .update(event, calendar_id, event_id)
            .doit()
            .await
            .map_err(|e| RepositoryError::ConnectionError(format!("イベント更新に失敗: {}", e)))?;

        Ok(())
    }

    async fn delete_event(&self, calendar_id: &str, event_id: &str) -> Result<(), RepositoryError> {
        self.hub
            .events()
            .delete(calendar_id, event_id)
            .doit()
            .await
            .map_err(|e| RepositoryError::ConnectionError(format!("イベント削除に失敗: {}", e)))?;

        Ok(())
    }
}
