//! 予約が予約者本人に使われているかの見立て

use crate::domain::aggregates::identity_link::value_objects::ExternalIdentity;
use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource};
use crate::domain::ports::resource_usage_observer::{ObservationSnapshot, ObservedUsage};

/// 予約者が押さえたまま計算していないGPUたち
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpusAtRest {
    at_rest: Vec<Gpu>,
    observed_count: usize,
    peak_utilization_percent: u32,
    used_memory_mib: Option<u64>,
}

impl GpusAtRest {
    /// 休んでいるGPUたちの姿を作る
    ///
    /// # Arguments
    /// * `at_rest` - 計算が走っていないGPU
    /// * `observed_count` - 計算しているかを問えたGPUの数
    /// * `peak_utilization_percent` - 休んでいるGPUのうち最も高かった稼働率
    /// * `used_memory_mib` - 休んでいるGPUで予約者が確保しているメモリ量の合計
    pub fn new(
        at_rest: Vec<Gpu>,
        observed_count: usize,
        peak_utilization_percent: u32,
        used_memory_mib: Option<u64>,
    ) -> Self {
        Self {
            at_rest,
            observed_count,
            peak_utilization_percent,
            used_memory_mib,
        }
    }

    /// 計算が走っていないGPU（デバイス番号順）
    pub fn at_rest(&self) -> &[Gpu] {
        &self.at_rest
    }

    /// 計算しているかを問えたGPUの数
    ///
    /// 予約が押さえている数とは限らない。稼働率を報告しないGPUは数に入らない。
    pub fn observed_count(&self) -> usize {
        self.observed_count
    }

    /// 問えたGPU全部が休んでいるのか、一部なのか
    pub fn is_every_observed_gpu(&self) -> bool {
        self.at_rest.len() == self.observed_count
    }

    /// 休んでいるGPUのうち、最も高かった稼働率
    pub fn peak_utilization_percent(&self) -> u32 {
        self.peak_utilization_percent
    }

    /// 休んでいるGPUで予約者が確保しているメモリ量の合計（MiB）
    ///
    /// `None`は「確保していない」ではなく「どれだけ確保しているかを問えない」を意味する。
    pub fn used_memory_mib(&self) -> Option<u64> {
        self.used_memory_mib
    }
}

/// 予約が予約者本人に使われているかの見立て
///
/// 「使われている」と「使われていない」の二分では、押さえたまま計算していない予約を
/// 言い表せない。予約が塞いでいる時間を他の人に開けられるかどうかは、プロセスの有無
/// ではなく計算が走っているかで決まる。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReservationActivity {
    /// 押さえているGPUのどれでも計算が走っている
    InUse,
    /// 押さえているGPUの全部または一部で、計算が走っていない
    HeldWithoutComputing(GpusAtRest),
    /// 予約者本人のプロセスがひとつも観測できない
    Absent,
    /// 使われているかを問えない（観測できないサーバー、OSユーザー未リンク、部屋の予約）
    Undecidable,
}

/// 観測結果から、予約が予約者本人に使われているかを見立てる
///
/// 判定できるのは、予約が押さえるGPUをすべて観測できていて、予約者のOSユーザー名も
/// 分かっている場合に限る。その手前で足りないものがあるときは呼び出し側が
/// [`ReservationActivity::Undecidable`]を選ぶ。この関数は観測できている事実だけを読む。
///
/// 押さえているGPUは1台ずつ見る。8枚のうち1枚で計算が走っていることは、
/// 残りの7枚が使われていることを意味しない。
///
/// # Arguments
/// * `reserved` - 予約が押さえているリソース
/// * `owner_identities` - 予約者のOSユーザーとしての識別子（サーバーごと）
/// * `snapshot` - 観測結果
/// * `computing_utilization_percent` - これ以上の稼働率が出ていれば計算が走っているとみなす
pub fn judge_reservation_activity(
    reserved: &[Resource],
    owner_identities: &[ExternalIdentity],
    snapshot: &ObservationSnapshot,
    computing_utilization_percent: u32,
) -> ReservationActivity {
    let occupied = gpus_the_owner_occupies(reserved, owner_identities, snapshot);

    if occupied.is_empty() {
        return ReservationActivity::Absent;
    }

    let observed: Vec<(&Gpu, u32)> = occupied
        .iter()
        .filter_map(|gpu| {
            snapshot
                .gpu_activity_of(gpu)
                .map(|activity| (*gpu, activity.peak_utilization_percent()))
        })
        .collect();

    // 稼働率を報告しない観測手段のもとでは、プロセスが乗っていることを利用の証と読むほかない
    if observed.is_empty() {
        return ReservationActivity::InUse;
    }

    let at_rest: Vec<(&Gpu, u32)> = observed
        .iter()
        .filter(|(_, peak)| *peak < computing_utilization_percent)
        .copied()
        .collect();

    if at_rest.is_empty() {
        return ReservationActivity::InUse;
    }

    ReservationActivity::HeldWithoutComputing(GpusAtRest::new(
        at_rest.iter().map(|(gpu, _)| (*gpu).clone()).collect(),
        observed.len(),
        at_rest.iter().map(|(_, peak)| *peak).max().unwrap_or(0),
        memory_held_on(
            at_rest.iter().map(|(gpu, _)| *gpu),
            owner_identities,
            snapshot,
        ),
    ))
}

/// 予約が押さえているGPUのうち、予約者本人のプロセスが乗っているもの（デバイス番号順）
///
/// 予約者以外の利用は無断使用として別に扱われるものであり、予約が使われていることには
/// ならない。
fn gpus_the_owner_occupies<'a>(
    reserved: &'a [Resource],
    owner_identities: &[ExternalIdentity],
    snapshot: &ObservationSnapshot,
) -> Vec<&'a Gpu> {
    let mut occupied: Vec<&Gpu> = reserved
        .iter()
        .filter_map(|resource| match resource {
            Resource::Gpu(gpu) => Some(gpu),
            Resource::Room { .. } => None,
        })
        .filter(|gpu| {
            owner_usages_on(gpu, owner_identities, snapshot)
                .next()
                .is_some()
        })
        .collect();

    occupied.sort_by_key(|gpu| gpu.device_number());
    occupied
}

/// このGPUに乗っている、予約者本人の利用
fn owner_usages_on<'a>(
    gpu: &Gpu,
    owner_identities: &'a [ExternalIdentity],
    snapshot: &'a ObservationSnapshot,
) -> impl Iterator<Item = &'a ObservedUsage> + 'a {
    let reserved = Resource::Gpu(gpu.clone());

    snapshot.usages().iter().filter(move |observed| {
        owner_identities.contains(observed.external_identity())
            && reserved.conflicts_with(observed.resource())
    })
}

/// これらのGPUで予約者が確保しているメモリ量の合計
///
/// ひとつでも読み出せない利用があれば、合計そのものを問えないものとして扱う。
/// 読めた分だけを足した数を確保量として伝えると、実際より少なく見える。
fn memory_held_on<'a>(
    gpus: impl Iterator<Item = &'a Gpu>,
    owner_identities: &[ExternalIdentity],
    snapshot: &ObservationSnapshot,
) -> Option<u64> {
    let mut total = 0_u64;

    for gpu in gpus {
        for usage in owner_usages_on(gpu, owner_identities, snapshot) {
            total += usage.used_memory_mib()?;
        }
    }

    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::identity_link::value_objects::ExternalSystem;
    use crate::domain::ports::resource_usage_observer::{GpuActivity, ServerObservation};
    use chrono::Utc;
    use std::collections::HashMap;

    const SERVER: &str = "Thalys";
    const COMPUTING: u32 = 5;

    fn gpu(device_number: u32) -> Gpu {
        Gpu::new(SERVER.to_string(), device_number, "A100".to_string())
    }

    fn owner_identity() -> ExternalIdentity {
        ExternalIdentity::new(
            ExternalSystem::Os {
                server: SERVER.to_string(),
            },
            "owner-os".to_string(),
        )
    }

    fn snapshot_of(usages: Vec<ObservedUsage>, activities: Vec<(u32, u32)>) -> ObservationSnapshot {
        ObservationSnapshot::new(
            usages,
            HashMap::from([(
                SERVER.to_string(),
                ServerObservation::Observed {
                    generated_at: Utc::now(),
                },
            )]),
        )
        .with_gpu_activities(
            activities
                .into_iter()
                .map(|(device_number, peak)| {
                    ((SERVER.to_string(), device_number), GpuActivity::new(peak))
                })
                .collect(),
        )
    }

    /// 本人のプロセスが乗っており、確保しているメモリも分かっている利用
    fn owner_process_on(device_number: u32, used_memory_mib: u64) -> ObservedUsage {
        ObservedUsage::new(
            Resource::Gpu(gpu(device_number)),
            owner_identity(),
            Utc::now(),
        )
        .with_used_memory(used_memory_mib)
    }

    fn judge(reserved: &[Resource], snapshot: &ObservationSnapshot) -> ReservationActivity {
        judge_reservation_activity(reserved, &[owner_identity()], snapshot, COMPUTING)
    }

    fn at_rest_of(activity: &ReservationActivity) -> &GpusAtRest {
        match activity {
            ReservationActivity::HeldWithoutComputing(at_rest) => at_rest,
            other => panic!("計算していないGPUがあるはず: {other:?}"),
        }
    }

    #[test]
    fn a_gpu_without_the_owners_processes_is_absent() {
        let reserved = vec![Resource::Gpu(gpu(0))];
        let snapshot = snapshot_of(vec![], vec![(0, 0)]);

        assert_eq!(judge(&reserved, &snapshot), ReservationActivity::Absent);
    }

    #[test]
    fn a_computing_process_is_in_use() {
        let reserved = vec![Resource::Gpu(gpu(0))];
        let snapshot = snapshot_of(vec![owner_process_on(0, 40_000)], vec![(0, 97)]);

        assert_eq!(judge(&reserved, &snapshot), ReservationActivity::InUse);
    }

    #[test]
    fn a_process_holding_memory_without_computing_is_told_apart_from_using_it() {
        let reserved = vec![Resource::Gpu(gpu(0))];
        let snapshot = snapshot_of(vec![owner_process_on(0, 38_000)], vec![(0, 1)]);

        let activity = judge(&reserved, &snapshot);
        let at_rest = at_rest_of(&activity);

        assert_eq!(at_rest.at_rest(), &[gpu(0)]);
        assert_eq!(at_rest.peak_utilization_percent(), 1);
        assert_eq!(at_rest.used_memory_mib(), Some(38_000));
        assert!(at_rest.is_every_observed_gpu());
    }

    #[test]
    fn a_gpu_left_at_rest_is_reported_even_when_another_one_is_computing() {
        // 2枚押さえて1枚しか回していない。残りの1枚は誰も使えないまま空いている
        let reserved = vec![Resource::Gpu(gpu(0)), Resource::Gpu(gpu(1))];
        let snapshot = snapshot_of(
            vec![owner_process_on(0, 38_000), owner_process_on(1, 12_000)],
            vec![(0, 90), (1, 0)],
        );

        let activity = judge(&reserved, &snapshot);
        let at_rest = at_rest_of(&activity);

        assert_eq!(
            at_rest.at_rest(),
            &[gpu(1)],
            "回っている1枚が、回っていない1枚を覆い隠してはいけない"
        );
        assert_eq!(at_rest.observed_count(), 2);
        assert!(
            !at_rest.is_every_observed_gpu(),
            "一部だけが休んでいることは、全部が休んでいることとは違う"
        );
        assert_eq!(
            at_rest.used_memory_mib(),
            Some(12_000),
            "休んでいるGPUの分だけを数える"
        );
    }

    #[test]
    fn a_gpu_that_does_not_report_its_activity_falls_back_to_the_presence_of_processes() {
        let reserved = vec![Resource::Gpu(gpu(0))];
        let snapshot = snapshot_of(vec![owner_process_on(0, 38_000)], vec![]);

        assert_eq!(
            judge(&reserved, &snapshot),
            ReservationActivity::InUse,
            "稼働率を知らないことを、計算していないことの証拠にしてはいけない"
        );
    }

    #[test]
    fn only_the_gpus_the_owner_occupies_are_weighed() {
        // 予約は2枚だが、本人のプロセスは0番だけ。誰も乗っていない1番は問わない
        let reserved = vec![Resource::Gpu(gpu(0)), Resource::Gpu(gpu(1))];
        let snapshot = snapshot_of(vec![owner_process_on(0, 38_000)], vec![(0, 90), (1, 0)]);

        assert_eq!(judge(&reserved, &snapshot), ReservationActivity::InUse);
    }

    #[test]
    fn memory_held_across_several_resting_gpus_is_summed() {
        let reserved = vec![Resource::Gpu(gpu(0)), Resource::Gpu(gpu(1))];
        let snapshot = snapshot_of(
            vec![owner_process_on(0, 38_000), owner_process_on(1, 12_000)],
            vec![(0, 2), (1, 0)],
        );

        let activity = judge(&reserved, &snapshot);

        assert_eq!(at_rest_of(&activity).used_memory_mib(), Some(50_000));
    }

    #[test]
    fn memory_that_cannot_be_read_leaves_the_amount_unanswerable() {
        let reserved = vec![Resource::Gpu(gpu(0))];
        let snapshot = snapshot_of(
            vec![ObservedUsage::new(
                Resource::Gpu(gpu(0)),
                owner_identity(),
                Utc::now(),
            )],
            vec![(0, 1)],
        );

        let activity = judge(&reserved, &snapshot);

        assert_eq!(
            at_rest_of(&activity).used_memory_mib(),
            None,
            "分からない量を数字にして伝えてはいけない"
        );
    }

    #[test]
    fn someone_elses_process_does_not_make_the_reservation_used() {
        let reserved = vec![Resource::Gpu(gpu(0))];
        let guest = ExternalIdentity::new(
            ExternalSystem::Os {
                server: SERVER.to_string(),
            },
            "guest-os".to_string(),
        );
        let snapshot = snapshot_of(
            vec![ObservedUsage::new(Resource::Gpu(gpu(0)), guest, Utc::now())],
            vec![(0, 90)],
        );

        assert_eq!(
            judge(&reserved, &snapshot),
            ReservationActivity::Absent,
            "他人が回しているGPUの稼働率は、予約者の利用の証にならない"
        );
    }
}
