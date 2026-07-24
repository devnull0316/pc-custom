use std::{
    collections::BTreeMap,
    sync::{mpsc, OnceLock},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{WindowsError, WindowsErrorKind, WindowsResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SleepLeaseSnapshot {
    pub owner_count: usize,
    pub keep_display_on: bool,
    pub active: bool,
    pub requested_owner_active: bool,
}

enum Command {
    Acquire {
        owner: Uuid,
        keep_display_on: bool,
        reply: mpsc::SyncSender<WindowsResult<SleepLeaseSnapshot>>,
    },
    Release {
        owner: Uuid,
        reply: mpsc::SyncSender<WindowsResult<SleepLeaseSnapshot>>,
    },
    Snapshot {
        owner: Option<Uuid>,
        reply: mpsc::SyncSender<WindowsResult<SleepLeaseSnapshot>>,
    },
}

pub struct SleepLeaseManager {
    sender: mpsc::Sender<Command>,
}

impl SleepLeaseManager {
    #[cfg(windows)]
    fn start() -> WindowsResult<Self> {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("totonoe-execution-state".to_owned())
            .spawn(move || worker(receiver))
            .map_err(|error| WindowsError::io("start execution-state worker", &error))?;
        Ok(Self { sender })
    }

    pub fn acquire(
        &self,
        owner: Uuid,
        keep_display_on: bool,
    ) -> WindowsResult<SleepLeaseSnapshot> {
        self.request(|reply| Command::Acquire {
            owner,
            keep_display_on,
            reply,
        })
    }

    pub fn release(&self, owner: Uuid) -> WindowsResult<SleepLeaseSnapshot> {
        self.request(|reply| Command::Release { owner, reply })
    }

    pub fn snapshot(&self) -> WindowsResult<SleepLeaseSnapshot> {
        self.request(|reply| Command::Snapshot { owner: None, reply })
    }

    pub fn snapshot_for(&self, owner: Uuid) -> WindowsResult<SleepLeaseSnapshot> {
        self.request(|reply| Command::Snapshot { owner: Some(owner), reply })
    }

    fn request(
        &self,
        command: impl FnOnce(mpsc::SyncSender<WindowsResult<SleepLeaseSnapshot>>) -> Command,
    ) -> WindowsResult<SleepLeaseSnapshot> {
        let (reply_sender, reply_receiver) = mpsc::sync_channel(1);
        self.sender.send(command(reply_sender)).map_err(|_| {
            WindowsError::new(
                WindowsErrorKind::ChannelClosed,
                "send execution-state request",
                None,
            )
        })?;
        reply_receiver.recv().map_err(|_| {
            WindowsError::new(
                WindowsErrorKind::ChannelClosed,
                "receive execution-state result",
                None,
            )
        })?
    }
}

static SLEEP_LEASE_MANAGER: OnceLock<SleepLeaseManager> = OnceLock::new();

#[cfg(windows)]
pub fn sleep_lease_manager() -> WindowsResult<&'static SleepLeaseManager> {
    if let Some(manager) = SLEEP_LEASE_MANAGER.get() {
        return Ok(manager);
    }
    let manager = SleepLeaseManager::start()?;
    match SLEEP_LEASE_MANAGER.set(manager) {
        Ok(()) => {}
        // Another caller won the initialization race; its live manager is used below.
        Err(_redundant_manager) => {}
    }
    SLEEP_LEASE_MANAGER.get().ok_or_else(|| {
        WindowsError::new(
            WindowsErrorKind::ChannelClosed,
            "initialize execution-state manager",
            None,
        )
    })
}

#[cfg(not(windows))]
pub fn sleep_lease_manager() -> WindowsResult<&'static SleepLeaseManager> {
    Err(WindowsError::unsupported("initialize execution-state manager"))
}

#[cfg(windows)]
fn worker(receiver: mpsc::Receiver<Command>) {
    let mut leases = BTreeMap::<Uuid, bool>::new();
    while let Ok(command) = receiver.recv() {
        match command {
            Command::Acquire {
                owner,
                keep_display_on,
                reply,
            } => {
                let mut next = leases.clone();
                next.insert(owner, keep_display_on);
                let result = configure_execution_state(&next).map(|()| {
                    leases = next;
                    snapshot_of(&leases, Some(owner))
                });
                if reply.send(result).is_err() {
                    eprintln!("execution-state requester disconnected after acquire");
                }
            }
            Command::Release { owner, reply } => {
                let mut next = leases.clone();
                next.remove(&owner);
                let result = configure_execution_state(&next).map(|()| {
                    leases = next;
                    snapshot_of(&leases, Some(owner))
                });
                if reply.send(result).is_err() {
                    eprintln!("execution-state requester disconnected after release");
                }
            }
            Command::Snapshot { owner, reply } => {
                if reply.send(Ok(snapshot_of(&leases, owner))).is_err() {
                    eprintln!("execution-state requester disconnected during snapshot");
                }
            }
        }
    }
    if let Err(error) = configure_execution_state(&BTreeMap::new()) {
        eprintln!("execution-state final release failed: {error}");
    }
}

#[cfg(windows)]
fn snapshot_of(leases: &BTreeMap<Uuid, bool>, owner: Option<Uuid>) -> SleepLeaseSnapshot {
    SleepLeaseSnapshot {
        owner_count: leases.len(),
        keep_display_on: leases.values().any(|value| *value),
        active: !leases.is_empty(),
        requested_owner_active: owner.is_some_and(|owner| leases.contains_key(&owner)),
    }
}

#[cfg(windows)]
fn configure_execution_state(leases: &BTreeMap<Uuid, bool>) -> WindowsResult<()> {
    use windows::Win32::System::Power::{
        SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED,
    };

    let mut flags = ES_CONTINUOUS;
    if !leases.is_empty() {
        flags |= ES_SYSTEM_REQUIRED;
        if leases.values().any(|value| *value) {
            flags |= ES_DISPLAY_REQUIRED;
        }
    }
    let previous = unsafe { SetThreadExecutionState(flags) };
    if previous.0 == 0 {
        return Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "SetThreadExecutionState",
            std::io::Error::last_os_error()
                .raw_os_error()
                .map(i64::from),
        ));
    }
    Ok(())
}
