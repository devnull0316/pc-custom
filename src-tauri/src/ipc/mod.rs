//! Elevated-helper protocol boundary.
//!
//! The default Task 2 build is per-user, `asInvoker`, and contains neither an
//! elevated helper executable nor administrative actions. `DenyAllElevatedActions`
//! therefore remains the only production allowlist in this task. The next task
//! implements the signed one-shot named-pipe helper against this frozen contract.

pub mod contract;
pub mod validation;

pub use contract::{
    nonce_to_lower_hex, BooleanParameter, ChoiceParameter, ElevatedActionRequest,
    ElevatedRequestEnvelope, FileIdentity, IntegrityLevel, MandatoryIntegrity,
    OneShotNamedPipePolicy, PeerEvidence, PeerExpectation, TypedActionParameters,
    TypedParameter, TypedParameterValue, UnsignedParameter, ValidatedElevatedRequest, ValidatedPeer,
    IPC_PROTOCOL_VERSION, MAX_ENVELOPE_BYTES, NONCE_BYTES, ONE_SHOT_NAMED_PIPE_POLICY,
};
pub use validation::{
    validate_peer_evidence, validate_request_envelope, DenyAllElevatedActions,
    ElevatedActionAllowlist, RequestValidationContext, ValidationError,
};
