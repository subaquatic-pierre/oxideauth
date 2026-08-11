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
