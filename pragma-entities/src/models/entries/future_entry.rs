use crate::dto::entry as dto;
use crate::models::DieselResult;
use bigdecimal::BigDecimal;
use diesel::dsl::sql;
use diesel::internal::derives::multiconnection::chrono::NaiveDateTime;
use diesel::BoolExpressionMethods;
use diesel::{
    AsChangeset, ExpressionMethods, Insertable, PgConnection, PgTextExpressionMethods, QueryDsl,
    Queryable, RunQueryDsl, Selectable, SelectableHelper,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::schema::{future_entries, starkex_future_entries};

#[derive(Serialize, Queryable, Selectable)]
#[diesel(table_name = future_entries)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct FutureEntry {
    pub id: Uuid,
    pub pair_id: String,
    pub publisher: String,
    pub source: String,
    pub timestamp: NaiveDateTime,
    // If expiration_timestamp is None, it means the entry is a perpetual future
    // else it is a regular future entry that will expire at the expiration_timestamp.
    pub expiration_timestamp: Option<NaiveDateTime>,
    pub publisher_signature: String,
    pub price: BigDecimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, Insertable, AsChangeset)]
#[diesel(table_name = future_entries)]
pub struct NewFutureEntry {
    pub pair_id: String,
    pub publisher: String,
    pub source: String,
    pub timestamp: NaiveDateTime,
    pub expiration_timestamp: Option<NaiveDateTime>,
    pub publisher_signature: String,
    pub price: BigDecimal,
}

#[derive(Serialize, Queryable, Selectable)]
#[diesel(table_name = starkex_future_entries)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct StarkexFutureEntry {
    pub id: Uuid,
    pub pair_id: String,
    pub publisher: String,
    pub source: String,
    pub timestamp: NaiveDateTime,
    // If expiration_timestamp is None, it means the entry is a perpetual future
    // else it is a regular future entry that will expire at the expiration_timestamp.
    pub expiration_timestamp: Option<NaiveDateTime>,
    pub publisher_signature: String,
    pub price: BigDecimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, Insertable, AsChangeset)]
#[diesel(table_name = starkex_future_entries)]
pub struct NewStarkexFutureEntry {
    pub pair_id: String,
    pub publisher: String,
    pub source: String,
    pub timestamp: NaiveDateTime,
    pub expiration_timestamp: Option<NaiveDateTime>,
    pub publisher_signature: String,
    pub price: BigDecimal,
}

impl NewStarkexFutureEntry {
    pub fn create_many(
        conn: &mut PgConnection,
        data: Vec<NewStarkexFutureEntry>,
    ) -> DieselResult<Vec<StarkexFutureEntry>> {
        use crate::schema::starkex_future_entries;

        diesel::insert_into(starkex_future_entries::table)
            .values(&data)
            .returning(StarkexFutureEntry::as_returning())
            .on_conflict((
                starkex_future_entries::pair_id,
                starkex_future_entries::source,
                starkex_future_entries::timestamp,
                starkex_future_entries::expiration_timestamp,
            ))
            .do_nothing()
            .get_results(conn)
    }
}

impl FutureEntry {
    pub fn create_one(conn: &mut PgConnection, data: NewFutureEntry) -> DieselResult<FutureEntry> {
        diesel::insert_into(future_entries::table)
            .values(data)
            .returning(FutureEntry::as_returning())
            .get_result(conn)
    }

    pub fn create_many(
        conn: &mut PgConnection,
        data: Vec<NewFutureEntry>,
    ) -> DieselResult<Vec<FutureEntry>> {
        diesel::insert_into(future_entries::table)
            .values(&data)
            .returning(FutureEntry::as_returning())
            .on_conflict((
                future_entries::pair_id,
                future_entries::source,
                future_entries::timestamp,
                future_entries::expiration_timestamp,
            ))
            // TODO(akhercha): We are loosing some data currently because of duplicates.
            // It happens because we don't have enough precision in the timestamp (in s, not ms).
            // So we have multiple price for the same timestamp.
            .do_nothing()
            .get_results(conn)
    }

    pub fn exists(conn: &mut PgConnection, pair_id: String) -> DieselResult<bool> {
        diesel::select(diesel::dsl::exists(
            future_entries::table.filter(future_entries::pair_id.eq(pair_id)),
        ))
        .get_result(conn)
    }

    pub fn get_by_pair_id(conn: &mut PgConnection, pair_id: String) -> DieselResult<FutureEntry> {
        future_entries::table
            .filter(future_entries::pair_id.eq(pair_id))
            .select(FutureEntry::as_select())
            .get_result(conn)
    }

    pub fn with_filters(
        conn: &mut PgConnection,
        filters: dto::EntriesFilter,
    ) -> DieselResult<Vec<FutureEntry>> {
        let mut query = future_entries::table.into_boxed::<diesel::pg::Pg>();

        if let Some(pair_id) = filters.pair_id {
            query = query.filter(future_entries::pair_id.eq(pair_id));
        }

        if let Some(publisher_contains) = filters.publisher_contains {
            query =
                query.filter(future_entries::publisher.ilike(format!("%{}%", publisher_contains)));
        }

        query
            .select(FutureEntry::as_select())
            .load::<FutureEntry>(conn)
    }

    pub fn get_existing_pairs(
        conn: &mut PgConnection,
        searched_pairs: Vec<String>,
    ) -> DieselResult<Vec<String>> {
        future_entries::table
            .filter(future_entries::pair_id.eq_any(searched_pairs))
            .select(future_entries::pair_id)
            .distinct()
            .load::<String>(conn)
    }

    pub fn get_existing_perp_pairs(
        conn: &mut PgConnection,
        searched_pairs: Vec<String>,
    ) -> DieselResult<Vec<String>> {
        future_entries::table
            .filter(future_entries::pair_id.eq_any(searched_pairs).and(
                future_entries::expiration_timestamp.is_null().or(
                    future_entries::expiration_timestamp.eq(sql("timestamp '1970-01-01 00:00:00'")),
                ),
            ))
            .select(future_entries::pair_id)
            .distinct()
            .load::<String>(conn)
    }
}

impl From<NewFutureEntry> for NewStarkexFutureEntry {
    fn from(entry: NewFutureEntry) -> NewStarkexFutureEntry {
        NewStarkexFutureEntry {
            pair_id: entry.pair_id,
            publisher: entry.publisher,
            source: entry.source,
            timestamp: entry.timestamp,
            expiration_timestamp: entry.expiration_timestamp,
            publisher_signature: entry.publisher_signature,
            price: entry.price,
        }
    }
}
