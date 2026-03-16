use anyhow::{Context, Result};
use include_dir::Dir;
use minijinja::Environment;
use std::fs;
use std::path::Path;

/// Template variables passed to every MiniJinja template during scaffolding.
pub struct ScaffoldContext<'a> {
    /// The project name, used as the crate name and in generated file content.
    pub project_name: &'a str,
    /// Database driver to activate: `"postgres"`, `"sqlite"`, or `""` for no DB.
    pub db_driver: &'a str,
    /// Whether to use S3 storage with RustFS.
    pub s3: bool,
}

/// Renders `template_dir` (and `shared_dir`) into `target_dir` using `context`.
///
/// Shared files are written first so template-specific files can override them.
/// Files with a `.jinja` extension are rendered through MiniJinja and the
/// extension is stripped from the output path. Conditional `.jinja` files that
/// render to empty output are skipped (e.g. `docker-compose.yaml.jinja` when
/// using SQLite).
pub fn scaffold(
    target_dir: &Path,
    template_dir: &Dir<'static>,
    shared_dir: &Dir<'static>,
    context: &ScaffoldContext<'_>,
) -> Result<()> {
    let env = Environment::new();

    // Process shared files first, then template-specific files
    write_dir(&env, shared_dir, target_dir, context, Path::new(""))?;
    write_dir(&env, template_dir, target_dir, context, Path::new(""))?;

    Ok(())
}

/// Recursively writes all files in `dir` into `target_dir`, rooted at `prefix`.
///
/// Renders `.jinja` files through MiniJinja and strips the `.jinja` extension.
/// Skips a `.jinja` file when it renders to empty output AND its source contains
/// a `{% if %}` block (so unconditionally empty stubs are still written).
fn write_dir(
    env: &Environment,
    dir: &Dir<'static>,
    target_dir: &Path,
    context: &ScaffoldContext<'_>,
    prefix: &Path,
) -> Result<()> {
    for file in dir.files() {
        let file_name = file
            .path()
            .file_name()
            .expect("include_dir path missing file_name");
        let relative = prefix.join(file_name);
        let is_jinja = relative.extension().is_some_and(|ext| ext == "jinja");

        let content = file
            .contents_utf8()
            .with_context(|| format!("non-UTF8 template: {}", relative.display()))?;

        // Render through MiniJinja
        let rendered = env
            .render_str(
                content,
                minijinja::context! {
                    project_name => context.project_name,
                    db_driver => context.db_driver,
                    s3 => context.s3,
                },
            )
            .with_context(|| format!("render failed: {}", relative.display()))?;

        // Skip files that render to empty (conditional files like docker-compose for sqlite)
        // but only when the source contains conditionals — unconditionally empty files
        // (like mod.rs stubs) should still be written out.
        if is_jinja && rendered.trim().is_empty() && content.contains("{% if") {
            continue;
        }

        // Strip .jinja extension
        let out_path = if is_jinja {
            relative.with_extension("")
        } else {
            relative.clone()
        };

        let dest = target_dir.join(&out_path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create dir: {}", parent.display()))?;
        }
        fs::write(&dest, rendered).with_context(|| format!("write file: {}", dest.display()))?;
    }

    // Recurse into subdirectories with updated prefix
    for subdir in dir.dirs() {
        let dir_name = subdir
            .path()
            .file_name()
            .expect("include_dir path missing file_name");
        let sub_prefix = prefix.join(dir_name);
        write_dir(env, subdir, target_dir, context, &sub_prefix)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use minijinja::Environment;

    #[test]
    fn scaffold_preserves_email_runtime_vars() {
        let env = Environment::new();
        let source = include_str!("../templates/web/templates/emails/welcome.md.jinja");
        let rendered = env
            .render_str(
                source,
                minijinja::context! {
                    project_name => "my_app",
                    db_driver => "postgres",
                    s3 => false,
                },
            )
            .unwrap();

        assert!(
            rendered.contains("{{name}}"),
            "runtime var {{{{name}}}} must survive scaffolding"
        );
        assert!(
            rendered.contains("{{dashboard_url}}"),
            "runtime var {{{{dashboard_url}}}} must survive scaffolding"
        );
        assert!(
            rendered.contains("{{project_name}}"),
            "runtime var {{{{project_name}}}} must survive scaffolding"
        );
    }

    #[test]
    fn scaffold_preserves_email_layout_runtime_vars() {
        let env = Environment::new();
        let source = include_str!("../templates/web/templates/emails/layouts/default.html.jinja");
        let rendered = env
            .render_str(
                source,
                minijinja::context! {
                    project_name => "my_app",
                    db_driver => "postgres",
                    s3 => false,
                },
            )
            .unwrap();

        assert!(
            rendered.contains("{{subject}}"),
            "runtime var {{{{subject}}}} must survive scaffolding"
        );
        assert!(
            rendered.contains("{{content}}"),
            "runtime var {{{{content}}}} must survive scaffolding"
        );
    }
}
