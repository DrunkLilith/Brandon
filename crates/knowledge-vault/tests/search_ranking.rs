use knowledge_vault::{SearchMode, Vault};
use std::{collections::HashSet, fs};

#[test]
fn task_chunk_duplicates_cannot_starve_other_matching_tasks_before_the_limit() {
    let directory = tempfile::tempdir().unwrap();
    let vault = Vault::open(directory.path()).unwrap();
    vault.create_task("AAA crowded", None).unwrap();
    let crowded = (0..100)
        .map(|id| format!("## Section {id}\n\nrankingneedle\n\n"))
        .collect::<String>();
    fs::write(directory.path().join("Tasks/AAA crowded.md"), crowded).unwrap();
    for id in 0..9 {
        vault.create_task(&format!("ZZZ task {id}"), None).unwrap();
        fs::write(
            directory.path().join(format!("Tasks/ZZZ task {id}.md")),
            "# Task\n\nrankingneedle with extra context words\n",
        )
        .unwrap();
    }
    let canonical = fs::read(directory.path().join(".brandon/tasks.sqlite3")).unwrap();
    for mode in [SearchMode::Literal, SearchMode::Words] {
        let result = vault
            .search_local("rankingneedle type:task", mode, 20)
            .unwrap();
        assert_eq!(
            result.hits.len(),
            10,
            "{mode:?}: duplicates consumed the candidate budget"
        );
        assert_eq!(
            result
                .hits
                .iter()
                .map(|hit| &hit.path)
                .collect::<HashSet<_>>()
                .len(),
            10
        );
        assert_eq!(
            vault
                .search_local("rankingneedle type:task", mode, 3)
                .unwrap()
                .hits
                .len(),
            3
        );
        assert_eq!(
            vault
                .search_local("rankingneedle", mode, 20)
                .unwrap()
                .hits
                .len(),
            10
        );
    }
    assert_eq!(
        fs::read(directory.path().join(".brandon/tasks.sqlite3")).unwrap(),
        canonical
    );
}

#[test]
fn task_collapse_keeps_best_match_but_note_sections_and_canvas_nodes_stay_distinct() {
    let directory = tempfile::tempdir().unwrap();
    let vault = Vault::open(directory.path()).unwrap();
    vault.create_task("Task", None).unwrap();
    fs::write(directory.path().join("Tasks/Task.md"), "# Task\n\nrankingneedle lots of other context words here to dilute the earlier match\n\n## Strong\n\nrankingneedle rankingneedle rankingneedle\n").unwrap();
    fs::write(
        directory.path().join("Note.md"),
        "# First\n\nrankingneedle\n\n## Second\n\nrankingneedle\n",
    )
    .unwrap();
    fs::write(directory.path().join("Board.canvas"), r#"{"nodes":[{"id":"a","type":"text","text":"rankingneedle one"},{"id":"b","type":"text","text":"rankingneedle two"}],"edges":[]}"#).unwrap();
    for mode in [SearchMode::Words, SearchMode::Literal] {
        let result = vault.search_local("rankingneedle", mode, 20).unwrap();
        assert_eq!(result.hits.len(), 5);
        let tasks: Vec<_> = result
            .hits
            .iter()
            .filter(|hit| hit.kind == "task")
            .collect();
        assert_eq!(tasks.len(), 1);
        if mode == SearchMode::Words {
            assert_eq!(
                tasks[0].line, 7,
                "Choose the best BM25 task section, not its first row"
            );
        }
        assert_eq!(
            result
                .hits
                .iter()
                .filter(|hit| hit.kind == "note")
                .map(|hit| hit.line)
                .collect::<HashSet<_>>(),
            HashSet::from([3, 7])
        );
        assert_eq!(
            result
                .hits
                .iter()
                .filter_map(|hit| hit.node_id.as_deref())
                .collect::<HashSet<_>>(),
            HashSet::from(["a", "b"])
        );
    }
    assert_eq!(
        vault
            .search_local("type:note", SearchMode::Words, 20)
            .unwrap()
            .hits
            .len(),
        1
    );
    assert_eq!(
        vault
            .search_local("type:canvas", SearchMode::Words, 20)
            .unwrap()
            .hits
            .len(),
        1
    );
}
