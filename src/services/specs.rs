use crate::{db::DbConnection, models::Spec, schema::specs};
use diesel::prelude::*;

pub fn get_spec(conn: &mut DbConnection, spec_id: &str) -> QueryResult<Spec> {
    specs::table
        .find(spec_id)
        .select(Spec::as_select())
        .first(conn)
}
