//! ゲームプロファイルの背景監視スレッド。
//!
//! 一定間隔で「有効プロファイルの同期 → プロセススナップショット → 状態機械 tick」を回す。
//! 有効プロファイルが 1 件も無い間はスナップショット(全プロセスの本人性確認は重い)を省き、
//! 実質ゼロコストで待機する。適用/復元は [`EngineProfileSink`] が journal を正として行うため、
//! アプリやゲームがクラッシュしても次回起動時の reconcile が未復元を引き取る。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::engine::TotonoeEngine;

use super::engine_sink::EngineProfileSink;
use super::store::ProfileStore;
use super::{InstanceKey, ObservedProcess, ProfileRuntime};

const POLL_INTERVAL: Duration = Duration::from_secs(3);
const STOP_CHECK: Duration = Duration::from_millis(200);

pub struct ProfileWatcher {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl ProfileWatcher {
    pub fn spawn(engine: Arc<TotonoeEngine>, store: Arc<ProfileStore>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let handle = thread::Builder::new()
            .name("totonoe-profile-watcher".to_owned())
            .spawn(move || run_loop(engine, store, stop_thread))
            .ok();
        Self { stop, handle }
    }
}

impl Drop for ProfileWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn run_loop(engine: Arc<TotonoeEngine>, store: Arc<ProfileStore>, stop: Arc<AtomicBool>) {
    let sink = EngineProfileSink::new(engine);
    let mut runtime = ProfileRuntime::new(sink);

    while !stop.load(Ordering::SeqCst) {
        let profiles = store.list();
        // 変換失敗プロファイルはスキップされる(理由は将来ログ/表示へ回す)。
        let _skipped = runtime.sync(&profiles);

        // 検知対象が無ければ重いスナップショットを取らない。
        if runtime.has_targets() {
            if let Ok(observed) = current_processes() {
                // tick 内の適用/復元失敗は journal に残る。ここでは監視を継続する。
                let _ = runtime.tick(&observed);
            }
        }

        // 停止要求へ素早く反応するため、短いスリープを繰り返して間隔を刻む。
        let ticks = (POLL_INTERVAL.as_millis() / STOP_CHECK.as_millis()).max(1);
        for _ in 0..ticks {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(STOP_CHECK);
        }
    }
}

fn current_processes() -> Result<Vec<ObservedProcess>, ()> {
    let report = crate::windows::snapshot_process_identities().map_err(|_| ())?;
    Ok(report
        .processes
        .into_iter()
        .map(|process| ObservedProcess {
            instance: InstanceKey {
                process_id: process.process_id,
                creation_time_100ns: process.creation_time_100ns,
            },
            canonical_path: process.canonical_path,
            file_identity: Some(process.file_identity),
        })
        .collect())
}
