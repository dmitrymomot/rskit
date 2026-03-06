use modo::db::EntityRegistration;

// --- Basic entity ---

#[modo::entity(table = "test_users")]
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
    // Verify the generated module has the expected types
    let _: test_user::Model = test_user::Model {
        id: 1,
        email: "test@example.com".to_string(),
        username: "test".to_string(),
        avatar_url: None,
        bio: "hello".to_string(),
        credits: 0,
    };
}

// --- Entity with belongs_to ---

#[modo::entity(table = "test_posts")]
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

#[modo::entity(table = "test_articles")]
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

// --- Entity with timestamps + soft_delete ---

#[modo::entity(table = "test_items")]
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
fn test_soft_delete_helpers_exist() {
    // Just verify the functions exist and have the right signatures
    let _ = test_item::find_active;
    let _ = test_item::soft_delete::<sea_orm::DatabaseConnection>;
    let _ = test_item::force_delete::<sea_orm::DatabaseConnection>;
}

// --- Entity with composite index ---

#[modo::entity(table = "test_indexed")]
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
    assert!(reg.extra_sql[0].contains("idx_test_indexed_user_id_created_at"));
    assert!(reg.extra_sql[1].contains("UNIQUE"));
    assert!(reg.extra_sql[1].contains("idx_test_indexed_slug"));
}

// --- Composite primary key (junction table) ---

#[modo::entity(table = "test_post_tags")]
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

#[modo::entity(table = "test_tags")]
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

#[modo::entity(table = "test_renamed")]
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

#[modo::entity(table = "test_auto_ulid")]
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

// --- Entity with auto NanoID ---

#[modo::entity(table = "test_auto_nanoid")]
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

// --- Entity with auto ULID + timestamps ---

#[modo::entity(table = "test_auto_ulid_ts")]
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
