use crate::monitoring::QueryType;
use crate::utils::DISALLOWED_STATEMENT;

use regex::Regex;
use sproot::{apierrors::ApiError, models::Alerts};

lazy_static::lazy_static! {
    static ref INTERVAL_RGX: Regex = {
        match Regex::new(r"(\d+)([a-zA-Z' '])|([m,h,d,minutes,hours,days,minute,hour,day])") {
            Ok(reg) => reg,
            Err(e) => {
                error!("Cannot build the Regex to validate INTERVAL: {}", e);
                std::process::exit(1);
            }
        }
    };
}

pub trait AlertsQuery {
    fn construct_query(&self) -> Result<(String, QueryType), ApiError>;
}

impl AlertsQuery for Alerts {
    /// Create the query for the Alert and get the QueryType
    fn construct_query(&self) -> Result<(String, QueryType), ApiError> {
        // Split the lookup String from the alert for analysis
        let lookup_parts: Vec<&str> = self.lookup.split(' ').collect();

        // Assert that we have enough parameters
        if lookup_parts.len() < 5 {
            return Err(ApiError::ServerError(String::from("query: the lookup query is invalid, define as follow: [aggr] [mode] [timeframe] of [table] {over} {table}")));
        }

        // Determine the mode of the query it's for now, either Pct or Abs
        let req_mode = match lookup_parts[1] {
            "pct" => QueryType::Pct,
            "abs" => QueryType::Abs,
            _ => {
                return Err(ApiError::ServerError(format!(
                    "query: mode {} is invalid. Valid are: pct, abs.",
                    lookup_parts[1]
                )));
            }
        };

        // If we're in mode Pct, we need more than 5 parts
        if req_mode == QueryType::Pct && lookup_parts.len() != 7 {
            return Err(ApiError::ServerError(String::from(
                "query: lookup defined as mode pct but missing values, check usage.",
            )));
        }

        // The type of the query this is pretty much the aggregation function Postgres is going to use
        let req_aggr = lookup_parts[0];
        // Assert that req_type is correct (avg, sum, min, max, count)
        if !["avg", "sum", "min", "max", "count"].contains(&req_aggr) {
            return Err(ApiError::ServerError(String::from(
                "query: aggr is invalid. Valid are: avg, sum, min, max, count.",
            )));
        }

        // Get the timing of the query, that is the interval range
        let req_time = lookup_parts[2];
        // Assert that req_time is correctly formatted (Regex?)
        if !INTERVAL_RGX.is_match(req_time) {
            return Err(ApiError::ServerError(String::from(
                "query: req_time is not correctly formatted (doesn't pass regex).",
            )));
        }

        // This is the columns we ask for in the first place, this value is mandatory
        let req_one = lookup_parts[4];

        // Construct the SELECT part of the query
        let mut pg_select = String::new();
        let select_cols = req_one.split(',');
        for col in select_cols {
            // We're casting everything to float8 to handle pretty much any type we need
            pg_select.push_str(&format!("{}({})::float8 + ", req_aggr, col));
        }
        // Remove the last " + "
        pg_select.drain(pg_select.len() - 3..pg_select.len());
        // Based on the mode, we might need to do some different things
        match req_mode {
            // For pct we need to define numerator and divisor.
            QueryType::Pct => {
                // req_two only exists if req_mode == Pct
                let req_two = lookup_parts[6];

                pg_select.push_str(" as numerator, ");
                let select_cols = req_two.split(',');
                for col in select_cols {
                    pg_select.push_str(&format!("{}({})::float8 + ", req_aggr, col));
                }
                pg_select.drain(pg_select.len() - 3..pg_select.len());
                pg_select.push_str(" as divisor");
            }
            // For abs we just need to define the addition of all columns as value
            QueryType::Abs => {
                pg_select.push_str(" as value");
            }
        }

        // Optional where clause
        // Allow us to add a WHERE condition to the query if needed
        let mut pg_where = String::new();
        if let Some(where_clause) = self.where_clause.as_ref() {
            pg_where.push_str(" AND ");
            pg_where.push_str(where_clause);
        }

        // Base of the query, we plug every pieces together here
        let query = format!("SELECT time_bucket('{0}', created_at) as time, {1} FROM {2} WHERE host_uuid=$1 AND created_at > now() at time zone 'utc' - INTERVAL '{0}' {3} GROUP BY time ORDER BY time DESC", req_time, pg_select, self.table, pg_where);

        trace!("Query[{:?}] is {}", req_mode, &query);

        // Assert that we don't have any malicious statement in the query
        // by changing it to uppercase and checking against our list of banned statement.
        let tmp_query = query.to_uppercase();
        for statement in DISALLOWED_STATEMENT {
            if tmp_query.contains(statement) {
                return Err(ApiError::ServerError(format!(
                    "Alert {} for host_uuid {:.6} contains disallowed statement \"{}\"",
                    self.name, self.host_uuid, statement
                )));
            }
        }

        Ok((query, req_mode))
    }
}
