use diesel::{
    sql_types::{Float8, Timestamp},
    *,
};
use serde::{Deserialize, Serialize};

pub mod config;
pub mod impls;

/// Enum used to hold either i32, String or Option<String> (from CDC)
///
/// Using untagged to give serde the opportinity to try match without a structure.
#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum Thing {
    Number(i32),
    String(String),
    OptionString(Option<String>),
}

/// Enum to represente the kind of the CdcChange message
///
/// Convert to lowercase to match with the message "update", "insert"
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CdcKind {
    Update,
    Insert,
    Delete,
}

/// Structure holding the info we need from the WebSocket
#[derive(Serialize, Deserialize, Debug)]
pub struct CdcChange {
    columnnames: Vec<String>,
    columnvalues: Vec<Thing>,
    pub kind: CdcKind,
    table: String,
}

/// Struct to hold the return from the sql_query for percentage query
#[derive(QueryableByName, Debug)]
pub struct PctDTORaw {
    #[diesel(sql_type = Float8)]
    pub numerator: f64,
    #[diesel(sql_type = Float8)]
    pub divisor: f64,
    #[diesel(sql_type = Timestamp)]
    pub time: chrono::NaiveDateTime,
}

/// Struct to hold the return from the sql_query for absolute query
#[derive(QueryableByName, Debug)]
pub struct AbsDTORaw {
    #[diesel(sql_type = Float8)]
    pub value: f64,
    #[diesel(sql_type = Timestamp)]
    pub time: chrono::NaiveDateTime,
}

/// Constant list of disallowed statement in the SQL query to avoid somthg bad
pub const DISALLOWED_STATEMENT: &[&str] = &[
    "DELETE",
    "UPDATE",
    "INSERT",
    //"CREATE", => conflict with created_at, TODO FIX LATER
    "ALTER",
    "DROP",
    "TRUNCATE",
    "GRANT",
    "REVOKE",
    "BEGIN",
    "COMMIT",
    "SAVEPOINT",
    "ROLLBACK",
];
