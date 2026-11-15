//! 送信するSlackメッセージに置くボタンのアクションID
//!
//! 通知やDMを組み立てるこの層がボタンを置き、押されたボタンは`interface::slack`が受け取る。
//! 両者が同じ値を使わなければボタンは動かないため、ボタンを置く側であるここで定める。
//!
//! 値は投稿済みのメッセージのボタンにも埋め込まれている。値を変えると、すでにチャンネルや
//! DMにあるボタンを押しても動かなくなる。定数名はユビキタス言語に合わせてあり、
//! 値とは一致しないものがあるが、値は変えない。

// 通知に付く予約枠のボタン
/// 予約枠の期間・備考を変更するボタンのアクション
pub const ACTION_UPDATE_RESERVED_SLOT: &str = "edit_reservation";
/// 予約枠を取り消すボタンのアクション
pub const ACTION_CANCEL_RESERVED_SLOT: &str = "cancel_reservation";
/// 予約枠を今の時点で終了するボタンのアクション
pub const ACTION_END_RESERVED_SLOT_EARLY: &str = "release_reservation_early";

// 使われていない予約枠のお知らせDM
/// 使われていない予約枠を今の時点で終了するボタンのアクション
pub const ACTION_IDLE_END_EARLY: &str = "idle_release_reservation";
/// 使われていない予約枠をこれから使うと答えるボタンのアクション
pub const ACTION_IDLE_KEEP: &str = "idle_keep_reservation";
/// 使われていない予約枠を取り消すボタンのアクション
pub const ACTION_IDLE_CANCEL: &str = "idle_cancel_reservation";

// 事後予約提案DM
/// 事後予約提案の受諾ボタンのアクション
pub const ACTION_ACCEPT_RESERVATION_PROPOSAL: &str = "accept_reservation_proposal";
