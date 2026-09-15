use knowledge_store::Store;
use sqlx::PgPool;

#[sqlx::test(migrations = false)]
async fn store_applies_migrations_idempotently(pool: PgPool) {
    let store = Store::from_pool(pool);

    store.migrate().await.expect("first migration should work");
    store
        .migrate()
        .await
        .expect("re-running migrations should be safe");

    let product_tables: i64 = sqlx::query_scalar(
        "SELECT count(*)
           FROM information_schema.tables
          WHERE table_schema = 'public'
            AND table_name <> '_sqlx_migrations'",
    )
    .fetch_one(store.pool())
    .await
    .expect("table count should be readable");

    assert_eq!(product_tables, 21);
}
