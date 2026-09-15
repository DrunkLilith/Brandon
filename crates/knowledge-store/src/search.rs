//! Persistence for the keyed blind index.
//!
//! Search supports normalized whole words and character prefixes of length 3
//! through 12. Multiple query words are ANDed to find candidates; quoted or
//! unquoted multi-word phrases are verified only after authorized candidates
//! are decrypted in memory. v1 intentionally has no stemming, fuzzy matching,
//! morphology, or global relevance service. SQL applies the complete ACL before
//! the fixed 100-candidate cap, then the server rechecks page authority before
//! decryption.

use chrono::{DateTime, Utc};
use knowledge_crypto::BlindToken;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::StoreError;

pub const SEARCH_CANDIDATE_LIMIT: i64 = 100;

pub struct BlindSearchRepository;

#[derive(Clone, Copy, Default)]
pub struct SearchFilters {
    pub space_id: Option<Uuid>,
    /// The author of the current page version (`pages.updated_by`).
    pub author_id: Option<Uuid>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

pub struct SearchCandidate {
    pub page_id: Uuid,
    pub title_ciphertext: Vec<u8>,
    pub title_nonce: Vec<u8>,
    pub title_key_version: i32,
    pub body_ciphertext: Vec<u8>,
    pub body_nonce: Vec<u8>,
    pub body_key_version: i32,
    pub updated_at: DateTime<Utc>,
    pub title_frequency: i64,
    pub body_frequency: i64,
}

impl BlindSearchRepository {
    pub async fn replace_page_tokens(
        transaction: &mut Transaction<'_, Postgres>,
        workspace_id: Uuid,
        page_id: Uuid,
        title_tokens: &[BlindToken],
        body_tokens: &[BlindToken],
    ) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM blind_index_tokens WHERE workspace_id = $1 AND page_id = $2")
            .bind(workspace_id)
            .bind(page_id)
            .execute(&mut **transaction)
            .await?;
        Self::insert_tokens(transaction, workspace_id, page_id, 1, title_tokens).await?;
        Self::insert_tokens(transaction, workspace_id, page_id, 2, body_tokens).await?;
        Ok(())
    }

    pub async fn remove_page_tokens(
        transaction: &mut Transaction<'_, Postgres>,
        workspace_id: Uuid,
        page_ids: &[Uuid],
    ) -> Result<(), StoreError> {
        if page_ids.is_empty() {
            return Ok(());
        }
        sqlx::query("DELETE FROM blind_index_tokens WHERE workspace_id = $1 AND page_id = ANY($2)")
            .bind(workspace_id)
            .bind(page_ids)
            .execute(&mut **transaction)
            .await?;
        Ok(())
    }

    pub async fn candidates(
        transaction: &mut Transaction<'_, Postgres>,
        workspace_id: Uuid,
        user_id: Uuid,
        query_tokens: &[BlindToken],
        filters: SearchFilters,
    ) -> Result<Vec<SearchCandidate>, StoreError> {
        let mut token_hmacs = Vec::new();
        let mut prefix_lengths = Vec::new();
        for token in query_tokens {
            let hmac = token.hmac().to_vec();
            let prefix_length = i16::from(token.prefix_length());
            if !token_hmacs
                .iter()
                .zip(&prefix_lengths)
                .any(|(existing, existing_length)| {
                    existing == &hmac && *existing_length == prefix_length
                })
            {
                token_hmacs.push(hmac);
                prefix_lengths.push(prefix_length);
            }
        }
        if token_hmacs.is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query_as(
            "WITH query_tokens AS (
                SELECT DISTINCT token_hmac, prefix_length
                  FROM unnest($3::bytea[], $4::smallint[])
                       AS requested(token_hmac, prefix_length)
             ), matched AS (
                SELECT bit.page_id,
                       SUM(CASE WHEN bit.field_kind = 1 THEN bit.frequency ELSE 0 END)::bigint
                           AS title_frequency,
                       SUM(CASE WHEN bit.field_kind = 2 THEN bit.frequency ELSE 0 END)::bigint
                           AS body_frequency
                  FROM blind_index_tokens bit
                  JOIN query_tokens requested
                    ON requested.token_hmac = bit.token_hmac
                   AND requested.prefix_length = bit.prefix_length
                 WHERE bit.workspace_id = $1
                 GROUP BY bit.page_id
                HAVING COUNT(DISTINCT (bit.token_hmac, bit.prefix_length)) =
                       (SELECT count(*) FROM query_tokens)
             )
             SELECT p.id, p.title_ciphertext, p.title_nonce, p.title_key_version,
                    p.body_ciphertext, p.body_nonce, p.body_key_version, p.updated_at,
                    matched.title_frequency, matched.body_frequency
               FROM matched
               JOIN pages p ON p.workspace_id = $1 AND p.id = matched.page_id
               JOIN users u ON u.workspace_id = $1 AND u.id = $2
               JOIN workspace_members wm ON wm.workspace_id = u.workspace_id AND wm.user_id = u.id
               CROSS JOIN LATERAL (
                    WITH RECURSIVE ancestors(id, parent_id, restricted, depth) AS (
                        SELECT p.id, p.parent_id, p.restricted, 0
                        UNION ALL
                        SELECT parent.id, parent.parent_id, parent.restricted, ancestors.depth + 1
                          FROM pages parent JOIN ancestors ON ancestors.parent_id = parent.id
                         WHERE parent.workspace_id = $1
                    )
                    SELECT (
                        SELECT id FROM ancestors WHERE restricted ORDER BY depth LIMIT 1
                    ) AS boundary_page_id
               ) boundary
              WHERE p.archived_at IS NULL
                AND u.status = 'active' AND u.deleted_at IS NULL AND wm.status = 'active'
                AND ($5::uuid IS NULL OR p.space_id = $5)
                AND ($6::uuid IS NULL OR p.updated_by = $6)
                AND ($7::timestamptz IS NULL OR p.updated_at >= $7)
                AND ($8::timestamptz IS NULL OR p.updated_at <= $8)
                AND (
                    wm.role = 'admin' OR EXISTS (
                        SELECT 1 FROM space_grants sg
                        LEFT JOIN groups g ON g.workspace_id = sg.workspace_id AND g.id = sg.group_id
                        LEFT JOIN group_members gm
                          ON gm.workspace_id = sg.workspace_id AND gm.group_id = sg.group_id
                         AND gm.user_id = $2
                         WHERE sg.workspace_id = $1 AND sg.space_id = p.space_id
                           AND (sg.user_id = $2 OR (sg.group_id IS NOT NULL AND g.archived_at IS NULL AND gm.user_id IS NOT NULL))
                    )
                )
                AND (
                    boundary.boundary_page_id IS NULL OR wm.role = 'admin' OR EXISTS (
                        SELECT 1 FROM page_permissions pp
                        LEFT JOIN groups g ON g.workspace_id = pp.workspace_id AND g.id = pp.group_id
                        LEFT JOIN group_members gm
                          ON gm.workspace_id = pp.workspace_id AND gm.group_id = pp.group_id
                         AND gm.user_id = $2
                         WHERE pp.workspace_id = $1 AND pp.page_id = boundary.boundary_page_id
                           AND (pp.user_id = $2 OR (pp.group_id IS NOT NULL AND g.archived_at IS NULL AND gm.user_id IS NOT NULL))
                    )
                )
              ORDER BY p.updated_at DESC, p.id ASC
              LIMIT $9",
        )
        .bind(workspace_id)
        .bind(user_id)
        .bind(token_hmacs)
        .bind(prefix_lengths)
        .bind(filters.space_id)
        .bind(filters.author_id)
        .bind(filters.from)
        .bind(filters.to)
        .bind(SEARCH_CANDIDATE_LIMIT)
        .fetch_all(&mut **transaction)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    page_id,
                    title_ciphertext,
                    title_nonce,
                    title_key_version,
                    body_ciphertext,
                    body_nonce,
                    body_key_version,
                    updated_at,
                    title_frequency,
                    body_frequency,
                )| SearchCandidate {
                    page_id,
                    title_ciphertext,
                    title_nonce,
                    title_key_version,
                    body_ciphertext,
                    body_nonce,
                    body_key_version,
                    updated_at,
                    title_frequency,
                    body_frequency,
                },
            )
            .collect())
    }

    async fn insert_tokens(
        transaction: &mut Transaction<'_, Postgres>,
        workspace_id: Uuid,
        page_id: Uuid,
        field_kind: i16,
        tokens: &[BlindToken],
    ) -> Result<(), StoreError> {
        for token in tokens {
            sqlx::query(
                "INSERT INTO blind_index_tokens (
                    workspace_id, page_id, token_hmac, field_kind, prefix_length, frequency
                 ) VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(workspace_id)
            .bind(page_id)
            .bind(token.hmac().as_slice())
            .bind(field_kind)
            .bind(i16::from(token.prefix_length()))
            .bind(
                i32::try_from(token.frequency())
                    .map_err(|_| StoreError::CorruptData("blind token frequency"))?,
            )
            .execute(&mut **transaction)
            .await?;
        }
        Ok(())
    }
}
