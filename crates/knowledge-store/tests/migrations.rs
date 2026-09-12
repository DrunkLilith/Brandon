use sqlx::PgPool;
use uuid::Uuid;

async fn seed_workspace(pool: &PgPool) -> Uuid {
    let workspace_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO workspaces (
            id, name_ciphertext, name_nonce, name_key_version,
            wrapped_key_ciphertext, wrapped_key_nonce, wrapped_key_version
         ) VALUES ($1, $2, $3, 1, $4, $5, 1)",
    )
    .bind(workspace_id)
    .bind(b"encrypted workspace".as_slice())
    .bind([1_u8; 12].as_slice())
    .bind(b"wrapped key".as_slice())
    .bind([2_u8; 12].as_slice())
    .execute(pool)
    .await
    .expect("workspace fixture should insert");
    workspace_id
}

async fn seed_user(pool: &PgPool, workspace_id: Uuid) -> Uuid {
    let user_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO users (
            id, workspace_id, email_hmac, display_name_ciphertext,
            display_name_nonce, display_name_key_version, status
         ) VALUES ($1, $2, $3, $4, $5, 1, 'active')",
    )
    .bind(user_id)
    .bind(workspace_id)
    .bind([3_u8; 32].as_slice())
    .bind(b"encrypted user".as_slice())
    .bind([4_u8; 12].as_slice())
    .execute(pool)
    .await
    .expect("user fixture should insert");
    user_id
}

async fn seed_group(pool: &PgPool, workspace_id: Uuid, creator_id: Uuid) -> Uuid {
    let group_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO groups (
            id, workspace_id, name_ciphertext, name_nonce, name_key_version, created_by
         ) VALUES ($1, $2, $3, $4, 1, $5)",
    )
    .bind(group_id)
    .bind(workspace_id)
    .bind(b"encrypted group".as_slice())
    .bind([5_u8; 12].as_slice())
    .bind(creator_id)
    .execute(pool)
    .await
    .expect("group fixture should insert");
    group_id
}

async fn seed_space(pool: &PgPool, workspace_id: Uuid, creator_id: Uuid) -> Uuid {
    let space_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO spaces (
            id, workspace_id, name_ciphertext, name_nonce, name_key_version, created_by
         ) VALUES ($1, $2, $3, $4, 1, $5)",
    )
    .bind(space_id)
    .bind(workspace_id)
    .bind(b"encrypted space".as_slice())
    .bind([6_u8; 12].as_slice())
    .bind(creator_id)
    .execute(pool)
    .await
    .expect("space fixture should insert");
    space_id
}

async fn seed_page(pool: &PgPool, workspace_id: Uuid, space_id: Uuid, author_id: Uuid) -> Uuid {
    let page_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO pages (
            id, workspace_id, space_id, stable_key, sort_key,
            title_ciphertext, title_nonce, title_key_version,
            body_ciphertext, body_nonce, body_key_version,
            properties_ciphertext, properties_nonce, properties_key_version,
            version, restricted, created_by, updated_by
         ) VALUES (
            $1, $2, $3, $4, 'a0',
            $5, $6, 1, $7, $8, 1, $9, $10, 1,
            1, false, $11, $11
         )",
    )
    .bind(page_id)
    .bind(workspace_id)
    .bind(space_id)
    .bind(Uuid::now_v7())
    .bind(b"encrypted title".as_slice())
    .bind([7_u8; 12].as_slice())
    .bind(b"encrypted body".as_slice())
    .bind([8_u8; 12].as_slice())
    .bind(b"encrypted properties".as_slice())
    .bind([9_u8; 12].as_slice())
    .bind(author_id)
    .execute(pool)
    .await
    .expect("page fixture should insert");
    page_id
}

#[sqlx::test]
async fn initial_schema_enforces_exactly_one_grant_subject(pool: PgPool) {
    let workspace_id = seed_workspace(&pool).await;
    let user_id = seed_user(&pool, workspace_id).await;
    let group_id = seed_group(&pool, workspace_id, user_id).await;
    let space_id = seed_space(&pool, workspace_id, user_id).await;

    let both_subjects = sqlx::query(
        "INSERT INTO space_grants (
            id, workspace_id, space_id, user_id, group_id, role, granted_by
         ) VALUES ($5, $1, $2, $3, $4, 'viewer', $3)",
    )
    .bind(workspace_id)
    .bind(space_id)
    .bind(user_id)
    .bind(group_id)
    .bind(Uuid::now_v7())
    .execute(&pool)
    .await;

    assert!(both_subjects.is_err());
}

#[sqlx::test]
async fn revision_rows_are_immutable(pool: PgPool) {
    let workspace_id = seed_workspace(&pool).await;
    let user_id = seed_user(&pool, workspace_id).await;
    let space_id = seed_space(&pool, workspace_id, user_id).await;
    let page_id = seed_page(&pool, workspace_id, space_id, user_id).await;
    let revision_id = Uuid::now_v7();

    sqlx::query(
        "INSERT INTO page_revisions (
            id, workspace_id, page_id, version,
            title_ciphertext, title_nonce, title_key_version,
            body_ciphertext, body_nonce, body_key_version,
            properties_ciphertext, properties_nonce, properties_key_version,
            content_hash, author_id
         ) VALUES (
            $1, $2, $3, 1,
            $4, $5, 1, $6, $7, 1, $8, $9, 1, $10, $11
         )",
    )
    .bind(revision_id)
    .bind(workspace_id)
    .bind(page_id)
    .bind(b"encrypted title".as_slice())
    .bind([10_u8; 12].as_slice())
    .bind(b"encrypted body".as_slice())
    .bind([11_u8; 12].as_slice())
    .bind(b"encrypted properties".as_slice())
    .bind([12_u8; 12].as_slice())
    .bind([13_u8; 32].as_slice())
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("revision fixture should insert");

    let update = sqlx::query("UPDATE page_revisions SET version = 2 WHERE id = $1")
        .bind(revision_id)
        .execute(&pool)
        .await;
    let delete = sqlx::query("DELETE FROM page_revisions WHERE id = $1")
        .bind(revision_id)
        .execute(&pool)
        .await;

    assert!(update.is_err());
    assert!(delete.is_err());
}

#[sqlx::test]
async fn page_parent_must_share_workspace_and_space(pool: PgPool) {
    let workspace_id = seed_workspace(&pool).await;
    let user_id = seed_user(&pool, workspace_id).await;
    let first_space = seed_space(&pool, workspace_id, user_id).await;
    let second_space = seed_space(&pool, workspace_id, user_id).await;
    let parent_id = seed_page(&pool, workspace_id, first_space, user_id).await;

    let child = sqlx::query(
        "INSERT INTO pages (
            id, workspace_id, space_id, parent_id, stable_key, sort_key,
            title_ciphertext, title_nonce, title_key_version,
            body_ciphertext, body_nonce, body_key_version,
            properties_ciphertext, properties_nonce, properties_key_version,
            version, restricted, created_by, updated_by
         ) VALUES (
            $1, $2, $3, $4, $5, 'a1',
            $6, $7, 1, $8, $9, 1, $10, $11, 1,
            1, false, $12, $12
         )",
    )
    .bind(Uuid::now_v7())
    .bind(workspace_id)
    .bind(second_space)
    .bind(parent_id)
    .bind(Uuid::now_v7())
    .bind(b"encrypted title".as_slice())
    .bind([14_u8; 12].as_slice())
    .bind(b"encrypted body".as_slice())
    .bind([15_u8; 12].as_slice())
    .bind(b"encrypted properties".as_slice())
    .bind([16_u8; 12].as_slice())
    .bind(user_id)
    .execute(&pool)
    .await;

    assert!(child.is_err());
}

#[sqlx::test]
async fn page_version_must_be_positive(pool: PgPool) {
    let workspace_id = seed_workspace(&pool).await;
    let user_id = seed_user(&pool, workspace_id).await;
    let space_id = seed_space(&pool, workspace_id, user_id).await;

    let page = sqlx::query(
        "INSERT INTO pages (
            id, workspace_id, space_id, stable_key, sort_key,
            title_ciphertext, title_nonce, title_key_version,
            body_ciphertext, body_nonce, body_key_version,
            properties_ciphertext, properties_nonce, properties_key_version,
            version, restricted, created_by, updated_by
         ) VALUES (
            $1, $2, $3, $4, 'a0',
            $5, $6, 1, $7, $8, 1, $9, $10, 1,
            0, false, $11, $11
         )",
    )
    .bind(Uuid::now_v7())
    .bind(workspace_id)
    .bind(space_id)
    .bind(Uuid::now_v7())
    .bind(b"encrypted title".as_slice())
    .bind([17_u8; 12].as_slice())
    .bind(b"encrypted body".as_slice())
    .bind([18_u8; 12].as_slice())
    .bind(b"encrypted properties".as_slice())
    .bind([19_u8; 12].as_slice())
    .bind(user_id)
    .execute(&pool)
    .await;

    assert!(page.is_err());
}

#[sqlx::test]
async fn schema_has_no_plaintext_knowledge_columns(pool: PgPool) {
    let forbidden_columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name
           FROM information_schema.columns
          WHERE table_schema = 'public'
            AND table_name <> '_sqlx_migrations'
            AND column_name IN ('title', 'body', 'name', 'description', 'original_filename')",
    )
    .fetch_all(&pool)
    .await
    .expect("schema metadata should be readable");

    assert!(forbidden_columns.is_empty(), "{forbidden_columns:?}");
}

#[sqlx::test]
async fn export_cannot_reference_job_from_another_workspace(pool: PgPool) {
    let first_workspace = seed_workspace(&pool).await;
    let first_user = seed_user(&pool, first_workspace).await;
    let first_space = seed_space(&pool, first_workspace, first_user).await;
    let second_workspace = seed_workspace(&pool).await;
    let foreign_job = Uuid::now_v7();

    sqlx::query(
        "INSERT INTO jobs (
            id, workspace_id, job_type, payload_ciphertext, payload_nonce,
            payload_key_version, status
         ) VALUES ($1, $2, 'export', $3, $4, 1, 'queued')",
    )
    .bind(foreign_job)
    .bind(second_workspace)
    .bind(b"encrypted job payload".as_slice())
    .bind([20_u8; 12].as_slice())
    .execute(&pool)
    .await
    .expect("foreign job fixture should insert");

    let cross_workspace_export = sqlx::query(
        "INSERT INTO exports (
            id, workspace_id, space_id, requested_by, job_id, status
         ) VALUES ($1, $2, $3, $4, $5, 'queued')",
    )
    .bind(Uuid::now_v7())
    .bind(first_workspace)
    .bind(first_space)
    .bind(first_user)
    .bind(foreign_job)
    .execute(&pool)
    .await;

    assert!(cross_workspace_export.is_err());
}
