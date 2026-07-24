use super::contract::{
    ElevatedRequestEnvelope, PeerEvidence, PeerExpectation,
    TypedActionParameters, TypedParameterValue, ValidatedElevatedRequest, ValidatedPeer,
    IPC_PROTOCOL_VERSION, MAX_ACTION_ID_BYTES, MAX_CHOICE_BYTES, MAX_CLOCK_SKEW_MS,
    MAX_ENVELOPE_BYTES, MAX_PARAMETER_COUNT, MAX_PARAMETER_KEY_BYTES,
    MAX_PARAMETER_SCHEMA_BYTES, MAX_REQUEST_LIFETIME_MS, MAX_SID_BYTES, NONCE_BYTES,
    NONCE_HEX_BYTES,
};
use std::{collections::BTreeSet, error::Error, fmt};

/// Current-edition authorization is intentionally empty. The next-task helper
/// supplies a separate, compile-time allowlist and exact per-action schemas.
pub trait ElevatedActionAllowlist: Send + Sync {
    fn authorizes(
        &self,
        action_id: &str,
        action_version: u32,
        parameters: &TypedActionParameters,
    ) -> bool;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DenyAllElevatedActions;

impl ElevatedActionAllowlist for DenyAllElevatedActions {
    fn authorizes(
        &self,
        _action_id: &str,
        _action_version: u32,
        _parameters: &TypedActionParameters,
    ) -> bool {
        false
    }
}

pub struct RequestValidationContext<'a> {
    pub expected_transaction_id: &'a str,
    pub expected_nonce: &'a [u8; NONCE_BYTES],
    pub expected_message_counter: u64,
    pub now_unix_ms: i64,
    pub seen_request_ids: &'a BTreeSet<String>,
    pub allowlist: &'a dyn ElevatedActionAllowlist,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    EmptyPayload,
    PayloadTooLarge,
    MalformedJson,
    UnsupportedProtocol,
    InvalidRequestId,
    InvalidTransactionId,
    TransactionMismatch,
    Replay,
    UnexpectedMessageCounter,
    InvalidTimeline,
    RequestNotYetValid,
    RequestExpired,
    RequestLifetimeTooLong,
    InvalidNonceEncoding,
    NonceMismatch,
    InvalidActionId,
    InvalidActionVersion,
    InvalidParameterSchema,
    InvalidParameter,
    ParameterLimitExceeded,
    ActionNotAllowlisted,
    InvalidPeerEvidence,
    PeerProcessMismatch,
    PeerCreationTimeMismatch,
    PeerSessionMismatch,
    PeerTokenMismatch,
    PeerIntegrityMismatch,
    PeerImageMismatch,
    PeerFileIdentityMismatch,
    PeerPublisherMismatch,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EmptyPayload => "IPC payload is empty",
            Self::PayloadTooLarge => "IPC payload exceeds the fixed limit",
            Self::MalformedJson => "IPC payload does not match the strict schema",
            Self::UnsupportedProtocol => "IPC protocol version is unsupported",
            Self::InvalidRequestId => "request ID is not canonical",
            Self::InvalidTransactionId => "transaction ID is not canonical",
            Self::TransactionMismatch => "transaction ID does not match the prepared transaction",
            Self::Replay => "request ID was already observed",
            Self::UnexpectedMessageCounter => "message counter is out of sequence",
            Self::InvalidTimeline => "request timestamps are inconsistent",
            Self::RequestNotYetValid => "request issue time is too far in the future",
            Self::RequestExpired => "request deadline has expired",
            Self::RequestLifetimeTooLong => "request lifetime exceeds the fixed limit",
            Self::InvalidNonceEncoding => "nonce encoding is invalid",
            Self::NonceMismatch => "nonce does not match the one-shot launch context",
            Self::InvalidActionId => "Action ID is invalid",
            Self::InvalidActionVersion => "Action version is invalid",
            Self::InvalidParameterSchema => "typed parameter schema is invalid",
            Self::InvalidParameter => "typed parameter value is invalid",
            Self::ParameterLimitExceeded => "typed parameter limit was exceeded",
            Self::ActionNotAllowlisted => "Action is not present in the elevated allowlist",
            Self::InvalidPeerEvidence => "peer evidence is incomplete",
            Self::PeerProcessMismatch => "peer process ID does not match",
            Self::PeerCreationTimeMismatch => "peer process creation time does not match",
            Self::PeerSessionMismatch => "peer session does not match",
            Self::PeerTokenMismatch => "peer token identity does not match",
            Self::PeerIntegrityMismatch => "peer integrity or elevation does not match",
            Self::PeerImageMismatch => "peer image identity does not match",
            Self::PeerFileIdentityMismatch => "peer file identity does not match",
            Self::PeerPublisherMismatch => "peer publisher identity does not match",
        })
    }
}

impl Error for ValidationError {}

pub fn validate_request_envelope(
    raw: &[u8],
    context: &RequestValidationContext<'_>,
) -> Result<ValidatedElevatedRequest, ValidationError> {
    if raw.is_empty() {
        return Err(ValidationError::EmptyPayload);
    }
    if raw.len() > MAX_ENVELOPE_BYTES {
        return Err(ValidationError::PayloadTooLarge);
    }

    let envelope: ElevatedRequestEnvelope =
        serde_json::from_slice(raw).map_err(|_| ValidationError::MalformedJson)?;

    if envelope.protocol_version != IPC_PROTOCOL_VERSION {
        return Err(ValidationError::UnsupportedProtocol);
    }
    if !is_canonical_uuid(&envelope.request_id) {
        return Err(ValidationError::InvalidRequestId);
    }
    if !is_canonical_uuid(&envelope.transaction_id) {
        return Err(ValidationError::InvalidTransactionId);
    }
    if envelope.transaction_id != context.expected_transaction_id {
        return Err(ValidationError::TransactionMismatch);
    }
    if context.seen_request_ids.contains(&envelope.request_id) {
        return Err(ValidationError::Replay);
    }
    if envelope.message_counter == 0
        || envelope.message_counter != context.expected_message_counter
    {
        return Err(ValidationError::UnexpectedMessageCounter);
    }

    let lifetime = envelope
        .deadline_unix_ms
        .checked_sub(envelope.issued_at_unix_ms)
        .ok_or(ValidationError::InvalidTimeline)?;
    if lifetime < 0 {
        return Err(ValidationError::InvalidTimeline);
    }
    if lifetime > MAX_REQUEST_LIFETIME_MS {
        return Err(ValidationError::RequestLifetimeTooLong);
    }
    let latest_issue_time = context
        .now_unix_ms
        .checked_add(MAX_CLOCK_SKEW_MS)
        .ok_or(ValidationError::InvalidTimeline)?;
    if envelope.issued_at_unix_ms > latest_issue_time {
        return Err(ValidationError::RequestNotYetValid);
    }
    if envelope.deadline_unix_ms < context.now_unix_ms {
        return Err(ValidationError::RequestExpired);
    }

    let nonce = decode_lower_hex_nonce(&envelope.nonce_hex)?;
    if !constant_time_equal(&nonce, context.expected_nonce) {
        return Err(ValidationError::NonceMismatch);
    }

    validate_action_id(&envelope.action.action_id)?;
    if envelope.action.action_version == 0 {
        return Err(ValidationError::InvalidActionVersion);
    }
    validate_parameters(&envelope.action.parameters)?;

    if !context.allowlist.authorizes(
        &envelope.action.action_id,
        envelope.action.action_version,
        &envelope.action.parameters,
    ) {
        return Err(ValidationError::ActionNotAllowlisted);
    }

    Ok(ValidatedElevatedRequest {
        request_id: envelope.request_id,
        transaction_id: envelope.transaction_id,
        message_counter: envelope.message_counter,
        action_id: envelope.action.action_id,
        action_version: envelope.action.action_version,
        parameters: envelope.action.parameters,
    })
}

pub fn validate_peer_evidence(
    evidence: &PeerEvidence,
    expected: &PeerExpectation,
) -> Result<ValidatedPeer, ValidationError> {
    if evidence.process_id == 0
        || evidence.process_creation_time_100ns == 0
        || !is_valid_sid_blob(&evidence.user_sid)
        || !is_valid_sid_blob(&evidence.logon_sid)
        || evidence.normalized_image_path.is_empty()
        || evidence.normalized_image_path.len() > 32_767
    {
        return Err(ValidationError::InvalidPeerEvidence);
    }
    if evidence.process_id != expected.process_id {
        return Err(ValidationError::PeerProcessMismatch);
    }
    if evidence.process_creation_time_100ns != expected.process_creation_time_100ns {
        return Err(ValidationError::PeerCreationTimeMismatch);
    }
    if evidence.session_id != expected.session_id {
        return Err(ValidationError::PeerSessionMismatch);
    }
    if !constant_time_equal(&evidence.user_sid, &expected.user_sid)
        || !constant_time_equal(&evidence.logon_sid, &expected.logon_sid)
    {
        return Err(ValidationError::PeerTokenMismatch);
    }
    if evidence.integrity_level != expected.integrity_level
        || evidence.elevated != expected.elevated
    {
        return Err(ValidationError::PeerIntegrityMismatch);
    }
    if evidence.normalized_image_path != expected.normalized_image_path {
        return Err(ValidationError::PeerImageMismatch);
    }
    if evidence.file_identity.volume_serial_number
        != expected.file_identity.volume_serial_number
        || !constant_time_equal(
            &evidence.file_identity.file_id,
            &expected.file_identity.file_id,
        )
    {
        return Err(ValidationError::PeerFileIdentityMismatch);
    }
    if !constant_time_equal(&evidence.publisher_sha256, &expected.publisher_sha256) {
        return Err(ValidationError::PeerPublisherMismatch);
    }

    Ok(ValidatedPeer {
        process_id: evidence.process_id,
        process_creation_time_100ns: evidence.process_creation_time_100ns,
        session_id: evidence.session_id,
        integrity_level: evidence.integrity_level,
        elevated: evidence.elevated,
    })
}

fn validate_action_id(value: &str) -> Result<(), ValidationError> {
    if value.len() > MAX_ACTION_ID_BYTES || !is_namespaced_identifier(value) {
        return Err(ValidationError::InvalidActionId);
    }
    Ok(())
}

fn validate_parameters(parameters: &TypedActionParameters) -> Result<(), ValidationError> {
    if parameters.schema_version == 0
        || parameters.schema_id.len() > MAX_PARAMETER_SCHEMA_BYTES
        || !is_namespaced_identifier(&parameters.schema_id)
    {
        return Err(ValidationError::InvalidParameterSchema);
    }
    if parameters.values.len() > MAX_PARAMETER_COUNT {
        return Err(ValidationError::ParameterLimitExceeded);
    }

    let mut parameter_names = BTreeSet::new();
    for parameter in &parameters.values {
        let key = &parameter.name;
        let value = &parameter.value;
        if key.is_empty()
            || key.len() > MAX_PARAMETER_KEY_BYTES
            || !key.bytes().enumerate().all(|(index, byte)| {
                byte.is_ascii_lowercase()
                    || byte == b'_'
                    || (index > 0 && byte.is_ascii_digit())
            })
        {
            return Err(ValidationError::InvalidParameter);
        }
        if !parameter_names.insert(key.as_str()) {
            return Err(ValidationError::InvalidParameter);
        }

        if let TypedParameterValue::Choice(choice) = value {
            if choice.value.is_empty()
                || choice.value.len() > MAX_CHOICE_BYTES
                || !choice.value.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-' | b'.')
                })
            {
                return Err(ValidationError::InvalidParameter);
            }
        }
    }
    Ok(())
}

fn is_namespaced_identifier(value: &str) -> bool {
    let mut saw_dot = false;
    for segment in value.split('.') {
        if segment.is_empty() {
            return false;
        }
        if !segment.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || byte == b'_'
                || (index > 0 && byte.is_ascii_digit())
        }) {
            return false;
        }
        saw_dot = true;
    }
    saw_dot && value.contains('.')
}

fn is_canonical_uuid(value: &str) -> bool {
    if value.len() != 36 {
        return false;
    }
    value.bytes().enumerate().all(|(index, byte)| match index {
        8 | 13 | 18 | 23 => byte == b'-',
        _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte),
    })
}

fn decode_lower_hex_nonce(value: &str) -> Result<[u8; NONCE_BYTES], ValidationError> {
    if value.len() != NONCE_HEX_BYTES {
        return Err(ValidationError::InvalidNonceEncoding);
    }
    let mut decoded = [0u8; NONCE_BYTES];
    let bytes = value.as_bytes();
    for index in 0..NONCE_BYTES {
        let high = lower_hex_nibble(bytes[index * 2])?;
        let low = lower_hex_nibble(bytes[index * 2 + 1])?;
        decoded[index] = (high << 4) | low;
    }
    Ok(decoded)
}

fn lower_hex_nibble(byte: u8) -> Result<u8, ValidationError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(ValidationError::InvalidNonceEncoding),
    }
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left_byte, right_byte) in left.iter().zip(right) {
        difference |= *left_byte ^ *right_byte;
    }
    difference == 0
}

fn is_valid_sid_blob(sid: &[u8]) -> bool {
    (8..=MAX_SID_BYTES).contains(&sid.len())
}

#[cfg(test)]
mod attack_spike_tests {
    use super::*;
    use crate::ipc::contract::{
        nonce_to_lower_hex, BooleanParameter, ElevatedActionRequest, IntegrityLevel,
        ONE_SHOT_NAMED_PIPE_POLICY,
    };

    const REQUEST_ID: &str = "11111111-1111-4111-8111-111111111111";
    const TRANSACTION_ID: &str = "22222222-2222-4222-8222-222222222222";
    const NOW: i64 = 1_000_000;

    struct ContractProbeAllowlist;

    impl ElevatedActionAllowlist for ContractProbeAllowlist {
        fn authorizes(
            &self,
            action_id: &str,
            action_version: u32,
            parameters: &TypedActionParameters,
        ) -> bool {
            action_id == "admin.contract_probe"
                && action_version == 1
                && parameters.schema_id == "admin.contract_probe"
                && parameters.schema_version == 1
                && parameters.values.len() == 1
        }
    }

    fn sample_request() -> ElevatedRequestEnvelope {
        let values = vec![TypedParameter {
            name: "enabled".to_owned(),
            value: TypedParameterValue::Boolean(BooleanParameter { value: true }),
        }];

        ElevatedRequestEnvelope {
            protocol_version: IPC_PROTOCOL_VERSION,
            request_id: REQUEST_ID.to_owned(),
            transaction_id: TRANSACTION_ID.to_owned(),
            message_counter: 1,
            issued_at_unix_ms: NOW - 1_000,
            deadline_unix_ms: NOW + 10_000,
            nonce_hex: nonce_to_lower_hex(&[0x11; NONCE_BYTES]),
            action: ElevatedActionRequest {
                action_id: "admin.contract_probe".to_owned(),
                action_version: 1,
                parameters: TypedActionParameters {
                    schema_id: "admin.contract_probe".to_owned(),
                    schema_version: 1,
                    values,
                },
            },
        }
    }

    fn validate(
        request: &ElevatedRequestEnvelope,
        allowlist: &dyn ElevatedActionAllowlist,
        seen: &BTreeSet<String>,
    ) -> Result<ValidatedElevatedRequest, ValidationError> {
        let raw = serde_json::to_vec(request).expect("test request must serialize");
        validate_request_envelope(
            &raw,
            &RequestValidationContext {
                expected_transaction_id: TRANSACTION_ID,
                expected_nonce: &[0x11; NONCE_BYTES],
                expected_message_counter: 1,
                now_unix_ms: NOW,
                seen_request_ids: seen,
                allowlist,
            },
        )
    }

    #[test]
    fn well_formed_contract_is_accepted_by_a_test_only_allowlist() {
        let result = validate(
            &sample_request(),
            &ContractProbeAllowlist,
            &BTreeSet::new(),
        )
        .expect("well-formed contract should pass structural validation");
        assert_eq!(result.action_id, "admin.contract_probe");
    }

    #[test]
    fn current_edition_rejects_every_elevated_action() {
        let error = validate(
            &sample_request(),
            &DenyAllElevatedActions,
            &BTreeSet::new(),
        )
        .expect_err("per-user edition must be deny-all");
        assert_eq!(error, ValidationError::ActionNotAllowlisted);
    }

    #[test]
    fn oversized_payload_is_rejected_before_json_parsing() {
        let raw = vec![b' '; MAX_ENVELOPE_BYTES + 1];
        let error = validate_request_envelope(
            &raw,
            &RequestValidationContext {
                expected_transaction_id: TRANSACTION_ID,
                expected_nonce: &[0x11; NONCE_BYTES],
                expected_message_counter: 1,
                now_unix_ms: NOW,
                seen_request_ids: &BTreeSet::new(),
                allowlist: &ContractProbeAllowlist,
            },
        )
        .expect_err("oversized payload must fail");
        assert_eq!(error, ValidationError::PayloadTooLarge);
    }

    #[test]
    fn unknown_and_duplicate_fields_are_rejected() {
        let mut json = serde_json::to_value(sample_request()).expect("serialize test request");
        json.as_object_mut()
            .expect("envelope is an object")
            .insert("unexpected".to_owned(), serde_json::Value::Bool(true));
        let unknown = serde_json::to_vec(&json).expect("serialize malformed test value");

        let context = RequestValidationContext {
            expected_transaction_id: TRANSACTION_ID,
            expected_nonce: &[0x11; NONCE_BYTES],
            expected_message_counter: 1,
            now_unix_ms: NOW,
            seen_request_ids: &BTreeSet::new(),
            allowlist: &ContractProbeAllowlist,
        };
        assert_eq!(
            validate_request_envelope(&unknown, &context),
            Err(ValidationError::MalformedJson)
        );

        let valid = serde_json::to_string(&sample_request()).expect("serialize test request");
        let duplicate = valid.replacen(
            "\"protocolVersion\":1",
            "\"protocolVersion\":1,\"protocolVersion\":1",
            1,
        );
        assert_eq!(
            validate_request_envelope(duplicate.as_bytes(), &context),
            Err(ValidationError::MalformedJson)
        );
    }

    #[test]
    fn nonce_replay_counter_and_deadline_attacks_are_rejected() {
        let mut bad_nonce = sample_request();
        bad_nonce.nonce_hex = "00".repeat(NONCE_BYTES);
        assert_eq!(
            validate(&bad_nonce, &ContractProbeAllowlist, &BTreeSet::new()),
            Err(ValidationError::NonceMismatch)
        );

        let mut seen = BTreeSet::new();
        seen.insert(REQUEST_ID.to_owned());
        assert_eq!(
            validate(&sample_request(), &ContractProbeAllowlist, &seen),
            Err(ValidationError::Replay)
        );

        let mut wrong_counter = sample_request();
        wrong_counter.message_counter = 2;
        assert_eq!(
            validate(
                &wrong_counter,
                &ContractProbeAllowlist,
                &BTreeSet::new()
            ),
            Err(ValidationError::UnexpectedMessageCounter)
        );

        let mut expired = sample_request();
        expired.issued_at_unix_ms = NOW - 20_000;
        expired.deadline_unix_ms = NOW - 1;
        assert_eq!(
            validate(&expired, &ContractProbeAllowlist, &BTreeSet::new()),
            Err(ValidationError::RequestExpired)
        );
    }

    #[test]
    fn transaction_action_schema_nonce_encoding_and_duplicate_parameters_fail_closed() {
        let mut wrong_transaction = sample_request();
        wrong_transaction.transaction_id =
            "33333333-3333-4333-8333-333333333333".to_owned();
        assert_eq!(
            validate(
                &wrong_transaction,
                &ContractProbeAllowlist,
                &BTreeSet::new()
            ),
            Err(ValidationError::TransactionMismatch)
        );

        let mut bad_action_id = sample_request();
        bad_action_id.action.action_id = "Admin.contract_probe".to_owned();
        assert_eq!(
            validate(&bad_action_id, &ContractProbeAllowlist, &BTreeSet::new()),
            Err(ValidationError::InvalidActionId)
        );

        let mut bad_version = sample_request();
        bad_version.action.action_version = 0;
        assert_eq!(
            validate(&bad_version, &ContractProbeAllowlist, &BTreeSet::new()),
            Err(ValidationError::InvalidActionVersion)
        );

        let mut bad_schema = sample_request();
        bad_schema.action.parameters.schema_version = 0;
        assert_eq!(
            validate(&bad_schema, &ContractProbeAllowlist, &BTreeSet::new()),
            Err(ValidationError::InvalidParameterSchema)
        );

        let mut duplicate_parameter = sample_request();
        let repeated = duplicate_parameter.action.parameters.values[0].clone();
        duplicate_parameter.action.parameters.values.push(repeated);
        assert_eq!(
            validate(
                &duplicate_parameter,
                &ContractProbeAllowlist,
                &BTreeSet::new()
            ),
            Err(ValidationError::InvalidParameter)
        );

        let mut invalid_nonce = sample_request();
        invalid_nonce.nonce_hex = "AA".repeat(NONCE_BYTES);
        assert_eq!(
            validate(&invalid_nonce, &ContractProbeAllowlist, &BTreeSet::new()),
            Err(ValidationError::InvalidNonceEncoding)
        );
    }

    fn sample_peer() -> PeerEvidence {
        PeerEvidence {
            process_id: 4242,
            process_creation_time_100ns: 123_456,
            session_id: 1,
            user_sid: vec![1; 12],
            logon_sid: vec![2; 12],
            integrity_level: IntegrityLevel::Medium,
            elevated: false,
            normalized_image_path: r"\\?\C:\Program Files\Totonoe\totonoe.exe".to_owned(),
            file_identity: FileIdentity {
                volume_serial_number: 9,
                file_id: [3; 16],
            },
            publisher_sha256: [4; 32],
        }
    }

    fn expectation(peer: &PeerEvidence) -> PeerExpectation {
        PeerExpectation {
            process_id: peer.process_id,
            process_creation_time_100ns: peer.process_creation_time_100ns,
            session_id: peer.session_id,
            user_sid: peer.user_sid.clone(),
            logon_sid: peer.logon_sid.clone(),
            integrity_level: peer.integrity_level,
            elevated: peer.elevated,
            normalized_image_path: peer.normalized_image_path.clone(),
            file_identity: peer.file_identity.clone(),
            publisher_sha256: peer.publisher_sha256,
        }
    }

    #[test]
    fn peer_pid_reuse_session_token_image_and_signature_mismatches_fail_closed() {
        let peer = sample_peer();
        let expected = expectation(&peer);
        assert!(validate_peer_evidence(&peer, &expected).is_ok());

        let mut changed = peer.clone();
        changed.process_id += 1;
        assert_eq!(
            validate_peer_evidence(&changed, &expected),
            Err(ValidationError::PeerProcessMismatch)
        );

        let mut changed = peer.clone();
        changed.integrity_level = IntegrityLevel::High;
        assert_eq!(
            validate_peer_evidence(&changed, &expected),
            Err(ValidationError::PeerIntegrityMismatch)
        );

        let mut changed = peer.clone();
        changed.normalized_image_path.push_str(".different");
        assert_eq!(
            validate_peer_evidence(&changed, &expected),
            Err(ValidationError::PeerImageMismatch)
        );

        let mut changed = peer.clone();
        changed.process_creation_time_100ns += 1;
        assert_eq!(
            validate_peer_evidence(&changed, &expected),
            Err(ValidationError::PeerCreationTimeMismatch)
        );

        let mut changed = peer.clone();
        changed.session_id += 1;
        assert_eq!(
            validate_peer_evidence(&changed, &expected),
            Err(ValidationError::PeerSessionMismatch)
        );

        let mut changed = peer.clone();
        changed.logon_sid[0] ^= 1;
        assert_eq!(
            validate_peer_evidence(&changed, &expected),
            Err(ValidationError::PeerTokenMismatch)
        );

        let mut changed = peer.clone();
        changed.file_identity.file_id[0] ^= 1;
        assert_eq!(
            validate_peer_evidence(&changed, &expected),
            Err(ValidationError::PeerFileIdentityMismatch)
        );

        let mut changed = peer;
        changed.publisher_sha256[0] ^= 1;
        assert_eq!(
            validate_peer_evidence(&changed, &expected),
            Err(ValidationError::PeerPublisherMismatch)
        );
    }

    #[test]
    fn one_shot_pipe_policy_cannot_fall_back_to_defaults() {
        let policy = ONE_SHOT_NAMED_PIPE_POLICY;
        assert!(policy.first_pipe_instance);
        assert!(policy.reject_remote_clients);
        assert_eq!(policy.max_instances, 1);
        assert!(policy.message_mode);
        assert!(policy.explicit_dacl);
        assert!(policy.requesting_logon_sid_read_write);
        assert!(policy.administrators_full_control);
        assert!(policy.system_full_control);
        assert!(policy.mandatory_no_write_up);
        assert!(policy.mutual_peer_verification);
    }
}
