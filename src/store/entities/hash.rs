use std::fmt::{self, Debug, Display};
use std::ops::{Deref, DerefMut};

use rand::Rng;
use sea_query::{Nullable, Value as SeaValue};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use sqlx::{decode::Decode, encode::Encode, error::BoxDynError, Type};
use tracing::{error, warn};

use crate::store::error::{StoreError, StoreResult};

#[derive(Deserialize, PartialEq, Eq, Clone, Serialize, Default)]
pub struct Sha256Hash {
    inner: [u8; 32],
}

impl Sha256Hash {
    pub fn new(data: [u8; 32]) -> Self {
        Self { inner: data }
    }

    pub fn gen_rand() -> Self {
        let mut rng = rand::thread_rng();
        let mut v = [0_u8; 32];

        for i in 0..32 {
            v[i] = rng.r#gen()
        }

        Self { inner: v }
    }

    pub fn bytes(&self) -> &[u8; 32] {
        &self.inner
    }
}

impl Deref for Sha256Hash {
    type Target = [u8; 32];

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Sha256Hash {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl From<&[u8]> for Sha256Hash {
    fn from(value: &[u8]) -> Self {
        let mut data = [0u8; 32];
        for i in 0..32 {
            data[i] = value[i];
        }

        Self { inner: data }
    }
}

impl Display for Sha256Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s: String = hex::encode(**self);
        write!(f, "{s}")
    }
}

impl Debug for Sha256Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
        // f.debug_struct("Sha256Hash")
        //     .field("inner", &self.to_string())
        //     .finish()
    }
}

impl Nullable for Sha256Hash {
    fn null() -> SeaValue {
        SeaValue::Bytes(None)
    }
}

impl From<Sha256Hash> for SeaValue {
    fn from(value: Sha256Hash) -> Self {
        SeaValue::Bytes(Some(value.inner.to_vec()))
    }
}
impl<'r> sqlx::decode::Decode<'r, sqlx::Postgres> for Sha256Hash {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> std::result::Result<Self, sqlx::error::BoxDynError> {
        let res = <&[u8] as sqlx::decode::Decode<sqlx::Postgres>>::decode(value)?;
        Ok(res.into())
    }
}

impl<'q> sqlx::encode::Encode<'q, sqlx::Postgres> for Sha256Hash {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> std::result::Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <&[u8] as sqlx::encode::Encode<sqlx::Postgres>>::encode(&self.inner, buf)
    }
}

impl sqlx::Type<sqlx::Postgres> for Sha256Hash {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("BYTEA")
    }
}

// -----------------------------------------------------------------------------
// endregion: --- sqlx Trait Implementations

#[cfg(test)]
mod tests {
    use super::*;
    use sea_query::{Nullable, Value as SeaValue};
    use std::collections::HashSet;

    #[test]
    fn test_sha256_new_and_bytes() {
        // -- Setup
        let data = [0xABu8; 32];

        // -- Execute
        let hash = Sha256Hash::new(data);

        // -- Assert
        assert_eq!(hash.bytes(), &data);
        assert_eq!(*hash, data);
    }

    #[test]
    fn test_sha256_gen_rand_length_and_uniqueness() {
        // -- Execute
        let mut seen = HashSet::with_capacity(1000);
        for _ in 0..1000 {
            let hash = Sha256Hash::gen_rand();
            assert_eq!(hash.bytes().len(), 32, "gen_rand must produce 32 bytes");
            assert!(
                seen.insert(hash.bytes().to_vec()),
                "Collision detected in gen_rand hashes"
            );
        }
        assert_eq!(seen.len(), 1000, "All 1000 generated hashes should be unique");
    }

    #[test]
    fn test_sha256_deref_and_deref_mut() {
        // -- Setup
        let mut hash = Sha256Hash::new([0u8; 32]);

        // -- Execute
        hash[0] = 42;

        // -- Assert
        assert_eq!(hash.bytes()[0], 42);
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_sha256_display_hex_lowercase() {
        // -- Setup
        let zero_hash = Sha256Hash::new([0u8; 32]);
        let ab_hash = Sha256Hash::new([0xABu8; 32]);

        // -- Assert
        assert_eq!(zero_hash.to_string(), "0".repeat(64));
        assert_eq!(ab_hash.to_string(), "ab".repeat(32));
        assert_eq!(ab_hash.to_string().len(), 64);
    }

    #[test]
    fn test_sha256_from_slice() {
        // -- Setup
        let data = [7u8; 32];
        let slice: &[u8] = &data;

        // -- Execute
        let hash = Sha256Hash::from(slice);

        // -- Assert
        assert_eq!(hash.bytes(), &data);
    }

    #[test]
    fn test_sha256_nullable_null() {
        // -- Execute
        let null = <Sha256Hash as Nullable>::null();

        // -- Assert
        assert_eq!(null, SeaValue::Bytes(None));
    }

    #[test]
    fn test_sha256_into_sea_value() {
        // -- Setup
        let data = [9u8; 32];
        let hash = Sha256Hash::new(data);

        // -- Execute
        let v: SeaValue = hash.into();

        // -- Assert
        assert_eq!(v, SeaValue::Bytes(Some(data.to_vec())));
    }
}
