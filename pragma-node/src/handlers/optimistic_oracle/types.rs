use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::fmt;
use strum::Display;
use utoipa::ToSchema;

#[derive(Debug, Serialize, Display, ToSchema)]
pub enum Status {
    Active,
    Disputed,
    Settled,
}

#[derive(Debug, Serialize, ToSchema)]
pub enum SettlementResolution {
    True,
    False,
    Undefined,
}

impl From<Option<bool>> for SettlementResolution {
    fn from(res: Option<bool>) -> Self {
        match res {
            Some(true) => SettlementResolution::True,
            Some(false) => SettlementResolution::False,
            None => SettlementResolution::Undefined,
        }
    }
}
impl From<bool> for SettlementResolution {
    fn from(res: bool) -> Self {
        match res {
            true => SettlementResolution::True,
            false => SettlementResolution::False,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GetAssertionsParams {
    pub status: Option<String>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Assertion {
    pub assertion_id: String,
    pub claim: String,
    pub bond: BigDecimal,
    pub expiration_time: NaiveDateTime,
    pub identifier: String,
    pub status: Status,
    pub timestamp: NaiveDateTime,
    pub currency: String,
}

impl fmt::Display for Assertion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Assertion {{ assertion_id: {}, claim: {}, bond: {}, expiration_time: {}, identifier: {}, status: {}, timestamp: {}, currency: {} }}",
            self.assertion_id,
            self.claim,
            self.bond,
            self.expiration_time,
            self.identifier,
            self.status,
            self.timestamp,
            self.currency
        )
    }
}
#[derive(Debug, Serialize, ToSchema)]
pub struct ResolvedAssertion {
    pub assertion: Assertion,
    pub settled_address: String,
    pub settlement_resolution: SettlementResolution,
    pub settled_at: NaiveDateTime,
    pub settle_caller: String,
    pub disputed: bool,
    pub settlement_tx: String,
    pub dispute_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DisputedAssertion {
    pub assertion: Assertion,
    pub disputer: String,
    pub disputed_at: NaiveDateTime,
    pub dispute_id: String,
    pub disputed_tx: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GetAssertionsResponse {
    pub assertions: Vec<Assertion>,
    pub total_count: i64,
    pub current_page: u32,
    pub total_pages: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AssertionDetails {
    pub assertion: Assertion,
    pub domain_id: String,
    pub asserter: String,
    pub disputer: String,
    pub disputed: bool,
    pub dispute_id: String,
    pub callback_recipient: String,
    pub caller: String,
    pub settled: bool,
    pub settle_caller: String,
    pub settlement_resolution: SettlementResolution,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GetDisputedAssertionsParams {
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GetDisputedAssertionsResponse {
    pub disputed_assertions: Vec<DisputedAssertion>,
    pub total_count: usize,
    pub current_page: u32,
    pub total_pages: u32,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GetResolvedAssertionsParams {
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GetResolvedAssertionsResponse {
    pub resolved_assertions: Vec<ResolvedAssertion>,
    pub total_count: usize,
    pub current_page: u32,
    pub total_pages: u32,
}
