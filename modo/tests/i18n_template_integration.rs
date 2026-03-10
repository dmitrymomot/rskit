#![cfg(all(feature = "i18n", feature = "templates"))]

use minijinja::{Environment, context};
use modo::i18n::{I18nConfig, load, register_template_functions};
use std::fs;
use std::path::PathBuf;

fn setup(name: &str) -> (std::sync::Arc<modo::i18n::TranslationStore>, PathBuf) {
    let dir = std::env::temp_dir().join(format!("modo_i18n_tmpl_test_{name}"));
    let _ = fs::remove_dir_all(&dir);
    let en = dir.join("en");
    fs::create_dir_all(&en).unwrap();
    fs::write(
        en.join("common.yml"),
        r#"
greeting: "Hello, {name}!"
title: "Welcome"
items_count:
  zero: "No items"
  one: "One item"
  other: "{count} items"
"#,
    )
    .unwrap();

    let config = I18nConfig {
        path: dir.to_str().unwrap().to_string(),
        default_lang: "en".to_string(),
        ..Default::default()
    };
    let store = load(&config).unwrap();
    (store, dir)
}

#[test]
fn t_function_simple_key() {
    let (store, dir) = setup("simple");
    let mut env = Environment::new();
    register_template_functions(&mut env, store);

    env.add_template("test", "{{ t('common.title') }}").unwrap();
    let tmpl = env.get_template("test").unwrap();
    let result = tmpl.render(context! { locale => "en" }).unwrap();
    assert_eq!(result, "Welcome");

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn t_function_with_variable() {
    let (store, dir) = setup("var");
    let mut env = Environment::new();
    register_template_functions(&mut env, store);

    env.add_template("test", "{{ t('common.greeting', name='World') }}")
        .unwrap();
    let tmpl = env.get_template("test").unwrap();
    let result = tmpl.render(context! { locale => "en" }).unwrap();
    assert_eq!(result, "Hello, World!");

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn t_function_plural() {
    let (store, dir) = setup("plural");
    let mut env = Environment::new();
    register_template_functions(&mut env, store);

    env.add_template("test", "{{ t('common.items_count', count=0) }} | {{ t('common.items_count', count=1) }} | {{ t('common.items_count', count=5) }}")
        .unwrap();
    let tmpl = env.get_template("test").unwrap();
    let result = tmpl.render(context! { locale => "en" }).unwrap();
    assert_eq!(result, "No items | One item | 5 items");

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn t_function_missing_key_returns_key() {
    let (store, dir) = setup("missing");
    let mut env = Environment::new();
    register_template_functions(&mut env, store);

    env.add_template("test", "{{ t('nonexistent.key') }}")
        .unwrap();
    let tmpl = env.get_template("test").unwrap();
    let result = tmpl.render(context! { locale => "en" }).unwrap();
    assert_eq!(result, "nonexistent.key");

    fs::remove_dir_all(&dir).unwrap();
}
