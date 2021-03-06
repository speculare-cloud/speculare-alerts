pub mod abs;
pub mod pct;

/// Represente the type of the Query an alert ask for
#[derive(Debug, PartialEq, Clone)]
pub enum QueryType {
    Pct,
    Abs,
}
