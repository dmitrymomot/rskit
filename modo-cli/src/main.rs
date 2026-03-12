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

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::New {
            name,
            template,
            postgres,
            sqlite,
        } => {
            if (postgres || sqlite) && !template.uses_db() {
                anyhow::bail!(
                    "minimal template does not use a database. Remove --postgres/--sqlite flags."
                );
            }

            let target = std::path::Path::new(&name);
            if target.exists() {
                anyhow::bail!("directory '{}' already exists", name);
            }

            let db_driver = if postgres { "postgres" } else { "sqlite" };
            let template_name = template.to_string();

            let template_dir = templates::get(&template_name)
                .ok_or_else(|| anyhow::anyhow!("unknown template: {}", template_name))?;
            let shared_dir = templates::shared();

            let mut context = std::collections::HashMap::new();
            context.insert("project_name", name.as_str());
            context.insert("db_driver", db_driver);

            std::fs::create_dir_all(target)?;
            scaffold::scaffold(target, template_dir, shared_dir, &context)?;

            // git init
            std::process::Command::new("git")
                .arg("init")
                .current_dir(target)
                .output()?;

            println!(
                "Created modo project '{}' with template '{}' ({})\n",
                name, template_name, db_driver
            );

            println!("Next steps:");
            println!("  cd {}", name);
            if matches!(template, Template::Web) {
                println!("  just tailwind-download   # download Tailwind CSS CLI");
                println!("  just assets-download     # download HTMX, Alpine.js");
                println!("  just css                 # build CSS");
            }
            println!("  just dev                 # start dev server");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_new_with_all_args() {
        let cli = Cli::parse_from(["modo", "new", "myapp", "-t", "api", "--postgres"]);
        match cli.command {
            Commands::New {
                name,
                template,
                postgres,
                ..
            } => {
                assert_eq!(name, "myapp");
                assert!(matches!(template, Template::Api));
                assert!(postgres);
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
                ..
            } => {
                assert!(matches!(template, Template::Web));
                assert!(!postgres);
                assert!(!sqlite);
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
}
