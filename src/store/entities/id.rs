use std::{fmt::Display, ops::Deref};

use serde::Deserialize;
use sqlx::{FromRow, Type};
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Debug, FromRow, Type, Hash, Default)]
#[sqlx(transparent)]
pub struct DbId(pub Uuid);

impl Deref for DbId {
    type Target = Uuid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for DbId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<DbId> for sea_query::Value {
    fn from(value: DbId) -> Self {
        sea_query::Value::Uuid(Some(value.0))
    }
}

impl From<Uuid> for DbId {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

impl From<&Uuid> for DbId {
    fn from(value: &Uuid) -> Self {
        Self(value.clone())
    }
}

impl From<DbId> for Uuid {
    fn from(value: DbId) -> Self {
        value.0
    }
}

impl From<&DbId> for Uuid {
    fn from(value: &DbId) -> Self {
        value.0
    }
}
