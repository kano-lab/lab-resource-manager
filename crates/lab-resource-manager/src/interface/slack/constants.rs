//! Slackインターフェース定数
//!
//! Slackインタラクションで使用されるアクションID、コールバックID、その他の定数

// アクションID - 予約フォーム
/// リソースタイプ選択（GPU/部屋）のラジオボタンアクション
pub const ACTION_RESERVE_RESOURCE_TYPE: &str = "reserve_resource_type";
/// サーバー選択のセレクトメニューアクション
pub const ACTION_RESERVE_SERVER_SELECT: &str = "reserve_server_select";
/// 部屋選択のセレクトメニューアクション
pub const ACTION_RESERVE_ROOM_SELECT: &str = "reserve_room_select";
/// デバイス（GPU）選択のチェックボックスアクション
pub const ACTION_RESERVE_DEVICES: &str = "reserve_devices";
/// 予約開始日の日付ピッカーアクション
pub const ACTION_RESERVE_START_DATE: &str = "reserve_start_date";
/// 予約開始時刻のタイムピッカーアクション
pub const ACTION_RESERVE_START_TIME: &str = "reserve_start_time";
/// 予約終了日の日付ピッカーアクション
pub const ACTION_RESERVE_END_DATE: &str = "reserve_end_date";
/// 予約終了時刻のタイムピッカーアクション
pub const ACTION_RESERVE_END_TIME: &str = "reserve_end_time";
/// 備考入力のテキストエリアアクション
pub const ACTION_RESERVE_NOTES: &str = "reserve_notes";

// モーダルコールバックID
/// メールアドレス登録モーダルのコールバックID
pub const CALLBACK_REGISTER_EMAIL: &str = "register_email";
/// ユーザーリンクモーダルのコールバックID
pub const CALLBACK_LINK_USER: &str = "link_user";
/// 新規予約送信モーダルのコールバックID
pub const CALLBACK_RESERVE_SUBMIT: &str = "reserve_submit";
/// 予約更新モーダルのコールバックID
pub const CALLBACK_RESERVE_UPDATE: &str = "reserve_update";

// アクションID - メールアドレス登録モーダル
/// メールアドレス入力フィールドのアクション
pub const ACTION_EMAIL_INPUT: &str = "email_input";

// アクションID - ユーザーリンクモーダル
/// ユーザー選択フィールドのアクション
pub const ACTION_USER_SELECT: &str = "user_select";
/// リンク先メールアドレス入力フィールドのアクション
pub const ACTION_LINK_EMAIL_INPUT: &str = "link_email_input";
/// リンク対象の種類（Slackユーザー/OSユーザー名）選択のラジオボタンアクション
pub const ACTION_LINK_TARGET_TYPE: &str = "link_target_type";
/// OSユーザー名リンク時のサーバー選択のセレクトメニューアクション
pub const ACTION_LINK_SERVER_SELECT: &str = "link_server_select";
/// OSユーザー名入力フィールドのアクション
pub const ACTION_LINK_OS_USERNAME_INPUT: &str = "link_os_username_input";

// アクションID - 送信するメッセージに置くボタン
//
// ボタンを置くのは通知やDMを組み立てるinfrastructure層なので、値はそちらで定める。
pub use crate::infrastructure::slack_button_actions::*;

// アクションID - 空き状況メッセージ
/// 表示する日を切り替えるボタンのアクション（値は表示タイムゾーンでのYYYY-MM-DD）
pub const ACTION_FREE_SHOW_DAY: &str = "free_show_day";
/// 表示する日を選ぶセレクトメニューのアクション
pub const ACTION_FREE_SELECT_DAY: &str = "free_select_day";
/// 空いているリソースを選んだ状態で予約モーダルを開くボタンのアクション
pub const ACTION_FREE_RESERVE: &str = "free_reserve";
