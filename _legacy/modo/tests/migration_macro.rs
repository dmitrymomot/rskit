use modo::db::MigrationRegistration;

#[modo::migration(version = 9000, description = "Test migration")]
async fn test_migration(db: &sea_orm::DatabaseConnection) -> Result<(), modo::error::Error> {
    let _ = db;
    Ok(())
}

#[modo::migration(version = 9001, description = "Another test migration")]
async fn another_test(db: &sea_orm::DatabaseConnection) -> Result<(), modo::error::Error> {
    let _ = db;
    Ok(())
}

#[test]
fn test_migration_macro_registers() {
    let migrations: Vec<&MigrationRegistration> =
        inventory::iter::<MigrationRegistration>().collect();
    let versions: Vec<u64> = migrations.iter().map(|m| m.version).collect();
    assert!(
        versions.contains(&9000),
        "migration v9000 not registered. Found: {versions:?}"
    );
    assert!(
        versions.contains(&9001),
        "migration v9001 not registered. Found: {versions:?}"
    );
}

#[test]
fn test_migration_descriptions() {
    let migrations: Vec<&MigrationRegistration> =
        inventory::iter::<MigrationRegistration>().collect();
    let m = migrations.iter().find(|m| m.version == 9000).unwrap();
    assert_eq!(m.description, "Test migration");
}
