use diesel::{
    prelude::PgConnection,
    r2d2::{ConnectionManager, Pool, PooledConnection},
};

pub type PgPool = Pool<ConnectionManager<PgConnection>>;
pub type DbConnection = PooledConnection<ConnectionManager<PgConnection>>;

pub fn create_pool(database_url: &str) -> PgPool {
    let manager = ConnectionManager::<PgConnection>::new(database_url);

    Pool::builder()
        .max_size(10)
        .build(manager)
        .expect("Failed to create pool")
}
