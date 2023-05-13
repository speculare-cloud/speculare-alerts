use serde::{Deserialize, Serialize};

pub mod config;
pub mod impls;

/// Enum used to hold either i32, String or Option<String> (from CDC)
///
/// Using untagged to give serde the opportinity to try match without a structure.
#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum Thing {
    Boolean(bool),
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