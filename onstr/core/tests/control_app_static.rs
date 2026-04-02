use std::fs;

fn read_file(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("failed to read {path}: {error}"))
}

#[test]
fn settings_surface_exposes_profile_setup_controls() {
    let index_html = read_file(&format!("{}/../app/index.html", env!("CARGO_MANIFEST_DIR")));
    let app_js = read_file(&format!("{}/../app/app.js", env!("CARGO_MANIFEST_DIR")));

    assert!(index_html.contains("id=\"profile-listbox\""));
    assert!(index_html.contains("id=\"profile-create-form\""));
    assert!(index_html.contains("id=\"profile-import-form\""));
    assert!(app_js.contains("'profile-create': true"));
    assert!(app_js.contains("'profile-import': true"));
    assert!(app_js.contains("'profile-use': true"));
}

#[test]
fn home_surface_has_first_run_setup_panel() {
    let index_html = read_file(&format!("{}/../app/index.html", env!("CARGO_MANIFEST_DIR")));
    let style_css = read_file(&format!("{}/../app/style.css", env!("CARGO_MANIFEST_DIR")));

    assert!(index_html.contains("id=\"setup-panel\""));
    assert!(index_html.contains("id=\"setup-open-settings\""));
    assert!(style_css.contains(".setup-panel"));
    assert!(style_css.contains(".setup-status-list"));
}

#[test]
fn note_compose_does_not_inject_title_shorthand_tags() {
    let app_js = read_file(&format!("{}/../app/app.js", env!("CARGO_MANIFEST_DIR")));

    assert!(!app_js.contains("composeTagsWithName"));
    assert!(!app_js.contains("title:"));
}
