//! Slackインターフェース定数
//!
//! Slackインタラクションで使用されるアクションID、コールバックID、その他の定数

// モーダルコールバックID
/// メールアドレス登録モーダルのコールバックID
pub const CALLBACK_REGISTER_EMAIL: &str = "register_email";
/// ユーザーリンクモーダルのコールバックID
pub const CALLBACK_LINK_USER: &str = "link_user";
/// リソース予約モーダルのコールバックID
pub const CALLBACK_RESERVE: &str = "reserve";

// アクションID - メールアドレス登録モーダル
/// メールアドレス入力フィールドのアクション
pub const ACTION_EMAIL_INPUT: &str = "email_input";

// アクションID - ユーザーリンクモーダル
/// ユーザー選択フィールドのアクション
pub const ACTION_USER_SELECT: &str = "user_select";
/// リンク先メールアドレス入力フィールドのアクション
pub const ACTION_LINK_EMAIL_INPUT: &str = "link_email_input";

// アクションID - リソース予約モーダル
/// リソースタイプ選択フィールドのアクション
pub const ACTION_RESOURCE_TYPE: &str = "resource_type";
/// GPUサーバー選択フィールドのアクション
pub const ACTION_GPU_SERVER: &str = "gpu_server";
/// GPUデバイス番号入力フィールドのアクション
pub const ACTION_GPU_DEVICE_NUMBER: &str = "gpu_device_number";
/// GPUモデル入力フィールドのアクション
pub const ACTION_GPU_MODEL: &str = "gpu_model";
/// 部屋名入力フィールドのアクション
pub const ACTION_ROOM_NAME: &str = "room_name";
/// 開始時刻入力フィールドのアクション
pub const ACTION_START_TIME: &str = "start_time";
/// 終了時刻入力フィールドのアクション
pub const ACTION_END_TIME: &str = "end_time";
/// 備考入力フィールドのアクション
pub const ACTION_NOTES: &str = "notes";
