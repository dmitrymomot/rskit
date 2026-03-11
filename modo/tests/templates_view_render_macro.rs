#![cfg(feature = "templates")]

use minijinja::Value;
use modo::templates::{engine, TemplateConfig, TemplateContext, TemplateEngine, ViewRender};
use std::io::Write;
use tempfile::TempDir;

fn setup_engine(templates: &[(&str, &str)]) -> (TempDir, TemplateEngine) {
    let dir = TempDir::new().unwrap();
    for (name, content) in templates {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }
    let config = TemplateConfig {
        path: dir.path().to_string_lossy().to_string(),
        ..Default::default()
    };
    let eng = engine(&config).unwrap();
    (dir, eng)
}

#[modo::view("test.html")]
struct SimpleView {
    name: String,
}

#[modo::view("page.html", htmx = "partial.html")]
struct DualView {
    title: String,
}

#[test]
fn simple_view_implements_view_render() {
    let (_dir, eng) = setup_engine(&[("test.html", "Hello {{ name }}!")]);
    let ctx = TemplateContext::new();
    let view = SimpleView { name: "World".into() };

    let html = view.render_with(&eng, &ctx, false).unwrap();
    assert_eq!(html, "Hello World!");
}

#[test]
fn simple_view_has_no_dual_template() {
    let view = SimpleView { name: "test".into() };
    assert!(!view.has_dual_template());
}

#[test]
fn dual_view_selects_htmx_template() {
    let (_dir, eng) = setup_engine(&[
        ("page.html", "Full: {{ title }}"),
        ("partial.html", "Partial: {{ title }}"),
    ]);
    let ctx = TemplateContext::new();
    let view = DualView { title: "Test".into() };

    let full = view.render_with(&eng, &ctx, false).unwrap();
    assert_eq!(full, "Full: Test");

    let partial = view.render_with(&eng, &ctx, true).unwrap();
    assert_eq!(partial, "Partial: Test");
}

#[test]
fn dual_view_has_dual_template() {
    let view = DualView { title: "test".into() };
    assert!(view.has_dual_template());
}

#[test]
fn view_render_merges_request_context() {
    let (_dir, eng) = setup_engine(&[("test.html", "{{ name }} ({{ csrf_token }})")]);
    let mut ctx = TemplateContext::new();
    ctx.insert("csrf_token", Value::from("abc123"));
    let view = SimpleView { name: "World".into() };

    let html = view.render_with(&eng, &ctx, false).unwrap();
    assert_eq!(html, "World (abc123)");
}
