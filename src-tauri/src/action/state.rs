use serde::{Deserialize, Serialize};

use crate::backup::Fingerprint;

use super::{ProcessFileIdentity, ThemeColorMode};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ObservedValue {
    RegistryDword { configured: Option<u32> },
    Theme(ThemeObservation),
    SleepLease {
        owned: bool,
        owner_count: usize,
        keep_display_on: bool,
    },
    ActivePowerScheme { guid: String },
    Processes { matches: Vec<ObservedProcess> },
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
