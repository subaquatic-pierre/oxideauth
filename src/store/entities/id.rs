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

#[cfg(test)]
mod tests {
    use super::*;
    use sea_query::Value as SeaValue;

    #[test]
    fn test_db_id_deref_to_uuid() {
        // -- Setup
        let uuid = Uuid::new_v4();
        let id = DbId(uuid);

        // -- Execute & Assert
        assert_eq!(*id, uuid, "Deref should expose the inner Uuid");
        assert_eq!(id.to_string(), uuid.to_string());
    }

    #[test]
    fn test_db_id_display() {
        // -- Setup
        let uuid = Uuid::new_v4();
        let id = DbId(uuid);

        // -- Assert
        assert_eq!(format!("{id}"), uuid.to_string());
    }

    #[test]
    fn test_db_id_from_uuid() {
        // -- Setup
        let uuid = Uuid::new_v4();

        // -- Execute
        let id = DbId::from(uuid);
        let id_ref = DbId::from(&uuid);

        // -- Assert
        assert_eq!(id.0, uuid);
        assert_eq!(id_ref.0, uuid);
    }

    #[test]
    fn test_db_id_into_uuid() {
        // -- Setup
        let uuid = Uuid::new_v4();
        let id = DbId(uuid);

        // -- Execute
        let owned: Uuid = id.into();
        let borrowed: Uuid = (&id).into();

        // -- Assert
        assert_eq!(owned, uuid);
        assert_eq!(borrowed, uuid);
    }

    #[test]
    fn test_db_id_into_sea_value() {
        // -- Setup
        let uuid = Uuid::new_v4();
        let id = DbId(uuid);

        // -- Execute
        let v: SeaValue = id.into();

        // -- Assert
        assert_eq!(v, SeaValue::Uuid(Some(uuid)));
    }

    #[test]
    fn test_db_id_default_is_nil() {
        // -- Execute
        let id = DbId::default();

        // -- Assert
        assert_eq!(id.0, Uuid::nil());
    }

    #[test]
    fn test_db_id_partial_eq() {
        // -- Setup
        let uuid = Uuid::new_v4();

        // -- Assert
        assert_eq!(DbId(uuid), DbId(uuid));
        assert_ne!(DbId(uuid), DbId(Uuid::new_v4()));
    }
}
