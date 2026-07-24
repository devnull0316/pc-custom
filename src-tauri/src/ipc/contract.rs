//! Wire types for the future elevated helper.
//!
//! Task 2 deliberately ships the default per-user edition without an elevated
//! helper or administrative actions. These types freeze a narrow protocol for
//! the helper executable implemented in the next task; the current allowlist is
//! deny-all and no transport is started from this module.

use serde::{Deserialize, Serialize};
use std::fmt;

pub const IPC_PROTOCOL_VERSION: u16 = 1;
pub const MAX_ENVELOPE_BYTES: usize = 64 * 1024;
pub const MAX_ACTION_ID_BYTES: usize = 64;
pub const MAX_PARAMETER_SCHEMA_BYTES: usize = 64;
pub const MAX_PARAMETER_COUNT: usize = 32;
pub const MAX_PARAMETER_KEY_BYTES: usize = 64;
pub const MAX_CHOICE_BYTES: usize = 64;
pub const NONCE_BYTES: usize = 32;
pub const NONCE_HEX_BYTES: usize = NONCE_BYTES * 2;
pub const MAX_REQUEST_LIFETIME_MS: i64 = 30_000;
pub const MAX_CLOCK_SKEW_MS: i64 = 5_000;
pub const MAX_SID_BYTES: usize = 68;

/// A single request is processed by a single, local-only pipe instance.
///
/// The next-task transport must translate this policy into an explicit DACL
/// and mandatory-label SACL. It must not use the named-pipe default security
/// descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OneShotNamedPipePolicy {
    pub first_pipe_instance: bool,
    pub reject_remote_clients: bool,
    pub max_instances: u8,
    pub message_mode: bool,
    pub explicit_dacl: bool,
    pub requesting_logon_sid_read_write: bool,
    pub administrators_full_control: bool,
    pub system_full_control: bool,
    pub mandatory_integrity: MandatoryIntegrity,
    pub mandatory_no_write_up: bool,
    pub mutual_peer_verification: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MandatoryIntegrity {
    Medium,
}

pub const ONE_SHOT_NAMED_PIPE_POLICY: OneShotNamedPipePolicy = OneShotNamedPipePolicy {
    first_pipe_instance: true,
    reject_remote_clients: true,
    max_instances: 1,
    message_mode: true,
    explicit_dacl: true,
    requesting_logon_sid_read_write: true,
    administrators_full_control: true,
    system_full_control: true,
    mandatory_integrity: MandatoryIntegrity::Medium,
    mandatory_no_write_up: true,
    mutual_peer_verification: true,
};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ElevatedRequestEnvelope {
    pub protocol_version: u16,
    pub request_id: String,
    pub transaction_id: String,
    pub message_counter: u64,
    pub issued_at_unix_ms: i64,
    pub deadline_unix_ms: i64,
    pub nonce_hex: String,
    pub action: ElevatedActionRequest,
}

impl fmt::Debug for ElevatedRequestEnvelope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ElevatedRequestEnvelope")
            .field("protocol_version", &self.protocol_version)
            .field("request_id", &self.request_id)
            .field("transaction_id", &self.transaction_id)
            .field("message_counter", &self.message_counter)
            .field("issued_at_unix_ms", &self.issued_at_unix_ms)
            .field("deadline_unix_ms", &self.deadline_unix_ms)
            .field("nonce_hex", &"[REDACTED]")
            .field("action_id", &self.action.action_id)
            .field("action_version", &self.action.action_version)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ElevatedActionRequest {
    pub action_id: String,
    pub action_version: u32,
    pub parameters: TypedActionParameters,
}

/// Parameter values are deliberately not arbitrary JSON strings. A future
/// allowlisted action must additionally validate the exact schema id, keys,
/// enum choices, and numeric ranges before dispatch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TypedActionParameters {
    pub schema_id: String,
    pub schema_version: u16,
    pub values: Vec<TypedParameter>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TypedParameter {
    pub name: String,
    pub value: TypedParameterValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypedParameterValue {
    Boolean(BooleanParameter),
    Unsigned(UnsignedParameter),
    Choice(ChoiceParameter),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BooleanParameter {
    pub value: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsignedParameter {
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceParameter {
    pub value: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct FileIdentity {
    pub volume_serial_number: u64,
    pub file_id: [u8; 16],
}

impl fmt::Debug for FileIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FileIdentity([REDACTED])")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrityLevel {
    Medium,
    High,
    System,
}

/// Evidence must be collected from process/token/file handles by the transport,
/// never from values claimed by the request payload.
#[derive(Clone, PartialEq, Eq)]
pub struct PeerEvidence {
    pub process_id: u32,
    pub process_creation_time_100ns: u64,
    pub session_id: u32,
    pub user_sid: Vec<u8>,
    pub logon_sid: Vec<u8>,
    pub integrity_level: IntegrityLevel,
    pub elevated: bool,
    pub normalized_image_path: String,
    pub file_identity: FileIdentity,
    pub publisher_sha256: [u8; 32],
}

impl fmt::Debug for PeerEvidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PeerEvidence")
            .field("process_id", &self.process_id)
            .field("process_creation_time_100ns", &self.process_creation_time_100ns)
            .field("session_id", &self.session_id)
            .field("user_sid", &"[REDACTED]")
            .field("logon_sid", &"[REDACTED]")
            .field("integrity_level", &self.integrity_level)
            .field("elevated", &self.elevated)
            .field("normalized_image_path", &"[REDACTED]")
            .field("file_identity", &"[REDACTED]")
            .field("publisher_sha256", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct PeerExpectation {
    pub process_id: u32,
    pub process_creation_time_100ns: u64,
    pub session_id: u32,
    pub user_sid: Vec<u8>,
    pub logon_sid: Vec<u8>,
    pub integrity_level: IntegrityLevel,
    pub elevated: bool,
    /// Exact output of the transport's handle-based normalization step.
    pub normalized_image_path: String,
    pub file_identity: FileIdentity,
    pub publisher_sha256: [u8; 32],
}

impl fmt::Debug for PeerExpectation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PeerExpectation")
            .field("process_id", &self.process_id)
            .field("session_id", &self.session_id)
            .field("integrity_level", &self.integrity_level)
            .field("elevated", &self.elevated)
            .field("sensitive_identity", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedElevatedRequest {
    pub request_id: String,
    pub transaction_id: String,
    pub message_counter: u64,
    pub action_id: String,
    pub action_version: u32,
    pub parameters: TypedActionParameters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatedPeer {
    pub process_id: u32,
    pub process_creation_time_100ns: u64,
    pub session_id: u32,
    pub integrity_level: IntegrityLevel,
    pub elevated: bool,
}

pub fn nonce_to_lower_hex(nonce: &[u8; NONCE_BYTES]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(NONCE_HEX_BYTES);
    for byte in nonce {
        let byte = *byte;
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}
