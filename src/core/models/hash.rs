use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::core::error::{CoreError, CoreResult};
use crate::store::entities::hash::Sha256Hash;

pub trait Hashable {
    fn hash_sha256(&self) -> CoreResult<Sha256Hash>;
}

impl<T: Serialize> Hashable for T {
    fn hash_sha256(&self) -> CoreResult<Sha256Hash> {
        let bytes = bincode::serialize(&self)?;
        let hash = Sha256::digest(bytes);
        let data: [u8; 32] = hash.try_into()?;
        Ok(Sha256Hash::new(data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Sample {
        name: String,
        value: u32,
    }

    #[test]
    fn test_hash_sha256_is_deterministic() {
        let sample = Sample {
            name: "oxideauth".to_string(),
            value: 42,
        };
        let h1 = sample.hash_sha256().expect("hash should succeed");
        let h2 = sample.hash_sha256().expect("hash should succeed");
        assert_eq!(h1, h2);
        assert_eq!(h1.bytes().len(), 32);
    }

    #[test]
    fn test_hash_sha256_differs_for_different_input() {
        let a = Sample {
            name: "a".to_string(),
            value: 1,
        };
        let b = Sample {
            name: "b".to_string(),
            value: 1,
        };
        assert_ne!(a.hash_sha256().unwrap(), b.hash_sha256().unwrap());
    }

    #[test]
    fn test_sha256_hash_display_is_hex() {
        let data = [7u8; 32];
        let hash = Sha256Hash::new(data);
        let s = hash.to_string();
        assert_eq!(s.len(), 64, "hex encoding of 32 bytes must be 64 chars");
        assert_eq!(s, hex::encode(data));
    }

    #[test]
    fn test_sha256_hash_from_slice() {
        let bytes = [0xABu8; 32];
        let hash = Sha256Hash::from(&bytes[..]);
        assert_eq!(hash.bytes(), &bytes);
    }

    #[test]
    fn test_sha256_hash_gen_rand_produces_distinct_values() {
        let a = Sha256Hash::gen_rand();
        let b = Sha256Hash::gen_rand();
        assert_ne!(a, b);
    }
}
