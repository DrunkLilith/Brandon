//! Permission-aware blind search.
//!
//! Queries use the same Unicode normalization as indexing. Each query word is
//! ANDed; words between 3 and 12 Unicode scalar values also match word-prefixes.
//! A multi-word query receives an in-memory exact-phrase ranking boost, but is
//! not a quoted-query language. v1 limits a query to 500 characters, 16 words,
//! and 100 ACL-filtered candidates; it deliberately has no stemming or fuzzy
//! matching. `author` filters the author of the current version (`updated_by`),
//! and `from`/`to` bound `updated_at` inclusively using RFC 3339 timestamps.

use axum::{
    Json,
    extract::{Extension, Query, State},
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use knowledge_core::permissions::Capability;
use knowledge_crypto::{BlindIndexer, BlindToken, KeyPurpose};
use knowledge_store::{
    pages::PageRepository,
    search::{BlindSearchRepository, SearchFilters},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    CurrentUser,
    middleware::RequestId,
    problem::ApiProblem,
    routes::{
        pages::page_aad,
        users::{
            audit_disclosure, decrypt_text, load_workspace_key_in_transaction,
            refresh_workspace_admin,
        },
    },
    state::AppState,
};

const MAX_QUERY_CHARACTERS: usize = 500;
const MAX_QUERY_WORDS: usize = 16;
const MAX_SNIPPET_CHARACTERS: usize = 160;

api_router! {
    pub(crate) fn router;
    "/api/v1/search" => [get => search => ("search", Session, 200)],
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    space: Option<Uuid>,
    author: Option<Uuid>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchHit {
    page_id: Uuid,
    title: String,
    snippet: String,
    updated_at: DateTime<Utc>,
}

struct RankedHit {
    hit: SearchHit,
    rank: u8,
}

async fn search(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Query(query): Query<SearchQuery>,
) -> Result<Response, ApiProblem> {
    if query.q.chars().count() > MAX_QUERY_CHARACTERS {
        return Err(ApiProblem::validation("q", "is too long"));
    }
    if query
        .from
        .is_some_and(|from| query.to.is_some_and(|to| from > to))
    {
        return Err(ApiProblem::validation("from", "must not be after to"));
    }

    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    crate::audit::lock_workspace_for_audit(&mut transaction, current.workspace_id)
        .await
        .map_err(internal)?;
    crate::routes::spaces::revalidate_session(&mut transaction, current).await?;
    let current = refresh_workspace_admin(&mut transaction, current).await?;
    let rotating: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM key_rotations WHERE workspace_id=$1 AND state<>'complete')",
    )
    .bind(current.workspace_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(internal)?;
    if rotating {
        return Err(ApiProblem::unavailable());
    }
    let workspace_key =
        load_workspace_key_in_transaction(&state, &mut transaction, current.workspace_id).await?;
    let indexer = BlindIndexer::new(workspace_key.derive(KeyPurpose::Search).map_err(internal)?);
    let words = BlindIndexer::words("und", &query.q);
    if words.is_empty() {
        return Err(ApiProblem::validation(
            "q",
            "must contain a searchable word",
        ));
    }
    if words.len() > MAX_QUERY_WORDS {
        return Err(ApiProblem::validation("q", "contains too many words"));
    }
    let query_tokens = query_tokens(&indexer, &words);

    let candidates = BlindSearchRepository::candidates(
        &mut transaction,
        current.workspace_id,
        current.user_id,
        &query_tokens,
        SearchFilters {
            space_id: query.space,
            author_id: query.author,
            from: query.from,
            to: query.to,
        },
    )
    .await
    .map_err(internal)?;

    let mut hits = Vec::new();
    for candidate in candidates {
        let still_active: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM pages WHERE workspace_id = $1 AND id = $2 AND archived_at IS NULL
            )",
        )
        .bind(current.workspace_id)
        .bind(candidate.page_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(internal)?;
        if !still_active {
            continue;
        }
        let allowed = PageRepository::access_context(
            &mut transaction,
            current.workspace_id,
            current.user_id,
            candidate.page_id,
        )
        .await
        .map_err(internal)?
        .is_some_and(|context| context.can(Capability::Search));
        if !allowed {
            continue;
        }

        let title = decrypt_text(
            &workspace_key,
            &page_aad(current.workspace_id, candidate.page_id, None, "title"),
            candidate.title_ciphertext,
            candidate.title_nonce,
            candidate.title_key_version,
        )?;
        let body = decrypt_text(
            &workspace_key,
            &page_aad(current.workspace_id, candidate.page_id, None, "body"),
            candidate.body_ciphertext,
            candidate.body_nonce,
            candidate.body_key_version,
        )?;
        let title_words = BlindIndexer::words("und", &title);
        let body_words = BlindIndexer::words("und", &body);
        let title_phrase = has_exact_phrase(&title_words, &words);
        let body_phrase = has_exact_phrase(&body_words, &words);
        let title_tokens = has_prefix_matches(&title_words, &words);
        let body_tokens = has_prefix_matches(&body_words, &words);
        let page_tokens = has_prefix_matches(
            &title_words
                .iter()
                .chain(&body_words)
                .cloned()
                .collect::<Vec<_>>(),
            &words,
        );
        let rank = if title_phrase {
            4
        } else if title_tokens {
            3
        } else if body_phrase {
            2
        } else if body_tokens {
            1
        } else if page_tokens {
            0
        } else {
            continue;
        };
        let snippet_source = if title_phrase || title_tokens {
            &title
        } else if body_phrase || body_tokens || has_any_prefix_match(&body_words, &words) {
            &body
        } else {
            &title
        };
        let snippet = snippet(snippet_source, &words);
        hits.push(RankedHit {
            hit: SearchHit {
                page_id: candidate.page_id,
                title,
                snippet,
                updated_at: candidate.updated_at,
            },
            rank,
        });
    }
    audit_disclosure(
        &mut transaction,
        &workspace_key,
        current,
        request_id,
        "search.admin_access",
    )
    .await?;
    transaction.commit().await.map_err(internal)?;

    hits.sort_by(|left, right| {
        right
            .rank
            .cmp(&left.rank)
            .then_with(|| right.hit.updated_at.cmp(&left.hit.updated_at))
            .then_with(|| left.hit.page_id.cmp(&right.hit.page_id))
    });
    let mut response = Json(
        hits.into_iter()
            .map(|ranked| ranked.hit)
            .collect::<Vec<_>>(),
    )
    .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

fn query_tokens(indexer: &BlindIndexer, words: &[String]) -> Vec<BlindToken> {
    words
        .iter()
        .filter_map(|word| {
            let character_count = word.chars().count();
            let desired_prefix = (3..=12)
                .contains(&character_count)
                .then_some(character_count as u8);
            indexer
                .tokens("und", word)
                .into_iter()
                .find(|token| token.prefix_length() == desired_prefix.unwrap_or(0))
        })
        .collect()
}

fn has_prefix_matches(content_words: &[String], query_words: &[String]) -> bool {
    query_words.iter().all(|query_word| {
        content_words
            .iter()
            .any(|content_word| matches_query_word(content_word, query_word))
    })
}

fn has_any_prefix_match(content_words: &[String], query_words: &[String]) -> bool {
    content_words.iter().any(|content_word| {
        query_words
            .iter()
            .any(|query_word| matches_query_word(content_word, query_word))
    })
}

fn matches_query_word(content_word: &str, query_word: &str) -> bool {
    if (3..=12).contains(&query_word.chars().count()) {
        content_word.starts_with(query_word)
    } else {
        content_word == query_word
    }
}

fn has_exact_phrase(content_words: &[String], query_words: &[String]) -> bool {
    !query_words.is_empty()
        && content_words
            .windows(query_words.len())
            .any(|window| window == query_words)
}

fn snippet(text: &str, query_words: &[String]) -> String {
    let Some(match_byte_offset) = matching_original_word_offset(text, query_words) else {
        return text.chars().take(MAX_SNIPPET_CHARACTERS).collect();
    };
    let prefix_characters = text[..match_byte_offset].chars().count();
    let start = prefix_characters.saturating_sub(MAX_SNIPPET_CHARACTERS / 3);
    let mut value: String = text
        .chars()
        .skip(start)
        .take(MAX_SNIPPET_CHARACTERS)
        .collect();
    if start > 0 {
        value.insert(0, '…');
    }
    if text.chars().count() > start + MAX_SNIPPET_CHARACTERS {
        value.push('…');
    }
    value
}

fn matching_original_word_offset(text: &str, query_words: &[String]) -> Option<usize> {
    let mut word_start = None;
    for (byte_offset, character) in text.char_indices() {
        if character.is_alphanumeric() {
            word_start.get_or_insert(byte_offset);
        } else if let Some(start) = word_start.take()
            && BlindIndexer::words("und", &text[start..byte_offset])
                .iter()
                .any(|word| {
                    query_words
                        .iter()
                        .any(|query_word| matches_query_word(word, query_word))
                })
        {
            return Some(start);
        }
    }
    word_start.filter(|start| {
        BlindIndexer::words("und", &text[*start..])
            .iter()
            .any(|word| {
                query_words
                    .iter()
                    .any(|query_word| matches_query_word(word, query_word))
            })
    })
}

fn internal<E>(_error: E) -> ApiProblem {
    ApiProblem::internal()
}
