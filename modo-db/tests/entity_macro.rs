use modo_db::EntityRegistration;

// --- Basic entity ---

#[modo_db::entity(table = "test_users")]
pub struct TestUser {
    #[entity(primary_key)]
    pub id: i32,
    #[entity(unique)]
    pub email: String,
    #[entity(indexed)]
    pub username: String,
    #[entity(nullable)]
    pub avatar_url: Option<String>,
    #[entity(column_type = "Text")]
    pub bio: String,
    #[entity(default_value = 0)]
    pub credits: i32,
}

#[test]
fn test_basic_entity_registers() {
    let registrations: Vec<&EntityRegistration> = inventory::iter::<EntityRegistration>().collect();
    let tables: Vec<&str> = registrations.iter().map(|r| r.table_name).collect();
    assert!(
        tables.contains(&"test_users"),
        "test_users not registered. Found: {tables:?}"
    );
}

#[test]
fn test_basic_entity_module_types_exist() {
    let _: test_user::Model = test_user::Model {
        id: 1,
        email: "test@example.com".to_string(),
        username: "test".to_string(),
        avatar_url: None,
        bio: "hello".to_string(),
        credits: 0,
    };
}

#[test]
fn test_basic_entity_preserved_struct() {
    // The macro preserves the struct alongside the module
    let user = TestUser {
        id: 0,
        email: String::new(),
        username: String::new(),
        avatar_url: None,
        bio: String::new(),
        credits: 0,
    };
    assert_eq!(user.id, 0);
    assert!(user.email.is_empty());
}

#[test]
fn test_basic_entity_default() {
    let user = TestUser::default();
    assert_eq!(user.id, 0);
    assert!(user.email.is_empty());
    assert!(user.username.is_empty());
    assert!(user.avatar_url.is_none());
    assert!(user.bio.is_empty());
    assert_eq!(user.credits, 0);
}

#[test]
fn test_basic_entity_from_model() {
    let model = test_user::Model {
        id: 42,
        email: "test@example.com".to_string(),
        username: "testuser".to_string(),
        avatar_url: Some("https://example.com/avatar.png".to_string()),
        bio: "hello world".to_string(),
        credits: 100,
    };
    let user = TestUser::from(model);
    assert_eq!(user.id, 42);
    assert_eq!(user.email, "test@example.com");
    assert_eq!(user.username, "testuser");
    assert_eq!(
        user.avatar_url.as_deref(),
        Some("https://example.com/avatar.png")
    );
    assert_eq!(user.bio, "hello world");
    assert_eq!(user.credits, 100);
}

#[test]
fn test_basic_entity_record_impl() {
    use modo_db::Record;
    // Test into_active_model_full
    let user = TestUser {
        id: 1,
        email: "test@example.com".to_string(),
        username: "tester".to_string(),
        avatar_url: None,
        bio: "".to_string(),
        credits: 10,
    };
    let _am: test_user::ActiveModel = user.into_active_model_full();

    // Test into_active_model (PK only)
    let user2 = TestUser {
        id: 2,
        email: "other@example.com".to_string(),
        username: "other".to_string(),
        avatar_url: None,
        bio: "".to_string(),
        credits: 0,
    };
    let _am_pk: test_user::ActiveModel = user2.into_active_model();
}

// --- Entity with belongs_to ---

#[modo_db::entity(table = "test_posts")]
pub struct TestPost {
    #[entity(primary_key)]
    pub id: i32,
    #[entity(belongs_to = "TestUser", on_delete = "Cascade")]
    pub test_user_id: i32,
    pub title: String,
}

#[test]
fn test_belongs_to_entity_registers() {
    let registrations: Vec<&EntityRegistration> = inventory::iter::<EntityRegistration>().collect();
    let tables: Vec<&str> = registrations.iter().map(|r| r.table_name).collect();
    assert!(
        tables.contains(&"test_posts"),
        "test_posts not registered. Found: {tables:?}"
    );
}

// --- Entity with timestamps ---

#[modo_db::entity(table = "test_articles")]
#[entity(timestamps)]
pub struct TestArticle {
    #[entity(primary_key)]
    pub id: i32,
    pub title: String,
}

#[test]
fn test_timestamps_entity_has_fields() {
    let now = chrono::Utc::now();
    let _: test_article::Model = test_article::Model {
        id: 1,
        title: "Test".to_string(),
        created_at: now,
        updated_at: now,
    };
}

#[test]
fn test_timestamps_preserved_struct() {
    // Timestamps should appear on the preserved struct too
    let article = TestArticle::default();
    assert_eq!(article.id, 0);
    assert!(article.title.is_empty());
    // created_at and updated_at should be set to now by default
    let _ = article.created_at;
    let _ = article.updated_at;
}

// --- Entity with timestamps + soft_delete ---

#[modo_db::entity(table = "test_items")]
#[entity(timestamps)]
#[entity(soft_delete)]
pub struct TestItem {
    #[entity(primary_key)]
    pub id: i32,
    pub name: String,
}

#[test]
fn test_soft_delete_entity_has_deleted_at() {
    let now = chrono::Utc::now();
    let _: test_item::Model = test_item::Model {
        id: 1,
        name: "Test".to_string(),
        created_at: now,
        updated_at: now,
        deleted_at: None,
    };
}

#[test]
fn test_soft_delete_preserved_struct() {
    let item = TestItem::default();
    assert_eq!(item.id, 0);
    assert!(item.name.is_empty());
    assert!(item.deleted_at.is_none());
}

#[test]
fn test_soft_delete_methods_exist_on_struct() {
    // with_deleted and only_deleted are static methods on the struct
    let _: modo_db::EntityQuery<TestItem, test_item::Entity> = TestItem::with_deleted();
    let _: modo_db::EntityQuery<TestItem, test_item::Entity> = TestItem::only_deleted();
}

#[test]
fn test_soft_delete_deleted_at_index() {
    let registrations: Vec<&EntityRegistration> = inventory::iter::<EntityRegistration>().collect();
    let reg = registrations
        .iter()
        .find(|r| r.table_name == "test_items")
        .expect("test_items not registered");
    // Should have a deleted_at index
    assert!(
        reg.extra_sql.iter().any(|s| s.contains("deleted_at")),
        "Expected deleted_at index. Found: {:?}",
        reg.extra_sql
    );
}

// --- Entity with composite index ---

#[modo_db::entity(table = "test_indexed")]
#[entity(index(columns = ["user_id", "created_at"]))]
#[entity(index(columns = ["slug"], unique))]
pub struct TestIndexed {
    #[entity(primary_key)]
    pub id: i32,
    pub user_id: i32,
    pub slug: String,
    pub created_at: String,
}

#[test]
fn test_composite_index_generates_extra_sql() {
    let registrations: Vec<&EntityRegistration> = inventory::iter::<EntityRegistration>().collect();
    let reg = registrations
        .iter()
        .find(|r| r.table_name == "test_indexed")
        .expect("test_indexed not registered");
    assert_eq!(reg.extra_sql.len(), 2);
    assert!(
        reg.extra_sql
            .iter()
            .any(|s| s.contains("idx_test_indexed_user_id_created_at")),
        "Expected composite index. Found: {:?}",
        reg.extra_sql
    );
    assert!(
        reg.extra_sql
            .iter()
            .any(|s| s.contains("UNIQUE") && s.contains("idx_test_indexed_slug")),
        "Expected unique slug index. Found: {:?}",
        reg.extra_sql
    );
}

// --- Composite primary key (junction table) ---

#[modo_db::entity(table = "test_post_tags")]
pub struct TestPostTag {
    #[entity(
        primary_key,
        auto_increment = false,
        belongs_to = "TestPost",
        on_delete = "Cascade"
    )]
    pub test_post_id: i32,
    #[entity(
        primary_key,
        auto_increment = false,
        belongs_to = "TestTag",
        on_delete = "Cascade"
    )]
    pub test_tag_id: i32,
}

#[modo_db::entity(table = "test_tags")]
pub struct TestTag {
    #[entity(primary_key)]
    pub id: i32,
    #[entity(unique)]
    pub name: String,
}

#[test]
fn test_junction_table_registers() {
    let registrations: Vec<&EntityRegistration> = inventory::iter::<EntityRegistration>().collect();
    let tables: Vec<&str> = registrations.iter().map(|r| r.table_name).collect();
    assert!(tables.contains(&"test_post_tags"));
    assert!(tables.contains(&"test_tags"));
}

// --- Entity with renamed_from ---

#[modo_db::entity(table = "test_renamed")]
pub struct TestRenamed {
    #[entity(primary_key)]
    pub id: i32,
    #[entity(renamed_from = "name")]
    pub display_name: String,
}

#[test]
fn test_renamed_entity_compiles() {
    let _ = test_renamed::Model {
        id: 1,
        display_name: "test".to_string(),
    };
}

// --- Entity with auto ULID ---

#[modo_db::entity(table = "test_auto_ulid")]
pub struct TestAutoUlid {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub name: String,
}

#[test]
fn test_auto_ulid_entity_registers() {
    let registrations: Vec<&EntityRegistration> = inventory::iter::<EntityRegistration>().collect();
    let tables: Vec<&str> = registrations.iter().map(|r| r.table_name).collect();
    assert!(tables.contains(&"test_auto_ulid"));
}

#[test]
fn test_auto_ulid_model_compiles() {
    let _ = test_auto_ulid::Model {
        id: "01HXYZ".to_string(),
        name: "test".to_string(),
    };
}

#[test]
fn test_auto_ulid_default_generates_id() {
    let record = TestAutoUlid::default();
    // Default should generate a ULID for the id field
    assert!(
        !record.id.is_empty(),
        "auto ULID field should not be empty on default"
    );
    assert_eq!(record.name, "");
}

// --- Entity with auto NanoID ---

#[modo_db::entity(table = "test_auto_nanoid")]
pub struct TestAutoNanoid {
    #[entity(primary_key, auto = "nanoid")]
    pub id: String,
    pub name: String,
}

#[test]
fn test_auto_nanoid_entity_registers() {
    let registrations: Vec<&EntityRegistration> = inventory::iter::<EntityRegistration>().collect();
    let tables: Vec<&str> = registrations.iter().map(|r| r.table_name).collect();
    assert!(tables.contains(&"test_auto_nanoid"));
}

#[test]
fn test_auto_nanoid_default_generates_id() {
    let record = TestAutoNanoid::default();
    assert!(
        !record.id.is_empty(),
        "auto NanoID field should not be empty on default"
    );
}

// --- Entity with auto ULID + timestamps ---

#[modo_db::entity(table = "test_auto_ulid_ts")]
#[entity(timestamps)]
pub struct TestAutoUlidTs {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub title: String,
}

#[test]
fn test_auto_ulid_with_timestamps_compiles() {
    let now = chrono::Utc::now();
    let _ = test_auto_ulid_ts::Model {
        id: "01HXYZ".to_string(),
        title: "test".to_string(),
        created_at: now,
        updated_at: now,
    };
}

#[test]
fn test_auto_ulid_with_timestamps_default() {
    let record = TestAutoUlidTs::default();
    assert!(!record.id.is_empty());
    assert!(record.title.is_empty());
    // Timestamps should be set
    let _ = record.created_at;
    let _ = record.updated_at;
}

// --- User entity not marked as framework ---

#[test]
fn test_user_entities_not_framework() {
    let registrations: Vec<&EntityRegistration> = inventory::iter::<EntityRegistration>().collect();
    let user_reg = registrations
        .iter()
        .find(|r| r.table_name == "test_users")
        .unwrap();
    assert!(!user_reg.is_framework);
}

// --- Test Record trait apply_auto_fields ---

#[test]
fn test_apply_auto_fields_sets_id() {
    use modo_db::Record;
    // TestAutoUlid has auto = "ulid" on id field
    let mut am = test_auto_ulid::ActiveModel::default();
    TestAutoUlid::apply_auto_fields(&mut am, true);
    // After apply, id should be Set with a non-empty ULID
    match &am.id {
        modo_db::sea_orm::ActiveValue::Set(id) => {
            assert!(!id.is_empty(), "apply_auto_fields should set a ULID");
        }
        _ => panic!("apply_auto_fields should have set the id field"),
    }
}

#[test]
fn test_apply_auto_fields_timestamps() {
    use modo_db::Record;
    // TestAutoUlidTs has timestamps
    let mut am = test_auto_ulid_ts::ActiveModel::default();
    TestAutoUlidTs::apply_auto_fields(&mut am, true);
    // Both created_at and updated_at should be set
    assert!(matches!(
        am.created_at,
        modo_db::sea_orm::ActiveValue::Set(_)
    ));
    assert!(matches!(
        am.updated_at,
        modo_db::sea_orm::ActiveValue::Set(_)
    ));
}

#[test]
fn test_apply_auto_fields_update_skips_created_at() {
    use modo_db::Record;
    let mut am = test_auto_ulid_ts::ActiveModel::default();
    TestAutoUlidTs::apply_auto_fields(&mut am, false); // not insert
    // created_at should NOT be set on update
    assert!(!matches!(
        am.created_at,
        modo_db::sea_orm::ActiveValue::Set(_)
    ));
    // updated_at should still be set
    assert!(matches!(
        am.updated_at,
        modo_db::sea_orm::ActiveValue::Set(_)
    ));
}

// --- Test serde::Serialize on preserved struct ---

#[test]
fn test_preserved_struct_is_serializable() {
    // The preserved struct derives serde::Serialize, verify it works
    let user = TestUser::default();
    let json = modo::serde_json::to_value(&user);
    assert!(json.is_ok(), "Preserved struct should be serializable");
}
