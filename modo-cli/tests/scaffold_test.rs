use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn modo_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_modo"))
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> (Self, PathBuf) {
        let parent =
            std::env::temp_dir().join(format!("modo-test-{}-{}", name, std::process::id()));
        if parent.exists() {
            fs::remove_dir_all(&parent).unwrap();
        }
        fs::create_dir_all(&parent).unwrap();
        let project = parent.join(name);
        (Self(parent), project)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run_new(parent: &std::path::Path, name: &str, args: &[&str]) -> std::process::Output {
    modo_bin()
        .current_dir(parent)
        .arg("new")
        .arg(name)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn scaffold_minimal() {
    let (tmp, dir) = TempDir::new("myapp");
    let output = run_new(tmp.path(), "myapp", &["-t", "minimal"]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(dir.join("Cargo.toml").exists());
    assert!(dir.join("src/main.rs").exists());
    assert!(dir.join("src/config.rs").exists());
    assert!(dir.join("config/development.yaml").exists());
    assert!(dir.join("config/production.yaml").exists());
    assert!(dir.join(".env").exists());
    assert!(dir.join(".env.example").exists());
    assert!(dir.join(".gitignore").exists());
    assert!(dir.join("CLAUDE.md").exists());
    assert!(dir.join("justfile").exists());
    // No database files
    assert!(!dir.join("docker-compose.yaml").exists());
    assert!(!dir.join("src/handlers").exists());

    // Verify Cargo.toml content
    let cargo = fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    assert!(!cargo.contains("{{"));
    assert!(!cargo.contains("modo-db"));
    assert!(cargo.contains("name = \"myapp\""));
}

#[test]
fn scaffold_api_sqlite() {
    let (tmp, dir) = TempDir::new("apiapp");
    let output = run_new(tmp.path(), "apiapp", &["-t", "api"]);
    assert!(output.status.success());

    assert!(dir.join("src/handlers/mod.rs").exists());
    assert!(dir.join("src/models/mod.rs").exists());
    assert!(!dir.join("docker-compose.yaml").exists());

    let cargo = fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    assert!(cargo.contains("sqlite"));

    // API template should NOT have jobs_database (no jobs support)
    let config_rs = fs::read_to_string(dir.join("src/config.rs")).unwrap();
    assert!(
        !config_rs.contains("jobs_database"),
        "api config.rs should NOT have jobs_database field"
    );
}

#[test]
fn scaffold_api_postgres() {
    let (tmp, dir) = TempDir::new("pgapp");
    let output = run_new(tmp.path(), "pgapp", &["-t", "api", "--postgres"]);
    assert!(output.status.success());

    assert!(dir.join("docker-compose.yaml").exists());

    let cargo = fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    assert!(cargo.contains("postgres"));

    let dc = fs::read_to_string(dir.join("docker-compose.yaml")).unwrap();
    assert!(dc.contains("postgres:18-alpine"));
}

#[test]
fn scaffold_web() {
    let (tmp, dir) = TempDir::new("webapp");
    let output = run_new(tmp.path(), "webapp", &["-t", "web"]);
    assert!(output.status.success());

    // All directories present
    assert!(dir.join("src/handlers/mod.rs").exists());
    assert!(dir.join("src/models/mod.rs").exists());
    assert!(dir.join("src/tasks/mod.rs").exists());
    assert!(dir.join("src/views/mod.rs").exists());
    assert!(dir.join("assets/src/app.css").exists());
    assert!(dir.join("templates/app/base.html").exists());
    assert!(dir.join("templates/app/index.html").exists());

    // Cargo.toml has all features
    let cargo = fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    assert!(cargo.contains("modo-auth"));
    assert!(cargo.contains("modo-session"));
    assert!(cargo.contains("modo-jobs"));
    assert!(cargo.contains("modo-email"));
    assert!(cargo.contains("modo-upload"));
    assert!(cargo.contains("modo-tenant"));

    // Config has email and i18n overrides
    let dev_cfg = fs::read_to_string(dir.join("config/development.yaml")).unwrap();
    assert!(dev_cfg.contains("templates_path: templates/emails"));
    assert!(dev_cfg.contains("path: locales"));
    assert!(dev_cfg.contains("backend: local"));
    // SQLite web template should have jobs_database section
    assert!(
        dev_cfg.contains("jobs_database:"),
        "web dev config should have jobs_database section"
    );
    assert!(dev_cfg.contains("data/jobs.db"));

    // Config should have jobs_database field
    let config_rs = fs::read_to_string(dir.join("src/config.rs")).unwrap();
    assert!(
        config_rs.contains("jobs_database"),
        "web config.rs should have jobs_database field"
    );

    // data/ directory for SQLite should exist
    assert!(
        dir.join("data").is_dir(),
        "web scaffold should create data/ directory for SQLite"
    );

    // main.rs should have dual-DB logic with group filtering
    let main_rs = fs::read_to_string(dir.join("src/main.rs")).unwrap();
    assert!(
        main_rs.contains("sync_and_migrate_group(&db, \"default\")"),
        "web main.rs should use sync_and_migrate_group with \"default\" group"
    );
    assert!(
        main_rs.contains("sync_and_migrate_group(&jobs_db, \"jobs\")"),
        "web main.rs should use sync_and_migrate_group with \"jobs\" group"
    );

    let prod_cfg = fs::read_to_string(dir.join("config/production.yaml")).unwrap();
    assert!(prod_cfg.contains("backend: s3"));
}

#[test]
fn scaffold_worker() {
    let (tmp, dir) = TempDir::new("workerapp");
    let output = run_new(tmp.path(), "workerapp", &["-t", "worker"]);
    assert!(output.status.success());

    assert!(dir.join("src/tasks/mod.rs").exists());
    assert!(!dir.join("src/handlers").exists());
    assert!(!dir.join("src/views").exists());

    // data/ directory for SQLite should exist
    assert!(
        dir.join("data").is_dir(),
        "worker scaffold should create data/ directory for SQLite"
    );

    let main_rs = fs::read_to_string(dir.join("src/main.rs")).unwrap();
    assert!(main_rs.contains("modo_jobs::new"));
    assert!(
        main_rs.contains("sync_and_migrate_group(&db, \"default\")"),
        "worker main.rs should use sync_and_migrate_group with \"default\" group"
    );
    assert!(
        main_rs.contains("sync_and_migrate_group(&jobs_db, \"jobs\")"),
        "worker main.rs should use sync_and_migrate_group with \"jobs\" group"
    );

    // Worker config should have jobs_database field
    let config_rs = fs::read_to_string(dir.join("src/config.rs")).unwrap();
    assert!(
        config_rs.contains("jobs_database"),
        "worker config.rs should have jobs_database field"
    );

    // Worker dev config should have jobs_database section (SQLite default)
    let dev_cfg = fs::read_to_string(dir.join("config/development.yaml")).unwrap();
    assert!(
        dev_cfg.contains("jobs_database:"),
        "worker dev config should have jobs_database section"
    );
}

#[test]
fn error_existing_directory() {
    let (tmp, dir) = TempDir::new("existsapp");
    fs::create_dir_all(&dir).unwrap();

    let output = run_new(tmp.path(), "existsapp", &["-t", "minimal"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("already exists"));
}

#[test]
fn error_db_flag_with_minimal() {
    let (tmp, dir) = TempDir::new("minpg");
    let output = run_new(tmp.path(), "minpg", &["-t", "minimal", "--postgres"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("does not use a database"));

    // Directory should NOT have been created
    assert!(!dir.exists());
}

#[test]
fn error_conflicting_db_flags() {
    let (tmp, _dir) = TempDir::new("conflict");
    let output = run_new(
        tmp.path(),
        "conflict",
        &["-t", "api", "--postgres", "--sqlite"],
    );
    assert!(!output.status.success());
}

#[test]
fn error_invalid_project_name() {
    let (tmp, _dir) = TempDir::new("badname");
    let output = run_new(tmp.path(), "123bad", &["-t", "minimal"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("must start with"));
}

#[test]
fn no_unrendered_placeholders() {
    let (tmp, dir) = TempDir::new("checkapp");
    let output = run_new(tmp.path(), "checkapp", &["-t", "web"]);
    assert!(output.status.success());

    // Walk all files and check for unrendered {{ }} (but skip raw Jinja in HTML templates)
    fn check_dir(dir: &std::path::Path) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().unwrap() != ".git" {
                    check_dir(&path);
                }
                continue;
            }
            // Skip HTML files (they contain MiniJinja syntax for the app)
            if path.extension() == Some(std::ffi::OsStr::new("html")) {
                continue;
            }
            let content = fs::read_to_string(&path).unwrap_or_default();
            // Check for any unrendered MiniJinja placeholder (catches both {{ x }} and {{x}})
            assert!(
                !content.contains("{{"),
                "unrendered placeholder in {}",
                path.display()
            );
        }
    }
    check_dir(&dir);
}
