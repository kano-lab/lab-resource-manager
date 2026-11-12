//! テンプレートレンダラー
//!
//! 通知メッセージのテンプレートとプレースホルダー置換を処理します。

use crate::domain::aggregates::reservation::value_objects::Resource;
use crate::domain::aggregates::reservation::{
    Reservation, ReservationEvent, ReservedSlot, Schedule,
};
use crate::domain::common::TimePeriod;
use crate::domain::read_models::ReservedSlotView;
use crate::infrastructure::config::{FormatConfig, TemplateConfig};
use crate::infrastructure::notifier::formatter::{
    format_recurrence, format_resources_styled, format_time_styled,
};

/// プレースホルダー定義
pub mod placeholders {
    /// ユーザー表示名/メンション
    pub const USER: &str = "{user}";
    /// リソース情報
    pub const RESOURCE: &str = "{resource}";
    /// 時刻情報
    pub const TIME: &str = "{time}";
    /// 備考
    pub const NOTES: &str = "{notes}";
    /// リソースラベル（💻 予約GPU等）
    pub const RESOURCE_LABEL: &str = "{resource_label}";
    /// 繰り返しの間隔（定期予約のテンプレートでのみ値を持つ）
    pub const RECURRENCE: &str = "{recurrence}";
}

/// デフォルトテンプレート（現在のハードコード値と同等）
pub mod defaults {
    /// 予約作成時のデフォルトテンプレート
    pub const CREATED: &str =
        "🔔 新規予約\n👤 {user}\n\n📅 期間\n{time}\n\n{resource_label}\n{resource}{notes}";
    /// 予約更新時のデフォルトテンプレート
    pub const UPDATED: &str =
        "🔄 予約更新\n👤 {user}\n\n📅 期間\n{time}\n\n{resource_label}\n{resource}{notes}";
    /// 予約取り消し時のデフォルトテンプレート
    pub const CANCELLED: &str =
        "🗑️ 予約取り消し\n👤 {user}\n\n📅 期間\n{time}\n\n{resource_label}\n{resource}{notes}";
    /// 予約競合時のデフォルトテンプレート（1件の競合）
    pub const CONFLICT: &str = "⚠️ 予約が重複しています\n👤 予約者\n{user}\n\n📅 競合する期間\n{time}\n\n{resource_label}\n{resource}{notes}";
    /// 定期予約作成時のデフォルトテンプレート
    pub const RECURRING_CREATED: &str = "🔁 新規定期予約\n👤 {user}\n\n🗓️ 繰り返し\n{recurrence}\n\n📅 時間帯（初回）\n{time}\n\n{resource_label}\n{resource}{notes}";
    /// 定期予約変更時のデフォルトテンプレート
    pub const RECURRING_UPDATED: &str = "🔁 定期予約変更\n👤 {user}\n\n🗓️ 繰り返し\n{recurrence}\n\n📅 時間帯（初回）\n{time}\n\n{resource_label}\n{resource}{notes}";
    /// 定期予約取り消し時のデフォルトテンプレート
    pub const RECURRING_CANCELLED: &str = "🗑️ 定期予約取り消し\n👤 {user}\n\n🗓️ 繰り返し\n{recurrence}\n\n📅 時間帯（初回）\n{time}\n\n{resource_label}\n{resource}{notes}";
    /// 定期予約の予約枠が1つだけ変更されたときのデフォルトテンプレート
    pub const SLOT_UPDATED: &str = "🔄 定期予約の予約枠を変更\n👤 {user}\n\n📅 期間\n{time}\n\n{resource_label}\n{resource}{notes}";
    /// 定期予約の予約枠が1つだけ取り消されたときのデフォルトテンプレート
    pub const SLOT_CANCELLED: &str = "🗑️ 定期予約の予約枠を取り消し\n👤 {user}\n\n📅 期間\n{time}\n\n{resource_label}\n{resource}{notes}";
    /// 複数競合時の各項目テンプレート（見出しなし）
    pub const CONFLICT_ITEM: &str = "👤 {user}\n\n📅 {time}\n\n{resource_label}\n{resource}{notes}";
}

/// 出来事の種類に応じたテンプレートでメッセージをレンダリング
///
/// 単発予約の今で終了は、予約枠の時間が変わったこととして`updated`のテンプレートで、
/// 定期予約の予約枠1つの今で終了は`slot_updated`のテンプレートで伝える。
pub fn render_event(
    renderer: &TemplateRenderer<'_>,
    event: &ReservationEvent,
    user_display: &str,
) -> String {
    match event {
        ReservationEvent::Reserved(reservation) if reservation.is_recurring() => {
            renderer.render_recurring_created(reservation, user_display)
        }
        ReservationEvent::Reserved(reservation) => {
            renderer.render_created(reservation, user_display)
        }
        ReservationEvent::Changed(reservation) | ReservationEvent::EndedEarly(reservation)
            if reservation.is_recurring() =>
        {
            renderer.render_recurring_updated(reservation, user_display)
        }
        ReservationEvent::Changed(reservation) | ReservationEvent::EndedEarly(reservation) => {
            renderer.render_updated(reservation, user_display)
        }
        ReservationEvent::Cancelled(reservation) if reservation.is_recurring() => {
            renderer.render_recurring_cancelled(reservation, user_display)
        }
        ReservationEvent::Cancelled(reservation) => {
            renderer.render_cancelled(reservation, user_display)
        }
        ReservationEvent::SlotChanged { reservation, slot }
        | ReservationEvent::SlotEndedEarly { reservation, slot } => {
            renderer.render_slot_updated(reservation, slot, user_display)
        }
        ReservationEvent::SlotCancelled { reservation, slot } => {
            renderer.render_slot_cancelled(reservation, slot, user_display)
        }
    }
}

/// テンプレートレンダラー
pub struct TemplateRenderer<'a> {
    templates: &'a TemplateConfig,
    format: &'a FormatConfig,
    timezone: Option<&'a str>,
}

impl<'a> TemplateRenderer<'a> {
    /// 新しいレンダラーを作成
    pub fn new(
        templates: &'a TemplateConfig,
        format: &'a FormatConfig,
        timezone: Option<&'a str>,
    ) -> Self {
        Self {
            templates,
            format,
            timezone,
        }
    }

    /// 単発予約の作成メッセージをレンダリング
    pub fn render_created(&self, reservation: &Reservation, user_display: &str) -> String {
        let template = self
            .templates
            .created
            .as_deref()
            .unwrap_or(defaults::CREATED);
        self.render_one_time(template, reservation, user_display)
    }

    /// 単発予約の変更メッセージをレンダリング
    pub fn render_updated(&self, reservation: &Reservation, user_display: &str) -> String {
        let template = self
            .templates
            .updated
            .as_deref()
            .unwrap_or(defaults::UPDATED);
        self.render_one_time(template, reservation, user_display)
    }

    /// 単発予約の取り消しメッセージをレンダリング
    pub fn render_cancelled(&self, reservation: &Reservation, user_display: &str) -> String {
        let template = self
            .templates
            .cancelled
            .as_deref()
            .unwrap_or(defaults::CANCELLED);
        self.render_one_time(template, reservation, user_display)
    }

    fn render_one_time(
        &self,
        template: &str,
        reservation: &Reservation,
        user_display: &str,
    ) -> String {
        let time_period = match reservation.schedule() {
            Schedule::OneTime(time_period) => time_period,
            Schedule::Recurring(recurring) => recurring.first(),
        };
        self.render(
            template,
            reservation.resources(),
            time_period,
            reservation.notes().map(String::as_str),
            user_display,
            None,
        )
    }

    /// 定期予約の予約枠1つの変更メッセージをレンダリング
    pub fn render_slot_updated(
        &self,
        reservation: &Reservation,
        slot: &ReservedSlot,
        user_display: &str,
    ) -> String {
        let template = self
            .templates
            .slot_updated
            .as_deref()
            .unwrap_or(defaults::SLOT_UPDATED);
        self.render_slot(template, reservation, slot, user_display)
    }

    /// 定期予約の予約枠1つの取り消しメッセージをレンダリング
    pub fn render_slot_cancelled(
        &self,
        reservation: &Reservation,
        slot: &ReservedSlot,
        user_display: &str,
    ) -> String {
        let template = self
            .templates
            .slot_cancelled
            .as_deref()
            .unwrap_or(defaults::SLOT_CANCELLED);
        self.render_slot(template, reservation, slot, user_display)
    }

    fn render_slot(
        &self,
        template: &str,
        reservation: &Reservation,
        slot: &ReservedSlot,
        user_display: &str,
    ) -> String {
        self.render(
            template,
            reservation.resources(),
            slot.time_period(),
            slot.notes().map(String::as_str),
            user_display,
            None,
        )
    }

    /// 定期予約作成メッセージをレンダリング
    pub fn render_recurring_created(
        &self,
        reservation: &Reservation,
        user_display: &str,
    ) -> String {
        let template = self
            .templates
            .recurring_created
            .as_deref()
            .unwrap_or(defaults::RECURRING_CREATED);
        self.render_recurring(template, reservation, user_display)
    }

    /// 定期予約変更メッセージをレンダリング
    pub fn render_recurring_updated(
        &self,
        reservation: &Reservation,
        user_display: &str,
    ) -> String {
        let template = self
            .templates
            .recurring_updated
            .as_deref()
            .unwrap_or(defaults::RECURRING_UPDATED);
        self.render_recurring(template, reservation, user_display)
    }

    /// 定期予約取り消しメッセージをレンダリング
    pub fn render_recurring_cancelled(
        &self,
        reservation: &Reservation,
        user_display: &str,
    ) -> String {
        let template = self
            .templates
            .recurring_cancelled
            .as_deref()
            .unwrap_or(defaults::RECURRING_CANCELLED);
        self.render_recurring(template, reservation, user_display)
    }

    fn render_recurring(
        &self,
        template: &str,
        reservation: &Reservation,
        user_display: &str,
    ) -> String {
        let (first_slot, recurrence) = match reservation.schedule() {
            Schedule::Recurring(recurring) => (
                recurring.first(),
                Some(format_recurrence(
                    recurring.rule(),
                    self.timezone,
                    self.format.date_format,
                )),
            ),
            Schedule::OneTime(time_period) => (time_period, None),
        };
        self.render(
            template,
            reservation.resources(),
            first_slot,
            reservation.notes().map(String::as_str),
            user_display,
            recurrence.as_deref(),
        )
    }

    /// 予約競合メッセージをレンダリング
    ///
    /// `existing_slot`は今回の予約要求と競合した既存の予約枠。
    /// `conflicting_resource`は競合したリソース単体（要求リソースのうち衝突した1件）。
    pub fn render_conflict(
        &self,
        conflicting_resources: &[Resource],
        existing_slot: &ReservedSlotView,
        user_display: &str,
    ) -> String {
        let template = self
            .templates
            .conflict
            .as_deref()
            .unwrap_or(defaults::CONFLICT);
        self.render(
            template,
            conflicting_resources,
            existing_slot.time_period(),
            existing_slot.notes().map(String::as_str),
            user_display,
            None,
        )
    }

    /// 複数競合時の各項目をレンダリング（見出しなし）
    ///
    /// 複数の予約と競合する場合に、1件ずつリスト表示するために使う。
    /// 最初の見出しと件数行は `conflict_message` が別途追加する。
    pub fn render_conflict_item(
        &self,
        conflicting_resources: &[Resource],
        existing_slot: &ReservedSlotView,
        user_display: &str,
    ) -> String {
        let template = self
            .templates
            .conflict_item
            .as_deref()
            .unwrap_or(defaults::CONFLICT_ITEM);
        self.render(
            template,
            conflicting_resources,
            existing_slot.time_period(),
            existing_slot.notes().map(String::as_str),
            user_display,
            None,
        )
    }

    /// テンプレートをレンダリング（シングルパス方式）
    ///
    /// チェーン式の`replace`だと置換後の値にプレースホルダーが含まれる場合に
    /// 誤置換が発生する可能性があるため、シングルパスで処理する。
    fn render(
        &self,
        template: &str,
        resources: &[Resource],
        time_period: &TimePeriod,
        notes: Option<&str>,
        user_display: &str,
        recurrence: Option<&str>,
    ) -> String {
        let resources_formatted = format_resources_styled(resources, self.format.resource_style);

        let time_formatted = format_time_styled(
            time_period,
            self.timezone,
            self.format.time_style,
            self.format.date_format,
        );

        let notes_formatted = notes
            .filter(|n| !n.is_empty())
            .map(|n| format!("\n\n📝 備考\n{}", n))
            .unwrap_or_default();

        let resource_label = Self::get_resource_label(resources);

        // シングルパスでテンプレートを走査し、プレースホルダーのみ置換する
        // プレースホルダーはすべてASCIIなので .len() で文字数を取得可能
        let mut result = String::with_capacity(template.len() + resources_formatted.len() * 2);
        let mut chars = template.char_indices().peekable();

        while let Some((i, ch)) = chars.next() {
            let rest = &template[i..];

            // 長いプレースホルダーから順にチェック（{resource_label}と{resource}の順序に注意）
            if rest.starts_with(placeholders::RESOURCE_LABEL) {
                result.push_str(resource_label);
                // プレースホルダー分の文字をスキップ（先頭1文字は既に読み込み済み）
                for _ in 1..placeholders::RESOURCE_LABEL.len() {
                    chars.next();
                }
            } else if rest.starts_with(placeholders::RECURRENCE) {
                // 単発の予約には繰り返しが無いため、空として扱う
                result.push_str(recurrence.unwrap_or_default());
                for _ in 1..placeholders::RECURRENCE.len() {
                    chars.next();
                }
            } else if rest.starts_with(placeholders::RESOURCE) {
                result.push_str(&resources_formatted);
                for _ in 1..placeholders::RESOURCE.len() {
                    chars.next();
                }
            } else if rest.starts_with(placeholders::USER) {
                result.push_str(user_display);
                for _ in 1..placeholders::USER.len() {
                    chars.next();
                }
            } else if rest.starts_with(placeholders::TIME) {
                result.push_str(&time_formatted);
                for _ in 1..placeholders::TIME.len() {
                    chars.next();
                }
            } else if rest.starts_with(placeholders::NOTES) {
                result.push_str(&notes_formatted);
                for _ in 1..placeholders::NOTES.len() {
                    chars.next();
                }
            } else {
                result.push(ch);
            }
        }

        result
    }

    /// リソースタイプに応じたラベルを取得
    fn get_resource_label(resources: &[Resource]) -> &'static str {
        if resources.is_empty() {
            return "📦 予約リソース";
        }

        let has_gpu = resources.iter().any(|r| matches!(r, Resource::Gpu(_)));
        let has_room = resources.iter().any(|r| matches!(r, Resource::Room { .. }));

        match (has_gpu, has_room) {
            (true, false) => "💻 予約GPU",
            (false, true) => "🏢 予約部屋",
            _ => "📦 予約リソース",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::reservation::value_objects::Gpu;
    use crate::domain::common::EmailAddress;
    use crate::infrastructure::config::{DateFormat, ResourceStyle, TimeStyle};
    use chrono::{TimeZone, Utc};

    fn create_test_slot() -> ReservedSlotView {
        let start = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        let period = TimePeriod::new(start, end).unwrap();

        let resources = vec![
            Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string())),
            Resource::Gpu(Gpu::new("Thalys".to_string(), 1, "A100".to_string())),
        ];

        ReservedSlotView::of_new_one_time_reservation(
            EmailAddress::new("test@example.com".to_string()).unwrap(),
            period,
            resources,
            Some("テスト用予約".to_string()),
        )
        .unwrap()
    }

    /// 予約枠の写しと同じ内容の単発予約
    fn reservation_of(slot: &ReservedSlotView) -> Reservation {
        Reservation::reconstruct(
            slot.reservation_id().clone(),
            slot.holder_email().clone(),
            slot.resources().to_vec(),
            slot.notes().cloned(),
            Schedule::OneTime(slot.time_period().clone()),
            None,
        )
        .unwrap()
    }

    #[test]
    fn test_render_with_default_template() {
        let templates = TemplateConfig::default();
        let format = FormatConfig::default();

        let renderer = TemplateRenderer::new(&templates, &format, Some("Asia/Tokyo"));
        let slot = create_test_slot();

        let result = renderer.render_created(&reservation_of(&slot), "<@U12345>");

        assert!(result.contains("🔔 新規予約"));
        assert!(result.contains("<@U12345>"));
        assert!(result.contains("Thalys"));
        assert!(result.contains("📝 備考"));
        assert!(result.contains("テスト用予約"));
    }

    #[test]
    fn test_render_with_custom_template() {
        let templates = TemplateConfig {
            created: Some("{user}が{resource}を{time}使います".to_string()),
            updated: None,
            cancelled: None,
            conflict: None,
            conflict_item: None,
            ..Default::default()
        };
        let format = FormatConfig {
            resource_style: ResourceStyle::Compact,
            time_style: TimeStyle::Smart,
            date_format: DateFormat::Md,
        };

        let renderer = TemplateRenderer::new(&templates, &format, Some("Asia/Tokyo"));
        let slot = create_test_slot();

        let result = renderer.render_created(&reservation_of(&slot), "田中太郎");

        assert!(result.contains("田中太郎が"));
        assert!(result.contains("Thalys 0,1")); // compact format
        assert!(result.contains("1/15")); // smart format with md date
        assert!(result.contains("使います"));
        // notes not in template, so not included
        assert!(!result.contains("備考"));
    }

    #[test]
    fn test_render_with_custom_format_only() {
        let templates = TemplateConfig::default();
        let format = FormatConfig {
            resource_style: ResourceStyle::ServerOnly,
            time_style: TimeStyle::Relative,
            date_format: DateFormat::MdJapanese,
        };

        let renderer = TemplateRenderer::new(&templates, &format, Some("Asia/Tokyo"));
        let slot = create_test_slot();

        let result = renderer.render_updated(&reservation_of(&slot), "user@example.com");

        assert!(result.contains("🔄 予約更新"));
        assert!(result.contains("Thalys")); // server only
        assert!(!result.contains("GPU:0")); // no device number in server_only
    }

    #[test]
    fn test_get_resource_label_gpu() {
        let resources = vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            0,
            "A100".to_string(),
        ))];
        assert_eq!(
            TemplateRenderer::get_resource_label(&resources),
            "💻 予約GPU"
        );
    }

    #[test]
    fn test_get_resource_label_room() {
        let resources = vec![Resource::Room {
            name: "会議室A".to_string(),
        }];
        assert_eq!(
            TemplateRenderer::get_resource_label(&resources),
            "🏢 予約部屋"
        );
    }

    #[test]
    fn test_get_resource_label_mixed() {
        let resources = vec![
            Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string())),
            Resource::Room {
                name: "会議室A".to_string(),
            },
        ];
        assert_eq!(
            TemplateRenderer::get_resource_label(&resources),
            "📦 予約リソース"
        );
    }

    #[test]
    fn test_render_conflict_with_default_template() {
        let templates = TemplateConfig::default();
        let format = FormatConfig::default();

        let renderer = TemplateRenderer::new(&templates, &format, Some("Asia/Tokyo"));
        let existing_slot = create_test_slot();
        let conflicting_resource =
            Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string()));

        let result = renderer.render_conflict(
            std::slice::from_ref(&conflicting_resource),
            &existing_slot,
            "<@U67890>",
        );

        assert!(result.contains("⚠️ 予約が重複しています"));
        assert!(result.contains("<@U67890>"));
        assert!(result.contains("Thalys"));
        // 競合リソースは1件のみなので、既存予約の2台目(GPU:1)は含まれない
        assert!(!result.contains("GPU:1"));
    }

    #[test]
    fn test_render_conflict_with_custom_template() {
        let templates = TemplateConfig {
            created: None,
            updated: None,
            cancelled: None,
            conflict: Some("{user}が既に{resource}を{time}で予約済みです".to_string()),
            conflict_item: None,
            ..Default::default()
        };
        let format = FormatConfig::default();

        let renderer = TemplateRenderer::new(&templates, &format, Some("Asia/Tokyo"));
        let existing_slot = create_test_slot();
        let conflicting_resource =
            Resource::Gpu(Gpu::new("Thalys".to_string(), 1, "A100".to_string()));

        let result = renderer.render_conflict(
            std::slice::from_ref(&conflicting_resource),
            &existing_slot,
            "田中太郎",
        );

        assert!(result.contains("田中太郎が既に"));
        assert!(result.contains("GPU:1"));
        assert!(result.contains("予約済みです"));
    }

    #[test]
    fn test_render_without_notes() {
        let templates = TemplateConfig::default();
        let format = FormatConfig::default();

        let renderer = TemplateRenderer::new(&templates, &format, Some("Asia/Tokyo"));

        let start = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        let period = TimePeriod::new(start, end).unwrap();
        let resources = vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            0,
            "A100".to_string(),
        ))];

        let slot = ReservedSlotView::of_new_one_time_reservation(
            EmailAddress::new("test@example.com".to_string()).unwrap(),
            period,
            resources,
            None, // no notes
        )
        .unwrap();

        let result = renderer.render_created(&reservation_of(&slot), "user");

        assert!(!result.contains("📝 備考"));
    }

    #[test]
    fn test_render_conflict_item_with_default_template() {
        let templates = TemplateConfig::default();
        let format = FormatConfig::default();

        let renderer = TemplateRenderer::new(&templates, &format, Some("Asia/Tokyo"));
        let existing_slot = create_test_slot();
        let conflicting_resource =
            Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string()));

        let result = renderer.render_conflict_item(
            std::slice::from_ref(&conflicting_resource),
            &existing_slot,
            "<@U67890>",
        );

        // conflict_itemテンプレートは見出し（⚠️）を含まない
        assert!(!result.contains("⚠️ 予約が重複しています"));
        assert!(result.contains("👤 <@U67890>"));
        assert!(result.contains("Thalys"));
    }

    #[test]
    fn test_render_conflict_item_with_custom_template() {
        let templates = TemplateConfig {
            created: None,
            updated: None,
            cancelled: None,
            conflict: None,
            conflict_item: Some("{user} : {time} に {resource}".to_string()),
            ..Default::default()
        };
        let format = FormatConfig::default();

        let renderer = TemplateRenderer::new(&templates, &format, Some("Asia/Tokyo"));
        let existing_slot = create_test_slot();
        let conflicting_resource =
            Resource::Gpu(Gpu::new("Thalys".to_string(), 1, "A100".to_string()));

        let result = renderer.render_conflict_item(
            std::slice::from_ref(&conflicting_resource),
            &existing_slot,
            "田中太郎",
        );

        assert!(result.contains("田中太郎 : "));
        assert!(result.contains("に"));
        assert!(result.contains("GPU:1"));
    }

    /// テスト用の予約枠と同じ時間帯・リソースで、毎週月曜に繰り返す定期予約
    fn create_test_recurring_reservation() -> Reservation {
        use crate::domain::aggregates::reservation::{
            RecurrenceRule, RecurringSchedule, ReservationId,
        };

        let slot = create_test_slot();
        Reservation::reconstruct(
            ReservationId::from_string("weekly".to_string()),
            slot.holder_email().clone(),
            slot.resources().to_vec(),
            slot.notes().cloned(),
            Schedule::Recurring(RecurringSchedule::new(
                slot.time_period().clone(),
                RecurrenceRule::parse(vec!["RRULE:FREQ=WEEKLY;BYDAY=MO".to_string()]).unwrap(),
                chrono_tz::Asia::Tokyo,
                Default::default(),
            )),
            None,
        )
        .unwrap()
    }

    #[test]
    fn a_recurring_reservation_is_announced_with_its_interval() {
        let templates = TemplateConfig::default();
        let format = FormatConfig::default();
        let renderer = TemplateRenderer::new(&templates, &format, Some("Asia/Tokyo"));
        let reservation = create_test_recurring_reservation();

        let created = renderer.render_recurring_created(&reservation, "<@U12345>");

        assert!(created.contains("🔁 新規定期予約"), "{created}");
        assert!(created.contains("毎週 月"), "{created}");
        assert!(created.contains("<@U12345>"));
        assert!(created.contains("Thalys"));
        assert!(created.contains("テスト用予約"));

        assert!(
            renderer
                .render_recurring_updated(&reservation, "<@U12345>")
                .contains("🔁 定期予約変更")
        );
        assert!(
            renderer
                .render_recurring_cancelled(&reservation, "<@U12345>")
                .contains("🗑️ 定期予約取り消し")
        );
    }

    #[test]
    fn a_custom_recurring_template_can_place_the_interval() {
        let templates = TemplateConfig {
            recurring_created: Some("{user}: {recurrence} / {resource}".to_string()),
            ..Default::default()
        };
        let format = FormatConfig {
            resource_style: ResourceStyle::Compact,
            ..Default::default()
        };
        let renderer = TemplateRenderer::new(&templates, &format, Some("Asia/Tokyo"));

        let result =
            renderer.render_recurring_created(&create_test_recurring_reservation(), "<@U12345>");

        assert_eq!(result, "<@U12345>: 毎週 月 / Thalys 0,1");
    }

    #[test]
    fn the_recurrence_placeholder_is_empty_for_a_single_reservation() {
        let templates = TemplateConfig {
            created: Some("{user}[{recurrence}]".to_string()),
            ..Default::default()
        };
        let format = FormatConfig::default();
        let renderer = TemplateRenderer::new(&templates, &format, Some("Asia/Tokyo"));

        let result = renderer.render_created(&reservation_of(&create_test_slot()), "<@U12345>");

        assert_eq!(result, "<@U12345>[]");
    }
}
