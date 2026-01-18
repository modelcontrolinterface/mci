use diesel::{
    prelude::PgConnection,
    r2d2::{ConnectionManager, Pool, PooledConnection},
};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use std::error::Error;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

pub type PgPool = Pool<ConnectionManager<PgConnection>>;
pub type DbConnection = PooledConnection<ConnectionManager<PgConnection>>;

pub fn create_pool(database_url: &str) -> PgPool {
    let manager = ConnectionManager::<PgConnection>::new(database_url);

    Pool::builder()
        .max_size(10)
        .build(manager)
        .expect("Failed to create pool")
}

pub fn run_migrations(conn: &mut PgConnection) -> Result<(), Box<dyn Error + Send + Sync>> {
    conn.run_pending_migrations(MIGRATIONS)?;

    Ok(())
}
