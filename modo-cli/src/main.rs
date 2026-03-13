use clap::{Parser, Subcommand, ValueEnum};

mod scaffold;
mod templates;

#[derive(Parser)]
#[command(name = "modo", version, about = "modo framework CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new modo project
    New {
        /// Project name (used as directory and crate name)
        name: String,

        /// Template preset
        #[arg(short, long, default_value = "web")]
        template: Template,

        /// Use PostgreSQL database driver
        #[arg(long, conflicts_with = "sqlite")]
        postgres: bool,

        /// Use SQLite database driver (default)
        #[arg(long, conflicts_with = "postgres")]
        sqlite: bool,

        /// Use S3 storage with RustFS (local S3-compatible server)
        #[arg(long)]
        s3: bool,
    },
}

#[derive(Clone, ValueEnum)]
enum Template {
    Minimal,
    Api,
    Web,
    Worker,
}

impl std::fmt::Display for Template {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Template::Minimal => write!(f, "minimal"),
            Template::Api => write!(f, "api"),
            Template::Web => write!(f, "web"),
            Template::Worker => write!(f, "worker"),
        }
    }
}

impl Template {
    fn uses_db(&self) -> bool {
        !matches!(self, Template::Minimal)
    }
}

// Strict keywords + reserved-for-future-use keywords
const RUST_KEYWORDS: &[&str] = &[
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "crate",
    "do", "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "if", "impl", "in",
    "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "true", "try", "type",
    "typeof", "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
];

fn validate_project_name(name: &str) -> anyhow::Result<()> {
    if name.is_empty() {
        anyhow::bail!("project name cannot be empty");
    }
    let first = name.as_bytes()[0];
    if !(first.is_ascii_alphabetic() || first == b'_') {
        anyhow::bail!(
            "project name must start with an ASCII letter or underscore, got '{}'",
            name
        );
    }
    if let Some(bad) = name
        .chars()
        .find(|c| !matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-'))
    {
        anyhow::bail!(
            "project name contains invalid character '{}' (only [a-zA-Z0-9_-] allowed)",
            bad
        );
    }
    if RUST_KEYWORDS.contains(&name) {
        anyhow::bail!(
            "'{}' is a Rust keyword and cannot be used as a project name",
            name
        );
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::New {
            name,
            template,
            postgres,
            sqlite,
            s3,
        } => {
            validate_project_name(&name)?;

            if (postgres || sqlite) && !template.uses_db() {
                anyhow::bail!(
                    "minimal template does not use a database. Remove --postgres/--sqlite flags."
                );
            }

            if s3 && !matches!(template, Template::Web) {
                anyhow::bail!("--s3 flag is only supported with the web template");
            }

            let target = std::path::Path::new(&name);
            if target.exists() {
                anyhow::bail!("directory '{}' already exists", name);
            }

            let db_driver = if !template.uses_db() {
                ""
            } else if postgres {
                "postgres"
            } else {
                "sqlite"
            };
            let template_name = template.to_string();

            let template_dir = templates::get(&template_name)
                .ok_or_else(|| anyhow::anyhow!("unknown template: {}", template_name))?;
            let shared_dir = templates::shared();

            let context = scaffold::ScaffoldContext {
                project_name: &name,
                db_driver,
                s3,
            };

            std::fs::create_dir_all(target)?;
            if let Err(e) = scaffold::scaffold(target, template_dir, shared_dir, &context) {
                let _ = std::fs::remove_dir_all(target);
                return Err(e);
            }

            // git init
            let status = std::process::Command::new("git")
                .arg("init")
                .current_dir(target)
                .status()?;
            if !status.success() {
                eprintln!("warning: git init failed (exit {})", status);
            }

            if db_driver.is_empty() {
                println!(
                    "Created modo project '{}' with template '{}'\n",
                    name, template_name
                );
            } else {
                println!(
                    "Created modo project '{}' with template '{}' ({})\n",
                    name, template_name, db_driver
                );
            }

            println!("Next steps:");
            println!("  cd {}", name);
            if matches!(template, Template::Web) {
                println!("  just assets-download     # download HTMX, Alpine.js (first time only)");
            }
            println!("  just dev                 # start dev server");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_new_with_all_args() {
        let cli = Cli::parse_from(["modo", "new", "myapp", "-t", "api", "--postgres"]);
        match cli.command {
            Commands::New {
                name,
                template,
                postgres,
                s3,
                ..
            } => {
                assert_eq!(name, "myapp");
                assert!(matches!(template, Template::Api));
                assert!(postgres);
                assert!(!s3);
            }
        }
    }

    #[test]
    fn parse_new_defaults() {
        let cli = Cli::parse_from(["modo", "new", "myapp"]);
        match cli.command {
            Commands::New {
                template,
                postgres,
                sqlite,
                s3,
                ..
            } => {
                assert!(matches!(template, Template::Web));
                assert!(!postgres);
                assert!(!sqlite);
                assert!(!s3);
            }
        }
    }

    #[test]
    fn parse_new_with_s3() {
        let cli = Cli::parse_from(["modo", "new", "myapp", "--s3"]);
        match cli.command {
            Commands::New { s3, .. } => {
                assert!(s3);
            }
        }
    }

    #[test]
    fn template_uses_db() {
        assert!(!Template::Minimal.uses_db());
        assert!(Template::Api.uses_db());
        assert!(Template::Web.uses_db());
        assert!(Template::Worker.uses_db());
    }

    #[test]
    fn conflicting_db_flags_rejected() {
        let result = Cli::try_parse_from(["modo", "new", "myapp", "--postgres", "--sqlite"]);
        assert!(result.is_err());
    }

    #[test]
    fn valid_project_names() {
        assert!(validate_project_name("myapp").is_ok());
        assert!(validate_project_name("my_app").is_ok());
        assert!(validate_project_name("my-app").is_ok());
        assert!(validate_project_name("_private").is_ok());
        assert!(validate_project_name("App123").is_ok());
    }

    #[test]
    fn invalid_project_names() {
        assert!(validate_project_name("").is_err());
        assert!(validate_project_name("123abc").is_err());
        assert!(validate_project_name("-dash").is_err());
        assert!(validate_project_name("has space").is_err());
        assert!(validate_project_name("a/b").is_err());
        assert!(validate_project_name("fn").is_err());
        assert!(validate_project_name("struct").is_err());
        assert!(validate_project_name("try").is_err());
        assert!(validate_project_name("abstract").is_err());
    }
}
