use super::*;
#[test]
fn export_snapshots_are_public_bounded_read_only_and_keep_instance_scope() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Board.canvas");
    fs::write(&path, "{\"nodes\":[],\"edges\":[],\"foreign\":73}").unwrap();
    let vault = Vault::open(directory.path()).unwrap();
    let clone = vault.clone();
    assert!(vault.same_instance(&clone));
    assert!(!vault.same_instance(&Vault::open(directory.path()).unwrap()));
    let before = fs::read(&path).unwrap();
    let snapshot = vault.canvas_export_source("Board.canvas").unwrap();
    assert_eq!(snapshot.fingerprint, fingerprint(&before));
    assert_eq!(snapshot.content.as_bytes(), before);
    assert!(!directory.path().join(".brandon").exists());
    assert!(vault.canvas_export_source("../Board.canvas").is_err());
    fs::create_dir(directory.path().join(".private")).unwrap();
    fs::write(directory.path().join(".private/Secret.canvas"), &before).unwrap();
    assert!(
        vault
            .canvas_export_source(".private/Secret.canvas")
            .is_err()
    );
    fs::write(directory.path().join("Note.md"), "# Note").unwrap();
    assert!(vault.canvas_export_source("Note.md").is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
}
#[test]
fn task_snapshot_does_not_create_migrate_or_repair_a_store() {
    let directory = tempfile::tempdir().unwrap();
    let vault = Vault::open(directory.path()).unwrap();
    let ids = vec!["absent".into()];
    assert!(vault.task_records_snapshot(&ids).unwrap().is_empty());
    assert!(!directory.path().join(".brandon").exists());
    let task = vault
        .create_task_record("Source task", None)
        .unwrap()
        .record;
    let db = directory.path().join(".brandon/tasks.sqlite3");
    let before = fs::read(&db).unwrap();
    let rows = vault
        .task_records_snapshot(std::slice::from_ref(&task.id))
        .unwrap();
    assert_eq!(rows[&task.id].revision(), task.revision());
    assert_eq!(fs::read(&db).unwrap(), before);
    fs::write(
        directory.path().join(".brandon/task-store.v1.json"),
        "{\"format\":\"brandon.task-store\",\"version\":99}",
    )
    .unwrap();
    assert!(vault.task_records_snapshot(&ids).is_err());
    assert_eq!(fs::read(&db).unwrap(), before);
}
#[test]
fn export_raster_is_fingerprinted_resampled_and_never_rewrites_the_original() {
    let directory = tempfile::tempdir().unwrap();
    let vault = Vault::open(directory.path()).unwrap();
    let path = directory.path().join("Photo.png");
    image::RgbaImage::from_pixel(1200, 700, image::Rgba([12, 34, 56, 128]))
        .save(&path)
        .unwrap();
    let before = fs::read(&path).unwrap();
    let exported = vault.attachment_export_raster("Photo.png", 300).unwrap();
    assert_eq!(exported.fingerprint, fingerprint(&before));
    assert_eq!([exported.source_width, exported.source_height], [1200, 700]);
    let raster = exported.raster.unwrap();
    assert_eq!([raster.width, raster.height], [300, 175]);
    assert_eq!(&raster.rgba[..4], &[12, 34, 56, 128]);
    assert_eq!(
        vault.attachment_export_fingerprint("Photo.png").unwrap(),
        fingerprint(&before)
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    fs::write(directory.path().join("Broken.png"), b"not image data").unwrap();
    let broken = vault.attachment_export_raster("Broken.png", 300).unwrap();
    assert_eq!(broken.fingerprint, fingerprint(b"not image data"));
    assert!(broken.raster.is_none());
    assert!(vault.attachment_export_raster("Photo.png", 4097).is_err());
    assert!(vault.attachment_export_raster("../Photo.png", 300).is_err());
}
