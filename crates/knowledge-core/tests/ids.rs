use knowledge_core::ids::{PageId, UserId};

#[test]
fn typed_id_round_trips_through_json_without_losing_its_type() {
    let user_id = UserId::new();
    let encoded = serde_json::to_string(&user_id).expect("typed ID should serialize");
    let decoded: UserId = serde_json::from_str(&encoded).expect("typed ID should deserialize");

    assert_eq!(decoded, user_id);
    assert_eq!(decoded.to_string(), user_id.as_uuid().to_string());
}

#[test]
fn distinct_entity_ids_cannot_be_compared_without_explicit_uuid_conversion() {
    let page_id = PageId::new();
    let user_id = UserId::from_uuid(page_id.into_uuid());

    assert_eq!(user_id.as_uuid(), page_id.as_uuid());
}
