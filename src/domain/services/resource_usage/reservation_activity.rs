//! 予約が使われているかの見立て

use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource};
use crate::domain::ports::resource_usage_observer::ObservationSnapshot;

/// 押さえられたまま計算していないGPUたち
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
    /// * `used_memory_mib` - 休んでいるGPUで確保されているメモリ量の合計
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

    /// 休んでいるGPUで確保されているメモリ量の合計（MiB）
    ///
    /// `None`は「確保していない」ではなく「どれだけ確保しているかを問えない」を意味する。
    pub fn used_memory_mib(&self) -> Option<u64> {
        self.used_memory_mib
    }
}

/// 予約が使われているかの見立て
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
    /// 押さえているGPUのどれにも利用が観測できない
    Absent,
    /// 使われているかを問えない（観測できないサーバー、部屋の予約）
    Undecidable,
}

/// 観測結果から、予約が使われているかを見立てる
///
/// 予約が押さえるデバイスの上で何が起きているかだけを読み、誰のプロセスかは問わない。
/// 予約はデバイスと責任者をすでに結んでおり、コンテナや共有アカウントのような中間層は
/// プロセスの名義を歪めても、デバイスが計算していることまでは覆い隠せない。
/// 名義の照合は無断使用の検出の仕事であり、この見立ての条件ではない。
///
/// 判定できるのは、予約が押さえるGPUをすべて観測できている場合に限る。その手前で
/// 足りないものがあるときは呼び出し側が[`ReservationActivity::Undecidable`]を選ぶ。
/// この関数は観測できている事実だけを読む。
///
/// 押さえているGPUは1台ずつ見る。8枚のうち1枚で計算が走っていることは、
/// 残りの7枚が使われていることを意味しない。
///
/// # Arguments
/// * `reserved` - 予約が押さえているリソース
/// * `snapshot` - 観測結果
/// * `computing_utilization_percent` - これ以上の稼働率が出ていれば計算が走っているとみなす
pub fn judge_reservation_activity(
    reserved: &[Resource],
    snapshot: &ObservationSnapshot,
    computing_utilization_percent: u32,
) -> ReservationActivity {
    let occupied = occupied_gpus(reserved, snapshot);

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
        memory_held_on(at_rest.iter().map(|(gpu, _)| *gpu), snapshot),
    ))
}

/// 予約が押さえているGPUのうち、何らかの利用が乗っているもの（デバイス番号順）
///
/// 帰属できた利用も帰属不明の利用も等しく数える。帰属できないことを理由に
/// 落とすと、コンテナ越しの利用が「誰もいない」と読まれてしまう。
fn occupied_gpus<'a>(reserved: &'a [Resource], snapshot: &ObservationSnapshot) -> Vec<&'a Gpu> {
    let mut occupied: Vec<&Gpu> = reserved
        .iter()
        .filter_map(|resource| match resource {
            Resource::Gpu(gpu) => Some(gpu),
            Resource::Room { .. } => None,
        })
        .filter(|gpu| memories_held_on(gpu, snapshot).next().is_some())
        .collect();

    occupied.sort_by_key(|gpu| gpu.device_number());
    occupied
}

/// このGPUに乗っている利用それぞれの確保メモリ量（帰属を問わない）
fn memories_held_on<'a>(
    gpu: &Gpu,
    snapshot: &'a ObservationSnapshot,
) -> impl Iterator<Item = Option<u64>> + 'a {
    let reserved = Resource::Gpu(gpu.clone());
    let attributed = snapshot
        .usages()
        .iter()
        .filter({
            let reserved = reserved.clone();
            move |observed| reserved.conflicts_with(observed.resource())
        })
        .map(|observed| observed.used_memory_mib());
    let unattributed = snapshot
        .unattributed_usages()
        .iter()
        .filter(move |observed| reserved.conflicts_with(observed.resource()))
        .map(|observed| observed.used_memory_mib());

    attributed.chain(unattributed)
}

/// これらのGPUで確保されているメモリ量の合計
///
/// ひとつでも読み出せない利用があれば、合計そのものを問えないものとして扱う。
/// 読めた分だけを足した数を確保量として伝えると、実際より少なく見える。
fn memory_held_on<'a>(
    gpus: impl Iterator<Item = &'a Gpu>,
    snapshot: &ObservationSnapshot,
) -> Option<u64> {
    let mut total = 0_u64;

    for gpu in gpus {
        for used_memory_mib in memories_held_on(gpu, snapshot) {
            total += used_memory_mib?;
        }
    }

    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::identity_link::value_objects::{
        ExternalIdentity, ExternalSystem,
    };
    use crate::domain::ports::resource_usage_observer::{
        GpuActivity, ObservedUsage, ServerObservation, UnattributedUsage,
    };
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

    /// 誰かのプロセスが乗っており、確保しているメモリも分かっている利用
    fn process_on(device_number: u32, used_memory_mib: u64) -> ObservedUsage {
        ObservedUsage::new(
            Resource::Gpu(gpu(device_number)),
            owner_identity(),
            Utc::now(),
        )
        .with_used_memory(used_memory_mib)
    }

    /// 誰のものか分からないプロセスが乗っている利用（コンテナ実行等）
    fn unattributed_process_on(device_number: u32, used_memory_mib: u64) -> UnattributedUsage {
        UnattributedUsage::new(Resource::Gpu(gpu(device_number)), 100_000, Utc::now())
            .with_used_memory(used_memory_mib)
    }

    fn judge(reserved: &[Resource], snapshot: &ObservationSnapshot) -> ReservationActivity {
        judge_reservation_activity(reserved, snapshot, COMPUTING)
    }

    fn at_rest_of(activity: &ReservationActivity) -> &GpusAtRest {
        match activity {
            ReservationActivity::HeldWithoutComputing(at_rest) => at_rest,
            other => panic!("計算していないGPUがあるはず: {other:?}"),
        }
    }

    #[test]
    fn a_gpu_without_any_usage_is_absent() {
        let reserved = vec![Resource::Gpu(gpu(0))];
        let snapshot = snapshot_of(vec![], vec![(0, 0)]);

        assert_eq!(judge(&reserved, &snapshot), ReservationActivity::Absent);
    }

    #[test]
    fn a_computing_process_is_in_use() {
        let reserved = vec![Resource::Gpu(gpu(0))];
        let snapshot = snapshot_of(vec![process_on(0, 40_000)], vec![(0, 97)]);

        assert_eq!(judge(&reserved, &snapshot), ReservationActivity::InUse);
    }

    #[test]
    fn a_process_holding_memory_without_computing_is_told_apart_from_using_it() {
        let reserved = vec![Resource::Gpu(gpu(0))];
        let snapshot = snapshot_of(vec![process_on(0, 38_000)], vec![(0, 1)]);

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
            vec![process_on(0, 38_000), process_on(1, 12_000)],
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
        let snapshot = snapshot_of(vec![process_on(0, 38_000)], vec![]);

        assert_eq!(
            judge(&reserved, &snapshot),
            ReservationActivity::InUse,
            "稼働率を知らないことを、計算していないことの証拠にしてはいけない"
        );
    }

    #[test]
    fn only_the_gpus_something_is_running_on_are_weighed() {
        // 予約は2枚だが、プロセスは0番だけ。誰も乗っていない1番は問わない
        let reserved = vec![Resource::Gpu(gpu(0)), Resource::Gpu(gpu(1))];
        let snapshot = snapshot_of(vec![process_on(0, 38_000)], vec![(0, 90), (1, 0)]);

        assert_eq!(judge(&reserved, &snapshot), ReservationActivity::InUse);
    }

    #[test]
    fn memory_held_across_several_resting_gpus_is_summed() {
        let reserved = vec![Resource::Gpu(gpu(0)), Resource::Gpu(gpu(1))];
        let snapshot = snapshot_of(
            vec![process_on(0, 38_000), process_on(1, 12_000)],
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
    fn whoever_is_computing_on_the_reserved_gpu_makes_it_in_use() {
        // 予約者がコンテナ越しに回していれば、プロセスの名義は本人に辿れない。
        // 名義を条件にすると、本人の計算を「誰もいない」と読んでしまう
        let reserved = vec![Resource::Gpu(gpu(0))];
        let snapshot = snapshot_of(vec![process_on(0, 40_000)], vec![(0, 90)]);

        assert_eq!(
            judge(&reserved, &snapshot),
            ReservationActivity::InUse,
            "デバイスが計算していることは、名義の分からなさに覆い隠されない"
        );
    }

    #[test]
    fn an_unattributed_computing_process_is_in_use() {
        let reserved = vec![Resource::Gpu(gpu(0))];
        let snapshot = snapshot_of(vec![], vec![(0, 92)])
            .with_unattributed_usages(vec![unattributed_process_on(0, 40_000)]);

        assert_eq!(
            judge(&reserved, &snapshot),
            ReservationActivity::InUse,
            "帰属できないことは、利用がないことを意味しない"
        );
    }

    #[test]
    fn an_unattributed_process_holding_memory_without_computing_is_reported() {
        let reserved = vec![Resource::Gpu(gpu(0))];
        let snapshot = snapshot_of(vec![], vec![(0, 1)])
            .with_unattributed_usages(vec![unattributed_process_on(0, 24_000)]);

        let activity = judge(&reserved, &snapshot);
        let at_rest = at_rest_of(&activity);

        assert_eq!(at_rest.at_rest(), &[gpu(0)]);
        assert_eq!(
            at_rest.used_memory_mib(),
            Some(24_000),
            "帰属不明の確保も、押さえられている量として数える"
        );
    }

    #[test]
    fn attributed_and_unattributed_memory_on_a_resting_gpu_are_summed_together() {
        let reserved = vec![Resource::Gpu(gpu(0))];
        let snapshot = snapshot_of(vec![process_on(0, 10_000)], vec![(0, 0)])
            .with_unattributed_usages(vec![unattributed_process_on(0, 24_000)]);

        let activity = judge(&reserved, &snapshot);

        assert_eq!(at_rest_of(&activity).used_memory_mib(), Some(34_000));
    }
}
