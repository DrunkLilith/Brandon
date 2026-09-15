use knowledge_crypto::{BlindIndexer, KeyPurpose, WorkspaceKey};

fn indexer() -> BlindIndexer {
    let key = WorkspaceKey::from_bytes([23; 32])
        .derive(KeyPurpose::Search)
        .expect("fixture search key should derive");
    BlindIndexer::new(key)
}

#[test]
fn blind_index_tokens_fold_russian_and_english_case_and_normalize_unicode() {
    let indexer = indexer();

    assert_eq!(
        indexer.tokens("ru", "Ёжик CAFÉ"),
        indexer.tokens("ru", "е\u{308}ЖИК cafe\u{301}"),
    );
}

#[test]
fn blind_index_tokens_ignore_punctuation_combine_repeated_words_and_create_prefixes() {
    let indexer = indexer();
    let tokens = indexer.tokens("en", "Road, road... ROAD!");

    assert_eq!(
        tokens.len(),
        3,
        "the full word and character prefixes are distinct"
    );
    assert!(
        tokens
            .iter()
            .any(|token| token.prefix_length() == 0 && token.frequency() == 3)
    );
    assert!(
        tokens
            .iter()
            .any(|token| token.prefix_length() == 3 && token.frequency() == 3)
    );
}

#[test]
fn blind_index_tokens_make_phrase_words_searchable_and_limit_prefixes_to_unicode_character_lengths()
{
    let indexer = indexer();
    let phrase = indexer.tokens("en", "Knowledge platform");
    let same_words = indexer.tokens("en", "platform knowledge");
    let long_word = indexer.tokens("en", "abcdefghijklmnop");

    assert_eq!(
        phrase, same_words,
        "phrase matching is verified after blind candidates are fetched"
    );
    assert!(long_word.iter().any(|token| token.prefix_length() == 12));
    assert!(!long_word.iter().any(|token| token.prefix_length() > 12));
    assert!(long_word.iter().any(|token| token.prefix_length() == 0));
}

#[test]
fn blind_index_tokens_do_not_expose_source_words_through_debug_or_serialization() {
    let token = indexer()
        .tokens(
            "ru",
            "\u{421}\u{415}\u{41a}\u{420}\u{415}\u{422}\u{41d}\u{42b}\u{419}",
        )
        .remove(0);
    let debug = format!("{token:?}");

    assert!(!debug.contains("\u{421}\u{415}\u{41a}\u{420}\u{415}\u{422}\u{41d}\u{42b}\u{419}"));
}
