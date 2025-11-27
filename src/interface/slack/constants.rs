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

// アクションID - ボタン
/// 予約詳細表示ボタンアクション
pub const ACTION_SHOW_DETAIL: &str = "show_detail";
/// 予約編集ボタンアクション
pub const ACTION_EDIT_RESERVATION: &str = "edit_reservation";
/// 予約キャンセルボタンアクション
pub const ACTION_CANCEL_RESERVATION: &str = "cancel_reservation";

// モーダルコールバックID
/// メールアドレス登録モーダルのコールバックID
pub const CALLBACK_REGISTER_EMAIL: &str = "register_email";
/// ユーザーリンクモーダルのコールバックID
pub const CALLBACK_LINK_USER: &str = "link_user";
/// 新規予約送信モーダルのコールバックID
pub const CALLBACK_RESERVE_SUBMIT: &str = "reserve_submit";

// アクションID - メールアドレス登録モーダル
/// メールアドレス入力フィールドのアクション
pub const ACTION_EMAIL_INPUT: &str = "email_input";

// アクションID - ユーザーリンクモーダル
/// ユーザー選択フィールドのアクション
pub const ACTION_USER_SELECT: &str = "user_select";
/// リンク先メールアドレス入力フィールドのアクション
pub const ACTION_LINK_EMAIL_INPUT: &str = "link_email_input";
