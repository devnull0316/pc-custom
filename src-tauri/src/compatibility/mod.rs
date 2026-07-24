//! The only module allowed to identify and classify the Windows build.

mod catalog;
mod os_identity;

pub use catalog::{CompatibilityCatalog, CompatibilityDecision, CompatibilityMode};
pub use os_identity::{
    Architecture, OsIdentity, OsIdentityError, OsIdentityErrorKind, OsIdentitySource,
};
