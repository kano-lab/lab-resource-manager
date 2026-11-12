//! Google Calendar APIへのイベント操作を集約するゲートウェイ (内部実装)

use crate::domain::ports::repositories::RepositoryError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use google_calendar3::{
    CalendarHub,
    api::{Event, Scope},
    common, hyper,
    hyper_rustls::{HttpsConnector, HttpsConnectorBuilder},
    hyper_util::{
        client::legacy::{Client, connect::HttpConnector},
        rt::TokioExecutor,
    },
    yup_oauth2,
};

/// すべての呼び出しで求める権限
///
/// 生成されたクライアントは呼び出しごとに既定の権限を選ぶ。google-calendar3 7 から、
/// イベントの取得・一覧の既定が`calendar.events.owned.readonly`（サービスアカウント自身が
/// 持つカレンダーだけ）になった。予約のカレンダーは人が作ってサービスアカウントに共有するので、
/// その権限では見つからない（404）。共有されたカレンダーも読み書きできる権限を明示する。
const SCOPE: Scope = Scope::Full;

/// 1ページあたりの最大取得件数（Google Calendar APIの上限値）
const MAX_RESULTS_PER_PAGE: i32 = 2500;

/// Google Calendar API の起点
const CALENDAR_API_BASE: &str = "https://www.googleapis.com/calendar/v3";

/// URLのパスの1区切りとして使えるよう、IDをパーセントエンコードする
///
/// RFC 3986の非予約文字と、カレンダーIDに含まれる`@`（パスにそのまま書ける）は残す。
fn encode_path_segment(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'@' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

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
    ///
    /// 繰り返しイベントは回ごとに展開せず、繰り返しイベントそのものと、規則どおりでない回
    /// （時間などを変えた回、削除された回）のイベントとして返る。削除されたイベントも
    /// `status: cancelled`で含まれる。
    async fn list_events(
        &self,
        calendar_id: &str,
        time_min: DateTime<Utc>,
        time_max: Option<DateTime<Utc>>,
    ) -> Result<Vec<Event>, RepositoryError>;

    /// 繰り返しイベントそのものと、その規則どおりでない回のイベントをすべて取得する
    ///
    /// 削除された回も`status: cancelled`で含まれる。
    async fn list_series(
        &self,
        calendar_id: &str,
        ical_uid: &str,
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

    /// 既存イベントを、版が`version`のままなら書き換え、書き換えたイベントを返す
    ///
    /// # Errors
    /// 版が違う、またはイベントが無い場合、`RepositoryError::Conflict`
    async fn update_event(
        &self,
        calendar_id: &str,
        event_id: &str,
        event: Event,
        version: &str,
    ) -> Result<Event, RepositoryError>;

    /// イベントを、版が`version`のままなら削除する
    ///
    /// # Errors
    /// 版が違う、またはイベントが無い場合、`RepositoryError::Conflict`
    async fn delete_event(
        &self,
        calendar_id: &str,
        event_id: &str,
        version: &str,
    ) -> Result<(), RepositoryError>;
}

/// Google Calendar APIを直接呼び出す実装
pub(super) struct GoogleCalendarEventGateway {
    hub: CalendarHub<HttpsConnector<HttpConnector>>,
}

/// イベント一覧の絞り込み
enum ListFilter<'a> {
    /// 期間に重なるイベント
    Period {
        time_min: DateTime<Utc>,
        time_max: Option<DateTime<Utc>>,
    },
    /// 1つの繰り返しイベントとその回
    Series { ical_uid: &'a str },
}

impl GoogleCalendarEventGateway {
    /// 繰り返しイベントを回ごとに展開して取得する（本物のカレンダーの振る舞いを調べる用）
    #[cfg(test)]
    pub(super) async fn list_instances(
        &self,
        calendar_id: &str,
        series_id: &str,
    ) -> Result<Vec<Event>, RepositoryError> {
        let (_response, events) = self
            .hub
            .events()
            .instances(calendar_id, series_id)
            .show_deleted(true)
            .add_scope(SCOPE)
            .doit()
            .await
            .map_err(|e| RepositoryError::ConnectionError(format!("Calendar API error: {}", e)))?;
        Ok(events.items.unwrap_or_default())
    }

    /// サービスアカウントの鍵でGoogle Calendar APIにつなぐ
    ///
    /// # Returns
    /// ゲートウェイと、サービスアカウントのメールアドレス（アプリが作るイベントの作成者になる）
    pub(super) async fn connect(
        service_account_key: &str,
    ) -> Result<(Self, String), Box<dyn std::error::Error>> {
        let secret = yup_oauth2::read_service_account_key(service_account_key).await?;
        let service_account_email = secret.client_email.clone();

        let auth = yup_oauth2::ServiceAccountAuthenticator::builder(secret)
            .build()
            .await?;

        let connector = HttpsConnectorBuilder::new()
            .with_native_roots()?
            .https_or_http()
            .enable_http1()
            .build();

        let client = Client::builder(TokioExecutor::new()).build(connector);

        Ok((
            Self::new(CalendarHub::new(client, auth)),
            service_account_email,
        ))
    }

    fn new(hub: CalendarHub<HttpsConnector<HttpConnector>>) -> Self {
        Self { hub }
    }

    /// イベントを、繰り返しを展開せず、削除されたものも含めて取得する
    async fn list(
        &self,
        calendar_id: &str,
        filter: ListFilter<'_>,
    ) -> Result<Vec<Event>, RepositoryError> {
        let mut all_events = Vec::new();
        let mut page_token: Option<String> = None;

        // 1ページに収まらない場合、続きのページを取得しないとイベントを取りこぼす
        loop {
            let mut call = self
                .hub
                .events()
                .list(calendar_id)
                .add_scope(SCOPE)
                // 繰り返しイベントは回ごとに展開しない。回は予約が自分の日程から求める
                .single_events(false)
                // 削除された回（規則どおりでない回の一種）も読む
                .show_deleted(true)
                .max_results(MAX_RESULTS_PER_PAGE);

            match &filter {
                ListFilter::Period { time_min, time_max } => {
                    call = call.time_min(*time_min);
                    if let Some(time_max) = time_max {
                        call = call.time_max(*time_max);
                    }
                }
                ListFilter::Series { ical_uid } => call = call.i_cal_uid(ical_uid),
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

    /// 版を条件にした書き換え・削除を送る
    ///
    /// 生成されたクライアントの呼び出しにはヘッダーを足せないため、同じ認証と HTTP
    /// クライアントで`If-Match`付きのリクエストを組み立てる。保存先の版が違えば、
    /// カレンダーは 412 を返して何も変えない。
    async fn send_if_match(
        &self,
        method: hyper::Method,
        calendar_id: &str,
        event_id: &str,
        event: Option<&Event>,
        version: &str,
    ) -> Result<Option<Event>, RepositoryError> {
        use hyper::header::{AUTHORIZATION, CONTENT_TYPE, IF_MATCH};

        let token = self
            .hub
            .auth
            .get_token(&[SCOPE.as_ref()])
            .await
            .map_err(|e| RepositoryError::ConnectionError(format!("認証に失敗: {}", e)))?;

        let url = format!(
            "{CALENDAR_API_BASE}/calendars/{}/events/{}",
            encode_path_segment(calendar_id),
            encode_path_segment(event_id)
        );

        let mut builder = hyper::Request::builder()
            .method(method)
            .uri(url)
            .header(IF_MATCH, version);
        if let Some(token) = token {
            builder = builder.header(AUTHORIZATION, format!("Bearer {}", token));
        }
        let request = match event {
            Some(event) => {
                let mut value = serde_json::to_value(event).map_err(|e| {
                    RepositoryError::Unknown(format!("イベントを JSON にできません: {}", e))
                })?;
                common::remove_json_null_values(&mut value);
                let body = serde_json::to_vec(&value).map_err(|e| {
                    RepositoryError::Unknown(format!("イベントを JSON にできません: {}", e))
                })?;
                builder
                    .header(CONTENT_TYPE, "application/json")
                    .body(common::to_body(Some(body)))
            }
            None => builder.body(common::to_body::<Vec<u8>>(None)),
        }
        .map_err(|e| RepositoryError::Unknown(format!("リクエストを組み立てられません: {}", e)))?;

        let response =
            self.hub.client.request(request).await.map_err(|e| {
                RepositoryError::ConnectionError(format!("Calendar API error: {}", e))
            })?;
        let status = response.status();
        let bytes = common::to_bytes(response.into_body())
            .await
            .unwrap_or_default();

        match status.as_u16() {
            200..=299 if bytes.is_empty() => Ok(None),
            200..=299 => serde_json::from_slice(&bytes).map(Some).map_err(|e| {
                RepositoryError::Unknown(format!("Calendar API の応答を読めません: {}", e))
            }),
            // 版が違う（412）、または消えている（404・410）。読んでから書くまでの間に
            // 誰かが変えたか取り消した
            404 | 410 | 412 => Err(RepositoryError::Conflict),
            _ => Err(RepositoryError::ConnectionError(format!(
                "Calendar API error {}: {}",
                status,
                String::from_utf8_lossy(&bytes)
            ))),
        }
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
        self.list(calendar_id, ListFilter::Period { time_min, time_max })
            .await
    }

    async fn list_series(
        &self,
        calendar_id: &str,
        ical_uid: &str,
    ) -> Result<Vec<Event>, RepositoryError> {
        self.list(calendar_id, ListFilter::Series { ical_uid })
            .await
    }

    async fn get_event(
        &self,
        calendar_id: &str,
        event_id: &str,
    ) -> Result<Option<Event>, RepositoryError> {
        match self
            .hub
            .events()
            .get(calendar_id, event_id)
            .add_scope(SCOPE)
            .doit()
            .await
        {
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
            .add_scope(SCOPE)
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
        version: &str,
    ) -> Result<Event, RepositoryError> {
        self.send_if_match(
            hyper::Method::PUT,
            calendar_id,
            event_id,
            Some(&event),
            version,
        )
        .await?
        .ok_or_else(|| RepositoryError::Unknown("書き換えたイベントが返りませんでした".to_string()))
    }

    async fn delete_event(
        &self,
        calendar_id: &str,
        event_id: &str,
        version: &str,
    ) -> Result<(), RepositoryError> {
        self.send_if_match(hyper::Method::DELETE, calendar_id, event_id, None, version)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::encode_path_segment;

    #[test]
    fn a_calendar_id_keeps_its_at_sign_and_other_characters_are_escaped() {
        assert_eq!(
            encode_path_segment("abc123@group.calendar.google.com"),
            "abc123@group.calendar.google.com"
        );
        assert_eq!(encode_path_segment("a/b c#d"), "a%2Fb%20c%23d");
        assert_eq!(encode_path_segment("予"), "%E4%BA%88");
    }
}
