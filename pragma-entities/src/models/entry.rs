use diesel::{ExpressionMethods, Insertable, PgTextExpressionMethods, QueryDsl, Queryable, RunQueryDsl, Selectable, SelectableHelper, PgConnection};
use bigdecimal::BigDecimal;
use diesel::internal::derives::multiconnection::chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::schema::entries;
use crate::dto::entry as dto;
use crate::models::DieselResult;

#[derive(Serialize, Queryable, Selectable)]
#[diesel(table_name = entries)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Entry {
    pub id: Uuid,
    pub pair_id: String,
    pub publisher: String,
    pub source: String,
    pub timestamp: NaiveDateTime,
    pub price: BigDecimal,
}

#[derive(Deserialize, Insertable)]
#[diesel(table_name = entries)]
pub struct NewEntry {
    pub pair_id: String,
    pub publisher: String,
    pub source: String,
    pub timestamp: NaiveDateTime,
    pub price: BigDecimal,
}

impl Entry {

    pub fn create_one(conn: &mut PgConnection, data: NewEntry) -> DieselResult<Entry> {
        diesel::insert_into(entries::table)
            .values(data)
            .returning(Entry::as_returning())
            .get_result(conn)
    }

    pub fn create_many(conn: &mut PgConnection, data: Vec<NewEntry>) -> DieselResult<Vec<Entry>> {
        diesel::insert_into(entries::table)
            .values(data)
            .returning(Entry::as_returning())
            .get_results(conn)
    }

    pub fn get_by_pair_id(conn: &mut PgConnection, pair_id: String) -> DieselResult<Entry> {
        entries::table
            .filter(entries::pair_id.eq(pair_id))
            .select(Entry::as_select())
            .get_result(conn)
    }

    pub fn with_filters(conn: &mut PgConnection, filters: dto::EntriesFilter) -> DieselResult<Vec<Entry>> {
        let mut query = entries::table.into_boxed::<diesel::pg::Pg>();

        if let Some(pair_id) = filters.pair_id {
            query = query.filter(entries::pair_id.eq(pair_id));
        }

        if let Some(publisher_contains) = filters.publisher_contains {
            query = query.filter(entries::publisher.ilike(format!("%{}%", publisher_contains)));
        }

        query.select(Entry::as_select()).load::<Entry>(conn)
    }
}