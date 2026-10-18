use super::*;

#[test]
fn canvas_file_references_follow_note_and_attachment_relocations() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::create_dir(root.join("Attachments")).unwrap();
    fs::create_dir(root.join("Media")).unwrap();
    fs::write(root.join("Old.md"), "# Old\n").unwrap();
    fs::write(root.join("Backlinks.md"), "See [[Old]].").unwrap();
    fs::write(root.join("Attachments/picture.png"), b"synthetic bytes").unwrap();
    fs::write(root.join("Board.canvas"), r##"{"nodes":[{"type":"file","id":"n","file":"Old.md","subpath":"#Part"},{"type":"file","id":"i","file":"Attachments/picture.png"}],"edges":[],"unknown":true}"##).unwrap();
    let vault = Vault::open(root).unwrap();
    vault.rename_entry("Old.md", "New", true).unwrap();
    vault
        .rename_entry("Attachments/picture.png", "renamed.png", true)
        .unwrap();
    vault
        .move_entry("Attachments", Some("Media"), true)
        .unwrap();
    let board: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("Board.canvas")).unwrap()).unwrap();
    assert_eq!(board["nodes"][0]["file"], "New.md");
    assert_eq!(board["nodes"][0]["subpath"], "#Part");
    assert_eq!(board["nodes"][1]["file"], "Media/Attachments/renamed.png");
    assert_eq!(
        fs::read_to_string(root.join("Backlinks.md")).unwrap(),
        "See [[New]]."
    );
}

#[test]
fn canvas_relocation_preserves_every_byte_outside_the_file_value() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::write(root.join("Old.md"), "# Old\n").unwrap();
    let source = "{\n  \"custom\": 1234567890123456789012345678901234567890,\n  \"nodes\": [{\"type\":\"file\",\"file\":\"Old.md\",\"precise\":1.2345678901234567890123456789}, {\"type\":\"text\",\"file\":\"Old.md\",\"text\":\"Old.md\"}, {\"type\":\"future\",\"file\":\"Old.md\"}],\n  \"edges\": [], \"url\":\"https://example.invalid/Old.md\"\n}\n";
    fs::write(root.join("Board.canvas"), source).unwrap();
    Vault::open(root)
        .unwrap()
        .rename_entry("Old.md", "New", true)
        .unwrap();
    let expected = source.replacen("\"file\":\"Old.md\"", "\"file\":\"New.md\"", 1);
    assert_eq!(
        fs::read_to_string(root.join("Board.canvas")).unwrap(),
        expected
    );
}

#[test]
fn moving_folder_updates_inside_and_outside_boards_not_sibling_prefixes() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::create_dir(root.join("Folder")).unwrap();
    fs::create_dir(root.join("Destination")).unwrap();
    fs::write(root.join("Folder/Note.md"), "# Note").unwrap();
    let source = r#"{"nodes":[{"type":"file","file":"Folder/Note.md"},{"type":"file","file":"Folderish/Note.md"},{"type":"file","file":"Folder\\Note.md"}],"edges":[]}"#;
    fs::write(root.join("Folder/Inside.canvas"), source).unwrap();
    fs::write(root.join("Outside.canvas"), source).unwrap();
    let vault = Vault::open(root).unwrap();
    vault
        .move_entry("Folder", Some("Destination"), true)
        .unwrap();
    let expected = source
        .replace("Folder/Note.md", "Destination/Folder/Note.md")
        .replace("Folder\\\\Note.md", "Destination/Folder/Note.md");
    for path in ["Destination/Folder/Inside.canvas", "Outside.canvas"] {
        assert_eq!(fs::read_to_string(root.join(path)).unwrap(), expected);
    }
}

#[test]
fn relocation_opt_out_and_target_collision_leave_boards_untouched() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::write(root.join("Old.md"), "# Old").unwrap();
    fs::write(root.join("Existing.md"), "# Existing").unwrap();
    let source = r#"{"nodes":[{"type":"file","file":"Old.md"}],"edges":[]}"#;
    fs::write(root.join("Board.canvas"), source).unwrap();
    let vault = Vault::open(root).unwrap();
    assert!(matches!(
        vault.rename_entry("Old.md", "Existing", true),
        Err(VaultError::AlreadyExists)
    ));
    assert!(root.join("Old.md").exists());
    assert_eq!(
        fs::read_to_string(root.join("Board.canvas")).unwrap(),
        source
    );
    vault.rename_entry("Old.md", "New", false).unwrap();
    assert_eq!(
        fs::read_to_string(root.join("Board.canvas")).unwrap(),
        source
    );
}

#[test]
fn invalid_ambiguous_and_oversize_boards_stay_unchanged_with_explicit_warnings() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::write(root.join("Old.md"), "# Old").unwrap();
    let invalid = "{\"nodes\": [";
    let ambiguous = r#"{"nodes":[{"type":"file","file":"Old.md","file":"Other.md"}]}"#;
    let oversize = " ".repeat(16 * 1024 * 1024 + 1);
    for (name, source) in [
        ("Bad.canvas", invalid),
        ("Duplicate.canvas", ambiguous),
        ("Large.canvas", &oversize),
    ] {
        fs::write(root.join(name), source).unwrap();
    }
    let mutation = Vault::open(root)
        .unwrap()
        .rename_entry("Old.md", "New", true)
        .unwrap();
    assert_eq!(mutation.path, "New.md");
    assert!(root.join("New.md").exists());
    assert_eq!(mutation.link_warnings.len(), 3);
    assert!(
        mutation
            .link_warnings
            .iter()
            .any(|warning| warning.path == "Large.canvas"
                && warning.reason == LinkUpdateReason::Limit)
    );
    assert!(
        mutation
            .link_warnings
            .iter()
            .any(|warning| warning.path == "Bad.canvas"
                && warning.reason == LinkUpdateReason::InvalidCanvas)
    );
    for (name, source) in [
        ("Bad.canvas", invalid),
        ("Duplicate.canvas", ambiguous),
        ("Large.canvas", &oversize),
    ] {
        assert_eq!(fs::read_to_string(root.join(name)).unwrap(), source);
    }
}

#[test]
fn changed_boards_and_markdown_are_not_overwritten_by_a_prepared_plan() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::create_dir(root.join("Folder")).unwrap();
    fs::create_dir(root.join("Destination")).unwrap();
    fs::write(root.join("Folder/Old.md"), "# Old").unwrap();
    fs::write(root.join("Links.md"), "[[Folder/Old]]").unwrap();
    fs::write(
        root.join("Folder/Board.canvas"),
        r#"{"nodes":[{"type":"file","file":"Folder/Old.md"}]}"#,
    )
    .unwrap();
    let vault = Vault::open(root).unwrap();
    let plan = vault
        .prepare_link_updates("Folder", "Destination/Folder", true)
        .unwrap();
    let outside = r#"{"nodes":[],"external":"do not clobber"}"#;
    fs::write(root.join("Folder/Board.canvas"), outside).unwrap();
    fs::write(root.join("Links.md"), "External note edit").unwrap();
    vault
        .move_entry("Folder", Some("Destination"), false)
        .unwrap();
    let warnings = vault.apply_link_updates(plan);
    assert_eq!(warnings.len(), 2);
    assert!(
        warnings
            .iter()
            .all(|warning| warning.reason == LinkUpdateReason::Changed)
    );
    assert_eq!(
        fs::read_to_string(root.join("Destination/Folder/Board.canvas")).unwrap(),
        outside
    );
    assert_eq!(
        fs::read_to_string(root.join("Links.md")).unwrap(),
        "External note edit"
    );
}

#[test]
fn unsafe_references_and_private_boards_are_not_rewritten() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::create_dir(root.join(".private")).unwrap();
    fs::write(root.join("Old.md"), "# Old").unwrap();
    let public = r#"{"nodes":[{"type":"file","file":"../Old.md"},{"type":"file","file":"C:\\Old.md"},{"type":"file","file":"/Old.md"},{"type":"file","file":".private/Old.md"},{"type":"file","file":"https://example.invalid/Old.md"},{"type":"file","file":"Old.md"}]}"#;
    let private = r#"{"nodes":[{"type":"file","file":"Old.md"}]}"#;
    fs::write(root.join("Board.canvas"), public).unwrap();
    fs::write(root.join(".private/Board.canvas"), private).unwrap();
    let mutation = Vault::open(root)
        .unwrap()
        .rename_entry("Old.md", "New", true)
        .unwrap();
    assert!(mutation.link_warnings.is_empty());
    assert_eq!(
        fs::read_to_string(root.join("Board.canvas")).unwrap(),
        public.replace("\"file\":\"Old.md\"", "\"file\":\"New.md\"")
    );
    assert_eq!(
        fs::read_to_string(root.join(".private/Board.canvas")).unwrap(),
        private
    );
}

#[test]
fn managed_task_identity_dates_and_description_survive_folder_and_note_moves() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::create_dir(root.join("Archive")).unwrap();
    let vault = Vault::open(root).unwrap();
    let created = vault.create_task("Demo", Some("2026-10-20")).unwrap();
    let task = &created.index.tasks[0];
    vault
        .update_task(
            &task.id,
            &task.path,
            task.line,
            None,
            Some(true),
            None,
            false,
            Some("2026-10-21"),
            false,
        )
        .unwrap();
    let description = "# Demo\n\n## Описание\n\nSynthetic description with [[Demo]].\n";
    fs::write(root.join(&task.path), description).unwrap();
    fs::write(
        root.join("Board.canvas"),
        r#"{"nodes":[{"type":"file","file":"Tasks/Demo.md"}]}"#,
    )
    .unwrap();
    vault.move_entry("Tasks", Some("Archive"), true).unwrap();
    vault
        .rename_entry("Archive/Tasks/Demo.md", "Renamed", true)
        .unwrap();
    let index = vault.index().unwrap();
    assert_eq!(index.tasks.len(), 1);
    assert_eq!(index.tasks[0].id, task.id);
    assert_eq!(index.tasks[0].path, "Archive/Tasks/Renamed.md");
    assert!(index.tasks[0].completed);
    assert_eq!(index.tasks[0].scheduled.as_deref(), Some("2026-10-20"));
    assert_eq!(index.tasks[0].due.as_deref(), Some("2026-10-21"));
    assert!(
        fs::read_to_string(root.join(&index.tasks[0].path))
            .unwrap()
            .contains("Synthetic description")
    );
    assert!(
        fs::read_to_string(root.join("Board.canvas"))
            .unwrap()
            .contains("Archive/Tasks/Renamed.md")
    );
}

#[test]
fn unicode_paths_escaped_json_keys_and_duplicate_node_keys_are_handled_safely() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::write(root.join("Старая.md"), "# Старая").unwrap();
    let source = r#"{"nodes":[{"type":"file","f\u0069le":"Старая.md"}],"future":1e4000}"#;
    let ambiguous = r#"{"nodes":[],"nodes":[{"type":"file","file":"Старая.md"}]}"#;
    fs::write(root.join("Board.canvas"), source).unwrap();
    fs::write(root.join("Ambiguous.canvas"), ambiguous).unwrap();
    let result = Vault::open(root)
        .unwrap()
        .rename_entry("Старая.md", "Новая", true)
        .unwrap();
    assert_eq!(
        fs::read_to_string(root.join("Board.canvas")).unwrap(),
        source.replace("Старая.md", "Новая.md")
    );
    assert_eq!(
        fs::read_to_string(root.join("Ambiguous.canvas")).unwrap(),
        ambiguous
    );
    assert_eq!(result.link_warnings.len(), 1);
    assert_eq!(result.link_warnings[0].path, "Ambiguous.canvas");
}

#[test]
fn references_to_a_board_itself_follow_rename_and_move() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::create_dir(root.join("Boards")).unwrap();
    fs::write(
        root.join("Old.canvas"),
        r#"{"nodes":[{"type":"file","file":"./Old.canvas"}],"edges":[]}"#,
    )
    .unwrap();
    fs::write(
        root.join("Other.canvas"),
        r#"{"nodes":[{"type":"file","file":"Old.canvas"}],"edges":[]}"#,
    )
    .unwrap();
    let vault = Vault::open(root).unwrap();
    assert!(
        vault
            .rename_entry("Old.canvas", "New", true)
            .unwrap()
            .link_warnings
            .is_empty()
    );
    assert!(
        vault
            .move_entry("New.canvas", Some("Boards"), true)
            .unwrap()
            .link_warnings
            .is_empty()
    );
    for path in ["Boards/New.canvas", "Other.canvas"] {
        assert_eq!(
            fs::read_to_string(root.join(path)).unwrap(),
            r#"{"nodes":[{"type":"file","file":"Boards/New.canvas"}],"edges":[]}"#
        );
    }
}

#[test]
fn a_board_above_the_node_budget_is_preserved_and_reported() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::write(root.join("Old.md"), "# Old").unwrap();
    let source = format!(
        "{{\"nodes\":[{}],\"edges\":[]}}",
        vec![r#"{"type":"file","file":"Old.md"}"#; 20_481].join(",")
    );
    fs::write(root.join("Large.canvas"), &source).unwrap();
    let result = Vault::open(root)
        .unwrap()
        .rename_entry("Old.md", "New", true)
        .unwrap();
    assert_eq!(result.link_warnings.len(), 1);
    assert_eq!(result.link_warnings[0].reason, LinkUpdateReason::Limit);
    assert_eq!(
        fs::read_to_string(root.join("Large.canvas")).unwrap(),
        source
    );
}

#[cfg(unix)]
#[test]
fn symlink_board_is_never_read_or_rewritten_even_with_an_internal_target() {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::write(root.join("Old.md"), "# Old").unwrap();
    let source = r#"{"nodes":[{"type":"file","file":"Old.md"}]}"#;
    fs::write(root.join(".private.canvas"), source).unwrap();
    symlink(root.join(".private.canvas"), root.join("Alias.canvas")).unwrap();
    Vault::open(root)
        .unwrap()
        .rename_entry("Old.md", "New", true)
        .unwrap();
    assert_eq!(
        fs::read_to_string(root.join(".private.canvas")).unwrap(),
        source
    );
}
