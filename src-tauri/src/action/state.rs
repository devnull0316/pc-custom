use serde::{Deserialize, Serialize};

use crate::backup::Fingerprint;
use crate::window_layout::WindowLayoutObservation;

use super::{AppLaunchBundle, ProcessFileIdentity, ThemeColorMode};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateEvidence {
    pub source: String,
    pub observed_at_unix_ms: u64,
    pub os_build: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeObservation {
    Light,
    Dark,
    Mixed,
    Unconfigured,
}

impl From<ThemeColorMode> for ThemeObservation {
    fn from(value: ThemeColorMode) -> Self {
        match value {
            ThemeColorMode::Light => Self::Light,
            ThemeColorMode::Dark => Self::Dark,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedProcess {
    pub process_id: u32,
    pub creation_time_100ns: u64,
    pub canonical_path: String,
    pub file_identity: ProcessFileIdentity,
    pub corroborated_by_wmi: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupEntrySource {
    CurrentUserRun,
    LocalMachineRun64,
    LocalMachineRun32,
    UserStartupFolder,
    CommonStartupFolder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupEntryStatus {
    RegistryCommand,
    RegistryExpandableCommand,
    StartupFile,
    ReparsePointNotFollowed,
    MalformedRegistryValue,
    UnsupportedRegistryType,
    RegistryValueTooLarge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartupInventoryEntry {
    pub name: String,
    pub source: StartupEntrySource,
    pub status: StartupEntryStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationWarning {
    pub source: String,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartupInventoryObservation {
    pub entries: Vec<StartupInventoryEntry>,
    pub warnings: Vec<ObservationWarning>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemDriveSpaceObservation {
    pub volume: String,
    pub available_bytes: u64,
    pub total_bytes: u64,
    pub total_free_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TempFilesObservation {
    pub file_count: u64,
    pub directory_count: u64,
    pub total_bytes: u64,
    pub skipped_reparse_points: u64,
    pub unreadable_entries: u64,
    pub warnings: Vec<ObservationWarning>,
    pub truncated: bool,
}

/// One independently collected readiness signal.
///
/// A missing user setting is distinct from an API failure: callers can show
/// `Unconfigured` without pretending that a default value was observed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "data", rename_all = "snake_case")]
pub enum ReadinessComponent<T> {
    Known { value: T },
    Unknown { reason_code: String },
    Unconfigured,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrimaryRefreshRateObservation {
    pub hertz: u32,
}

/// Aggregate of DisplayConfig's Advanced Color flags for active display paths.
/// The Windows API covers HDR and wide-color capabilities, so this type avoids
/// claiming that every supported path is necessarily an HDR panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdvancedColorObservation {
    pub active_path_count: u32,
    pub supported_path_count: u32,
    pub enabled_path_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultRenderAudioObservation {
    pub endpoint_exists: bool,
}

/// One active Windows Core Audio render endpoint.
///
/// Core Audio endpoint identifiers are intentionally absent: they are opaque
/// machine-specific values and are not needed for this read-only observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioOutputEndpointObservation {
    pub friendly_name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioOutputObservation {
    pub endpoints: Vec<AudioOutputEndpointObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledPrinterObservation {
    pub name: crate::windows::PrinterName,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultPrinterObservation {
    pub windows_managed: bool,
    pub printers: Vec<InstalledPrinterObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemporaryVpnObservation {
    pub entries: Vec<crate::windows::VpnEntryState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameReadinessObservation {
    pub refresh_rate: ReadinessComponent<PrimaryRefreshRateObservation>,
    pub advanced_color: ReadinessComponent<AdvancedColorObservation>,
    /// Raw `AutoGameModeEnabled` user preference. This is a configuration hint,
    /// not proof that Game Mode is effective for a running process.
    pub game_mode: ReadinessComponent<bool>,
    pub active_power_scheme: ReadinessComponent<String>,
    pub system_drive_space: ReadinessComponent<SystemDriveSpaceObservation>,
    pub default_render_audio: ReadinessComponent<DefaultRenderAudioObservation>,
    /// Raw `ToastEnabled` user preference. Focus, policy, or per-app settings
    /// can still prevent a notification banner from being shown.
    pub toast_notifications: ReadinessComponent<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnownAppState {
    Running,
    NotRunning,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownAppObservation {
    pub name: String,
    pub state: KnownAppState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownAppsObservation {
    pub bundle: AppLaunchBundle,
    pub apps: Vec<KnownAppObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowsUpdateStatusObservation {
    /// WUA returns a local-time Automation date without a UTC offset.
    pub last_checked_local: ReadinessComponent<String>,
    pub restart_pending: ReadinessComponent<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PowerToysInstallationObservation {
    /// True only when the documented App Paths registration resolves to an
    /// existing PowerToys.exe. Uninstall metadata alone never makes this true.
    pub installed: bool,
    /// Optional DisplayVersion from Windows uninstall registration.
    pub version: Option<String>,
    /// Launch is offered only when the fixed App Paths entry resolved.
    pub launch_available: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowColorPreset {
    WindowsBlue,
    Teal,
    Purple,
    Green,
    Amber,
    Red,
    Graphite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ObservedValue {
    RegistryDword {
        configured: Option<u32>,
    },
    Theme(ThemeObservation),
    /// `HIGHCONTRASTW` の全フィールド。scheme の NULL は `None`、空文字は `Some("")`。
    HighContrast {
        enabled: bool,
        structure_size: u32,
        flags: u32,
        scheme: Option<String>,
    },
    SleepLease {
        owned: bool,
        owner_count: usize,
        keep_display_on: bool,
    },
    ShiftInterruptionGuard {
        shift_five_press_shortcut_enabled: bool,
        shift_five_press_confirmation_enabled: bool,
        right_shift_hold_shortcut_enabled: bool,
        right_shift_hold_confirmation_enabled: bool,
        input_assistance_in_use: bool,
    },
    ActivePowerScheme {
        guid: String,
    },
    /// マウスポインターの動き方。
    PointerFeel {
        acceleration_enabled: bool,
        /// Windows が扱う 1〜20 の速さ。
        speed: i32,
    },
    /// Software-mute bit on the one saved `eCommunications` capture endpoint.
    ///
    /// The machine-specific endpoint identifier remains in the private backup,
    /// not in presentation state.
    CommunicationsMicrophone {
        muted: bool,
    },
    /// 電源モード。**要求値と実効値を1つにまとめない。**
    /// Windows は前者を「他の signal に上書きされ得る vote」と説明している。
    PowerMode {
        /// 電源接続時に利用者が選んでいるモード。読めなければ None。
        requested_ac: Option<String>,
        /// 電池使用時に利用者が選んでいるモード。
        requested_dc: Option<String>,
        /// Windows がいま効いていると報告するモード。要求値と一致するとは限らない。
        effective: Option<String>,
    },
    Processes {
        matches: Vec<ObservedProcess>,
    },
    StartupInventory(StartupInventoryObservation),
    SystemDriveSpace(SystemDriveSpaceObservation),
    TempFiles(TempFilesObservation),
    GameReadiness(GameReadinessObservation),
    KnownApps(KnownAppsObservation),
    PowerToysInstallation(PowerToysInstallationObservation),
    WindowsUpdateStatus(WindowsUpdateStatusObservation),
    AudioOutput(AudioOutputObservation),
    DefaultPrinter(DefaultPrinterObservation),
    TemporaryVpn(TemporaryVpnObservation),
    WindowLayout(WindowLayoutObservation),
    AccentColor {
        hex: String,
        opaque_blend: bool,
    },
    AppVolumeSessions {
        active_sessions: usize,
        #[serde(default)]
        unavailable_saved_sessions: usize,
    },
    NoOsChange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DetectedState {
    Known {
        value: ObservedValue,
        evidence: StateEvidence,
    },
    Unknown {
        reason: String,
    },
    Unsupported {
        reason: String,
    },
    PolicyManaged {
        authority: Option<String>,
    },
    Conflict {
        current_fingerprint: String,
    },
    NeedsRestart {
        value: ObservedValue,
        evidence: StateEvidence,
    },
    Error {
        code: String,
        reason: String,
    },
}

impl DetectedState {
    pub fn known_value(&self) -> Option<&ObservedValue> {
        match self {
            Self::Known { value, .. } | Self::NeedsRestart { value, .. } => Some(value),
            _ => None,
        }
    }

    /// Produces a deterministic fingerprint for preview/commit comparisons.
    ///
    /// Observation time is intentionally excluded: taking the same observation
    /// twice must not turn an unchanged OS state into a stale preview.
    pub fn stable_fingerprint(&self) -> Result<Fingerprint, serde_json::Error> {
        #[derive(Serialize)]
        #[serde(deny_unknown_fields)]
        struct StableEvidence<'a> {
            source: &'a str,
            os_build: u32,
        }

        #[derive(Serialize)]
        #[serde(tag = "status", rename_all = "snake_case")]
        enum StableDetectedState<'a> {
            Known {
                value: &'a ObservedValue,
                evidence: StableEvidence<'a>,
            },
            Unknown {
                reason: &'a str,
            },
            Unsupported {
                reason: &'a str,
            },
            PolicyManaged {
                authority: Option<&'a str>,
            },
            Conflict {
                current_fingerprint: &'a str,
            },
            NeedsRestart {
                value: &'a ObservedValue,
                evidence: StableEvidence<'a>,
            },
            Error {
                code: &'a str,
                reason: &'a str,
            },
        }

        let stable = match self {
            Self::Known { value, evidence } => StableDetectedState::Known {
                value,
                evidence: StableEvidence {
                    source: &evidence.source,
                    os_build: evidence.os_build,
                },
            },
            Self::Unknown { reason } => StableDetectedState::Unknown { reason },
            Self::Unsupported { reason } => StableDetectedState::Unsupported { reason },
            Self::PolicyManaged { authority } => StableDetectedState::PolicyManaged {
                authority: authority.as_deref(),
            },
            Self::Conflict {
                current_fingerprint,
            } => StableDetectedState::Conflict {
                current_fingerprint,
            },
            Self::NeedsRestart { value, evidence } => StableDetectedState::NeedsRestart {
                value,
                evidence: StableEvidence {
                    source: &evidence.source,
                    os_build: evidence.os_build,
                },
            },
            Self::Error { code, reason } => StableDetectedState::Error { code, reason },
        };
        serde_json::to_vec(&stable).map(|bytes| Fingerprint::of_bytes(&bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_fingerprint_excludes_observation_time() {
        let first = DetectedState::Known {
            value: ObservedValue::ActivePowerScheme {
                guid: "00000000-0000-0000-0000-000000000000".to_owned(),
            },
            evidence: StateEvidence {
                source: "PowerGetActiveScheme".to_owned(),
                observed_at_unix_ms: 1,
                os_build: 26_100,
            },
        };
        let second = DetectedState::Known {
            value: first.known_value().expect("known value").clone(),
            evidence: StateEvidence {
                source: "PowerGetActiveScheme".to_owned(),
                observed_at_unix_ms: u64::MAX,
                os_build: 26_100,
            },
        };

        assert_eq!(
            first.stable_fingerprint().expect("serialize first"),
            second.stable_fingerprint().expect("serialize second")
        );
    }

    #[test]
    fn stable_fingerprint_retains_evidence_source_and_build() {
        let state = |source: &str, os_build| DetectedState::Known {
            value: ObservedValue::NoOsChange,
            evidence: StateEvidence {
                source: source.to_owned(),
                observed_at_unix_ms: 1,
                os_build,
            },
        };

        assert_ne!(
            state("source-a", 26_100)
                .stable_fingerprint()
                .expect("serialize source a"),
            state("source-b", 26_100)
                .stable_fingerprint()
                .expect("serialize source b")
        );
        assert_ne!(
            state("source-a", 26_100)
                .stable_fingerprint()
                .expect("serialize build a"),
            state("source-a", 26_200)
                .stable_fingerprint()
                .expect("serialize build b")
        );
    }
}
