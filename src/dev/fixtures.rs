use std::str::FromStr;

use uuid::Uuid;

pub fn global_ws_id() -> Uuid {
    let id: Uuid = Uuid::from_str("10000000-0000-0000-0000-000000000001").unwrap();
    id
}

pub fn second_ws_id() -> Uuid {
    let id: Uuid = Uuid::from_str("10000000-0000-0000-0000-000000000002").unwrap();
    id
}

pub fn root_user_id() -> Uuid {
    let id: Uuid = Uuid::from_str("00000000-0000-0000-0000-000000000001").unwrap();
    id
}
