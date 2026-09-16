use knowledge_store::attachments::lock_object;
use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test]
async fn contended_object_locks_do_not_exhaust_the_database_pool(pool: PgPool) {
    let object = Uuid::now_v7();
    let guard = lock_object(&pool, object).await.unwrap();
    let waiter_count = pool.options().get_max_connections() as usize * 2;
    let mut waiters = Vec::with_capacity(waiter_count);
    for _ in 0..waiter_count {
        let pool = pool.clone();
        waiters.push(tokio::spawn(
            async move { lock_object(&pool, object).await },
        ));
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let query = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&pool),
    )
    .await;
    for waiter in waiters {
        waiter.abort();
    }
    drop(guard);
    assert!(
        query.is_ok(),
        "lock waiters must not retain pool connections while another session owns the lock"
    );
}
