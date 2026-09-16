use knowledge_store::{MIGRATOR, jobs::JobRepository};
use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test]
async fn leases_exclude_reclaim_and_fence_stale_owner_even_when_owner_is_reused(pool: PgPool) {
    MIGRATOR.run(&pool).await.unwrap();
    let workspace = Uuid::now_v7();
    let job = Uuid::now_v7();
    let owner = Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces(id,name_ciphertext,name_nonce,name_key_version,wrapped_key_ciphertext,wrapped_key_nonce,wrapped_key_version) VALUES($1,'cipher',$2,1,'cipher',$2,1)").bind(workspace).bind([0u8;12].as_slice()).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO jobs(id,workspace_id,job_type,payload_ciphertext,payload_nonce,payload_key_version,status) VALUES($1,$2,'export','cipher',$3,1,'queued')").bind(job).bind(workspace).bind([0u8;12].as_slice()).execute(&pool).await.unwrap();
    let lease = JobRepository::claim(&pool, owner, 60)
        .await
        .unwrap()
        .unwrap();
    assert!(
        JobRepository::claim(&pool, Uuid::now_v7(), 60)
            .await
            .unwrap()
            .is_none()
    );
    sqlx::query("UPDATE jobs SET lease_expires_at=now()-interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(!JobRepository::renew(&pool, &lease, 60).await.unwrap());
    let newer = JobRepository::claim(&pool, owner, 60)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(newer.attempts, 2);
    let mut tx = pool.begin().await.unwrap();
    assert!(!JobRepository::finish(&mut tx, &lease).await.unwrap());
    tx.commit().await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert!(JobRepository::retry(&mut tx, &newer, 0).await.unwrap());
    tx.commit().await.unwrap();
    let final_lease = JobRepository::claim(&pool, owner, 60)
        .await
        .unwrap()
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert!(JobRepository::finish(&mut tx, &final_lease).await.unwrap());
    assert!(!JobRepository::finish(&mut tx, &newer).await.unwrap());
    tx.commit().await.unwrap();
    assert!(
        JobRepository::claim(&pool, owner, 60)
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test]
async fn expired_fifth_attempt_is_reclaimed_only_for_fenced_terminalization(pool: PgPool) {
    MIGRATOR.run(&pool).await.unwrap();
    let workspace = Uuid::now_v7();
    let job = Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces(id,name_ciphertext,name_nonce,name_key_version,wrapped_key_ciphertext,wrapped_key_nonce,wrapped_key_version) VALUES($1,'cipher',$2,1,'cipher',$2,1)").bind(workspace).bind([0u8;12].as_slice()).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO jobs(id,workspace_id,job_type,payload_ciphertext,payload_nonce,payload_key_version,status,attempts,lease_owner,lease_expires_at) VALUES($1,$2,'export','cipher',$3,1,'running',5,$4,now()-interval '1 second')").bind(job).bind(workspace).bind([0u8;12].as_slice()).bind(Uuid::now_v7()).execute(&pool).await.unwrap();

    let terminal = JobRepository::claim(&pool, Uuid::now_v7(), 60)
        .await
        .unwrap()
        .expect("expired final attempt must not remain stuck forever");
    assert!(
        !terminal.should_execute(),
        "a sixth remote execution is forbidden"
    );
    let mut tx = pool.begin().await.unwrap();
    assert!(JobRepository::retry(&mut tx, &terminal, 0).await.unwrap());
    tx.commit().await.unwrap();
    let status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id=$1")
        .bind(job)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "failed");
    assert!(
        JobRepository::claim(&pool, Uuid::now_v7(), 60)
            .await
            .unwrap()
            .is_none()
    );
}
