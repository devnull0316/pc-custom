use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Fingerprint(pub [u8; 32]);

impl Fingerprint {
    pub fn of_bytes(bytes: &[u8]) -> Self {
        let mut digest = Sha256::new();
        digest.update(bytes);
        Self(digest.finalize().into())
    }

    pub fn of_parts<'a>(parts: impl IntoIterator<Item = &'a [u8]>) -> Self {
        let mut digest = Sha256::new();
        for part in parts {
            digest.update((part.len() as u64).to_le_bytes());
            digest.update(part);
        }
        Self(digest.finalize().into())
    }

    pub fn to_hex(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut output = String::with_capacity(64);
        for byte in self.0 {
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_keeps_parts_unambiguous() {
        let left = Fingerprint::of_parts([b"ab".as_slice(), b"c".as_slice()]);
        let right = Fingerprint::of_parts([b"a".as_slice(), b"bc".as_slice()]);
        assert_ne!(left, right);
        assert_eq!(left.to_hex().len(), 64);
    }
}
