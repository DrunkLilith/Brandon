use knowledge_vault::{SearchMode, Vault};
use std::fs;

fn fixture() -> (tempfile::TempDir, Vault) {
    let directory = tempfile::tempdir().expect("isolated search vault");
    let vault = Vault::open(directory.path()).expect("open vault");
    (directory, vault)
}

#[test]
fn russian_english_phrases_prefixes_and_exact_lines() {
    let (directory, vault) = fixture();
    fs::write(
        directory.path().join("План.md"),
        "# План\n\n## Разработка\n\nПривет мир. GPU renderer ready. #работа\nДругой абзац.\n",
    )
    .unwrap();
    let result = vault
        .search_local("\"привет мир\" renderer tag:работа", SearchMode::Words, 20)
        .unwrap();
    assert_eq!(result.hits.len(), 1);
    assert_eq!(result.hits[0].line, 5);
    assert_eq!(result.hits[0].path, "План.md");
    assert!(!result.hits[0].revision.is_empty());
    assert_eq!(
        vault
            .search_local("разраб*", SearchMode::Words, 20)
            .unwrap()
            .hits
            .len(),
        1
    );
    assert!(
        vault
            .search_local("\"мир привет\"", SearchMode::Words, 20)
            .unwrap()
            .hits
            .is_empty()
    );
}

#[test]
fn literal_search_preserves_code_punctuation_and_unicode_offsets() {
    let (directory, vault) = fixture();
    fs::write(
        directory.path().join("Code.md"),
        "# Code\n\nİstanbul\nlet key = foo::bar(42);\nA literal [x] marker.\n",
    )
    .unwrap();
    let result = vault
        .search_local("foo::bar(42)", SearchMode::Literal, 20)
        .unwrap();
    assert_eq!(result.hits.len(), 1);
    assert_eq!(result.hits[0].line, 4);
    assert!(result.hits[0].snippet.contains("foo::bar(42)"));
    assert_eq!(
        vault
            .search_local("[x]", SearchMode::Literal, 20)
            .unwrap()
            .hits[0]
            .line,
        5
    );
    assert!(
        vault
            .search_local("\"unfinished", SearchMode::Words, 20)
            .is_err()
    );
    assert!(
        vault
            .search_local(&"a".repeat(513), SearchMode::Words, 20)
            .is_err()
    );
    assert!(
        vault
            .search_local("type:unknown", SearchMode::Words, 20)
            .is_err()
    );
    assert!(
        vault
            .search_local("\" OR secret", SearchMode::Words, 20)
            .is_err()
    );
    fs::write(
        directory.path().join("Words.md"),
        "alpha beta\ngamma alpha\nbeta gamma\n",
    )
    .unwrap();
    assert!(
        vault
            .search_local("alpha gamma", SearchMode::Literal, 20)
            .unwrap()
            .hits
            .is_empty()
    );
    assert!(
        !vault
            .search_local("alpha gamma", SearchMode::Words, 20)
            .unwrap()
            .hits
            .is_empty()
    );
    fs::write(
        directory.path().join("Quoted.md"),
        "# Code\n\nİstanbul\nprintln!(\"Привет мир\");\nlet  exact = 1;\nalpha\nbeta\n",
    )
    .unwrap();
    assert_eq!(
        vault
            .search_local("println!(\"Привет мир\")", SearchMode::Literal, 20)
            .unwrap()
            .hits[0]
            .line,
        4
    );
    assert_eq!(
        vault
            .search_local("let  exact", SearchMode::Literal, 20)
            .unwrap()
            .hits
            .len(),
        1
    );
    assert!(
        vault
            .search_local("let exact", SearchMode::Literal, 20)
            .unwrap()
            .hits
            .is_empty()
    );
    assert_eq!(
        vault
            .search_local("alpha\nbeta path:Quoted", SearchMode::Literal, 20)
            .unwrap()
            .hits[0]
            .line,
        6
    );
}

#[test]
fn warm_index_updates_external_edits_renames_and_deletions() {
    let (directory, vault) = fixture();
    let original = directory.path().join("First.md");
    fs::write(&original, "# First\n\noldmarker\n").unwrap();
    assert_eq!(
        vault
            .search_local("oldmarker", SearchMode::Words, 20)
            .unwrap()
            .changed_sources,
        1
    );
    assert_eq!(
        vault
            .search_local("oldmarker", SearchMode::Words, 20)
            .unwrap()
            .changed_sources,
        0
    );
    fs::write(&original, "# First\n\nnewmarkertoday\n").unwrap();
    assert!(
        vault
            .search_local("oldmarker", SearchMode::Words, 20)
            .unwrap()
            .hits
            .is_empty()
    );
    assert_eq!(
        vault
            .search_local("newmarkertoday", SearchMode::Words, 20)
            .unwrap()
            .hits
            .len(),
        1
    );
    let renamed = directory.path().join("Renamed.md");
    fs::rename(&original, &renamed).unwrap();
    let result = vault
        .search_local("newmarkertoday", SearchMode::Words, 20)
        .unwrap();
    assert_eq!(result.hits[0].path, "Renamed.md");
    assert_eq!(result.removed_sources, 1);
    fs::remove_file(&renamed).unwrap();
    assert!(
        vault
            .search_local("newmarkertoday", SearchMode::Words, 20)
            .unwrap()
            .hits
            .is_empty()
    );
}

#[test]
fn private_paths_and_unparsed_binary_contents_are_not_indexed() {
    let (directory, vault) = fixture();
    fs::create_dir(directory.path().join(".private")).unwrap();
    fs::write(directory.path().join(".private/Secret.md"), "privatemarker").unwrap();
    fs::write(directory.path().join("token.txt"), "privatemarker").unwrap();
    fs::write(directory.path().join("credentials.json"), "privatemarker").unwrap();
    fs::write(directory.path().join("Manual.pdf"), "%PDF-privatemarker").unwrap();
    let result = vault
        .search_local("privatemarker", SearchMode::Literal, 20)
        .unwrap();
    assert!(result.hits.is_empty());
    let named = vault.search_local("Manual", SearchMode::Words, 20).unwrap();
    assert_eq!(named.hits.len(), 1);
    assert_eq!(named.hits[0].kind, "file");
    assert!(!named.hits[0].content_indexed);
    assert_eq!(named.name_only_sources, 1);
}

#[test]
fn canvas_hits_have_exact_node_ids_and_do_not_dereference_files() {
    let (directory, vault) = fixture();
    fs::write(directory.path().join("Board.canvas"), r#"{"nodes":[{"id":"found","type":"text","text":"Русская карточка telescope"},{"id":"ref","type":"file","file":"../outside.md"}],"edges":[]}"#).unwrap();
    let result = vault
        .search_local("telescope type:canvas", SearchMode::Words, 20)
        .unwrap();
    assert_eq!(result.hits.len(), 1);
    assert_eq!(result.hits[0].node_id.as_deref(), Some("found"));
    assert_eq!(result.hits[0].path, "Board.canvas");
}

#[test]
fn task_metadata_is_indexed_without_modifying_the_authoritative_database() {
    let (directory, vault) = fixture();
    vault
        .create_task("Подготовить демо", Some("2026-10-20"))
        .unwrap();
    let database = directory.path().join(".brandon/tasks.sqlite3");
    let before = fs::read(&database).unwrap();
    let result = vault
        .search_local("2026-10-20 type:task", SearchMode::Literal, 20)
        .unwrap();
    assert!(!result.hits.is_empty());
    assert!(
        result
            .hits
            .iter()
            .all(|hit| hit.path == "Tasks/Подготовить демо.md")
    );
    assert!(result.hits.iter().all(|hit| hit.line == 1));
    assert_eq!(fs::read(&database).unwrap(), before);
    assert!(
        directory
            .path()
            .join(".brandon/search-v1.sqlite3")
            .is_file()
    );
}

#[test]
fn filter_only_browsing_returns_resources_not_duplicate_chunks() {
    let (directory, vault) = fixture();
    fs::write(
        directory.path().join("Другой.md"),
        "# Другой\n\nuniquetitlemarker\n",
    )
    .unwrap();
    let titled = vault
        .search_local("uniquetitlemarker", SearchMode::Words, 20)
        .unwrap();
    assert_eq!(titled.hits[0].title, "Другой");
    fs::write(
        directory.path().join("Note.md"),
        "# Title\n\nContent\n\n## Section\n\nMore content\n",
    )
    .unwrap();
    fs::write(directory.path().join("Board.canvas"), r#"{"nodes":[{"id":"a","type":"text","text":"First"},{"id":"b","type":"text","text":"Second"}],"edges":[]}"#).unwrap();
    vault.create_task("Unique task", None).unwrap();
    assert_eq!(
        vault
            .search_local("type:task", SearchMode::Words, 20)
            .unwrap()
            .hits
            .len(),
        1
    );
    assert_eq!(
        vault
            .search_local("type:note", SearchMode::Words, 20)
            .unwrap()
            .hits
            .len(),
        2
    );
    let board = vault
        .search_local("type:canvas", SearchMode::Words, 20)
        .unwrap();
    assert_eq!(board.hits.len(), 1);
    assert_eq!(board.hits[0].title, "Board");
    assert!(board.hits[0].node_id.is_none());
}

#[test]
fn corrupt_sources_and_oversized_notes_are_reported_not_silently_understood() {
    let (directory, vault) = fixture();
    fs::write(directory.path().join("Broken.canvas"), "not json").unwrap();
    fs::File::create(directory.path().join("Enormous.md"))
        .unwrap()
        .set_len(16 * 1024 * 1024 + 1)
        .unwrap();
    let result = vault
        .search_local("type:canvas", SearchMode::Words, 20)
        .unwrap();
    assert_eq!(result.failed_sources, 1);
    assert_eq!(result.name_only_sources, 1);
    assert!(!result.hits[0].content_indexed);
    assert_eq!(
        vault
            .search_local("Enormous", SearchMode::Words, 20)
            .unwrap()
            .hits
            .len(),
        1
    );
}

#[test]
fn a_twenty_thousand_card_board_remains_content_searchable() {
    let (directory, vault) = fixture();
    let nodes: Vec<_> = (0..20_000).map(|id| serde_json::json!({
        "id": format!("card-{id}"), "type": "text", "x": id * 300, "y": 0,
        "width": 280, "height": 150, "text": format!("Карточка {id}: renderer knowledge workspace {}", if id == 19_999 { "findlastcard" } else { "common" })
    })).collect();
    fs::write(
        directory.path().join("Large.canvas"),
        serde_json::to_vec(&serde_json::json!({"nodes": nodes,"edges":[]})).unwrap(),
    )
    .unwrap();
    let result = vault
        .search_local("findlastcard", SearchMode::Words, 20)
        .unwrap();
    assert_eq!(result.hits.len(), 1);
    assert_eq!(result.hits[0].node_id.as_deref(), Some("card-19999"));
    let warm = vault
        .search_local("findlastcard", SearchMode::Words, 20)
        .unwrap();
    assert_eq!(warm.changed_sources, 0);
    eprintln!(
        "20k Canvas: cold {:.1}ms, warm {:.1}ms",
        result.elapsed_ms, warm.elapsed_ms
    );
}

#[test]
fn notes_exceeding_the_chunk_budget_are_visibly_name_only_not_silently_partial() {
    let (directory, vault) = fixture();
    let body = (0..1050)
        .map(|id| format!("## Section {id}\n\ncontent-{id}\n\n"))
        .collect::<String>();
    fs::write(directory.path().join("Overchunk.md"), body).unwrap();
    let result = vault
        .search_local("path:Overchunk", SearchMode::Words, 20)
        .unwrap();
    assert_eq!(result.name_only_sources, 1);
    assert_eq!(result.hits.len(), 1);
    assert!(!result.hits[0].content_indexed);
    assert!(
        vault
            .search_local("content-1049", SearchMode::Literal, 20)
            .unwrap()
            .hits
            .is_empty()
    );
}

#[cfg(unix)]
#[test]
fn symlinked_sources_and_cache_fail_closed() {
    use std::os::unix::fs::symlink;
    let (directory, vault) = fixture();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("Outside.md"), "outsidemarker").unwrap();
    symlink(
        outside.path().join("Outside.md"),
        directory.path().join("Linked.md"),
    )
    .unwrap();
    assert!(
        vault
            .search_local("outsidemarker", SearchMode::Words, 20)
            .unwrap()
            .hits
            .is_empty()
    );
    fs::remove_file(directory.path().join(".brandon/search-v1.sqlite3")).unwrap();
    symlink(
        outside.path().join("Outside.md"),
        directory.path().join(".brandon/search-v1.sqlite3"),
    )
    .unwrap();
    assert!(
        vault
            .search_local("anything", SearchMode::Words, 20)
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(outside.path().join("Outside.md")).unwrap(),
        "outsidemarker"
    );
}
