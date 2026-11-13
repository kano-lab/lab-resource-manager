//! 今の時刻

use chrono::{DateTime, Utc};

/// 今の時刻を教える時計
///
/// ユースケースは今の時刻をここから読む。テストでは止めた時計を渡し、時刻に依存する
/// 振る舞い（予定どおり終わった、今で終了した、閾値を過ぎた）を決まった時刻で確かめる。
/// ドメインの操作は時刻を引数で受け取るので、時計を持たない。
pub trait Clock: Send + Sync {
    /// 今の時刻
    fn now(&self) -> DateTime<Utc>;
}

/// システムの時計
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// 止めておき、テストが進める時計
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct FixedClock(std::sync::Mutex<DateTime<Utc>>);

#[cfg(test)]
impl FixedClock {
    /// `now`で止まった時計
    pub(crate) fn at(now: DateTime<Utc>) -> Self {
        Self(std::sync::Mutex::new(now))
    }

    /// 時計を`by`だけ進める
    pub(crate) fn advance(&self, by: chrono::Duration) {
        *self.0.lock().unwrap() += by;
    }
}

#[cfg(test)]
impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}
