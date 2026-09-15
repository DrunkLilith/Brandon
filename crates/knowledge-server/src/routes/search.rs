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
    Json, Router,
    extract::{Query, State},
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
    routing::get,
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
    problem::ApiProblem,
    routes::{
        pages::page_aad,
        users::{decrypt_text, load_workspace_key},
    },
    state::AppState,
};

const MAX_QUERY_CHARACTERS: usize = 500;
const MAX_QUERY_WORDS: usize = 16;
const MAX_SNIPPET_CHARACTERS: usize = 160;

pub(crate) fn router() -> Router<AppState> {
    Router::new().route("/api/v1/search", get(search))
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
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
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

    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    crate::routes::spaces::revalidate_session(&mut transaction, current).await?;
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
        let rank = if title_phrase {
            4
        } else if title_tokens {
            3
        } else if body_phrase {
            2
        } else if body_tokens {
            1
        } else {
            continue;
        };
        let snippet_source = if title_phrase || title_tokens {
            &title
        } else {
            &body
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
        content_words.iter().any(|content_word| {
            if (3..=12).contains(&query_word.chars().count()) {
                content_word.starts_with(query_word)
            } else {
                content_word == query_word
            }
        })
    })
}

fn has_exact_phrase(content_words: &[String], query_words: &[String]) -> bool {
    !query_words.is_empty()
        && content_words
            .windows(query_words.len())
            .any(|window| window == query_words)
}

fn snippet(text: &str, query_words: &[String]) -> String {
    let lower_words = BlindIndexer::words("und", text);
    let match_word = lower_words.iter().find(|word| {
        query_words.iter().any(|query_word| {
            if (3..=12).contains(&query_word.chars().count()) {
                word.starts_with(query_word)
            } else {
                word.as_str() == query_word
            }
        })
    });
    let Some(match_word) = match_word else {
        return text.chars().take(MAX_SNIPPET_CHARACTERS).collect();
    };
    let lower = text.to_lowercase();
    let start = lower.find(match_word).unwrap_or(0);
    let prefix_characters = lower[..start].chars().count();
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

fn internal<E>(_error: E) -> ApiProblem {
    ApiProblem::internal()
}
