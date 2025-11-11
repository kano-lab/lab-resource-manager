//! Slackインターフェース定数
//!
//! Slackインタラクションで使用されるアクションID、コールバックID、その他の定数

// アクションID - 予約フォーム
pub const ACTION_RESERVE_RESOURCE_TYPE: &str = "reserve_resource_type";
pub const ACTION_RESERVE_SERVER_SELECT: &str = "reserve_server_select";
pub const ACTION_RESERVE_ROOM_SELECT: &str = "reserve_room_select";
pub const ACTION_RESERVE_DEVICES: &str = "reserve_devices";
pub const ACTION_RESERVE_START_DATE: &str = "reserve_start_date";
pub const ACTION_RESERVE_START_TIME: &str = "reserve_start_time";
pub const ACTION_RESERVE_END_DATE: &str = "reserve_end_date";
pub const ACTION_RESERVE_END_TIME: &str = "reserve_end_time";
pub const ACTION_RESERVE_NOTES: &str = "reserve_notes";

// アクションID - ボタン
pub const ACTION_SHOW_DETAIL: &str = "show_detail";
pub const ACTION_EDIT_RESERVATION: &str = "edit_reservation";
pub const ACTION_CANCEL_RESERVATION: &str = "cancel_reservation";

// モーダルコールバックID
pub const CALLBACK_REGISTER_EMAIL: &str = "register_email";
pub const CALLBACK_RESERVE_SUBMIT: &str = "reserve_submit";
pub const CALLBACK_UPDATE_SUBMIT: &str = "update_submit";

// アクションID - メールアドレス登録モーダル
pub const ACTION_EMAIL_INPUT: &str = "email_input";
