use serde::{Deserialize, Serialize};

use super::ActionId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionRiskLevel {
    Safe,
    Caution,
    Experimental,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Persistent,
    Session,
    /// Starts an allowlisted external application. It cannot be reversed safely.
    OneWay,
    Observation,
    Guided,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MethodClass {
    PublicApi,
    MicrosoftCli,
    WinGet,
    OfficialModule,
    DocumentedRegistry,
    LimitedExternal,
    /// A fixed storage location that may be observed, but whose setter
    /// semantics have not been established by primary documentation and
    /// target-build UI smoke tests.
    UnverifiedStorage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowsReleaseFamily {
    Windows11_24H2,
    Windows11_25H2,
    Windows11_26H1,
}

/// Immutable, first-party metadata compiled into the binary.
#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
pub struct ActionMetadata {
    pub id: ActionId,
    pub name: &'static str,
    pub description: &'static str,
    pub category: &'static str,
    pub tags: &'static [&'static str],
    pub supportedWindowsVersions: &'static [WindowsReleaseFamily],
    pub minimumBuild: u32,
    pub maximumTestedBuild: u32,
    pub riskLevel: ActionRiskLevel,
    pub requiresAdmin: bool,
    pub requiresRestart: bool,
    pub requiresExplorerRestart: bool,
    pub conflicts: &'static [ActionId],
    pub dependencies: &'static [ActionId],
    pub action_version: u32,
    pub kind: ActionKind,
    pub parameter_schema: &'static str,
    pub resource_keys: &'static [&'static str],
    pub method_class: MethodClass,
    pub evidence_urls: &'static [&'static str],
    pub compatibility_key: &'static str,
    pub backup_codec_version: u32,
    pub rollback_decoder_versions: &'static [u32],
    pub auto_apply_eligible: bool,
    pub windows_update_impact: &'static str,
}

impl ActionMetadata {
    pub fn validate_static_contract(&self) -> Result<(), &'static str> {
        if self.id.as_str().is_empty() || self.name.is_empty() || self.description.is_empty() {
            return Err("Action metadata contains an empty required field");
        }
        if self.action_version == 0 || self.backup_codec_version == 0 {
            return Err("Action and backup codec versions must be non-zero");
        }
        if !self
            .rollback_decoder_versions
            .contains(&self.backup_codec_version)
        {
            return Err("current backup codec has no rollback decoder");
        }
        // requiresExplorerRestart は「反映にシェル再起動が要る」という表示であって、
        // 「適用時に勝手に再起動する」という意味ではない。再起動は利用者が別途選んだときだけ行う。
        // ただしゲーム起動などで自動適用される経路に載せてはいけない。
        // 開いているフォルダーの窓が予告なく閉じることになる。
        if self.requiresExplorerRestart && self.auto_apply_eligible {
            return Err("Actions needing an Explorer restart may not be auto-applied");
        }
        if matches!(
            self.kind,
            ActionKind::OneWay | ActionKind::Observation | ActionKind::Guided
        ) && self.auto_apply_eligible
        {
            return Err("one-way, observation and guided Actions may not be auto-applied");
        }
        if self.method_class == MethodClass::UnverifiedStorage
            && (self.kind != ActionKind::Guided || self.maximumTestedBuild != 0)
        {
            return Err("unverified storage must be guided and use the untested build sentinel");
        }
        if self.maximumTestedBuild == 0 && self.method_class != MethodClass::UnverifiedStorage {
            return Err("only unverified storage may use the untested build sentinel");
        }
        if self.maximumTestedBuild != 0 && self.maximumTestedBuild < self.minimumBuild {
            return Err("maximum tested build must not be below the minimum build");
        }
        Ok(())
    }
}
