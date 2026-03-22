use std::fs;

fn read_file(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("failed to read {path}: {error}"))
}

#[test]
fn app_hides_nostr_blog_preset_flow() {
    let app_js = read_file(&format!("{}/app/app.js", env!("CARGO_MANIFEST_DIR")));
    assert!(!app_js.contains("Apply nostr-blog preset"));
    assert!(app_js.contains("Site author pubkey"));
}

#[test]
fn diagnostics_surfaces_mirror_and_retention_health() {
    let app_js = read_file(&format!("{}/app/app.js", env!("CARGO_MANIFEST_DIR")));
    assert!(app_js.contains("Mirror health"));
    assert!(app_js.contains("Retention health"));
    assert!(app_js.contains("Failed to load relay health"));
}

#[test]
fn diagnostics_styles_exist() {
    let style_css = read_file(&format!("{}/app/style.css", env!("CARGO_MANIFEST_DIR")));
    assert!(style_css.contains(".diagnostic-item"));
    assert!(style_css.contains(".diagnostic-kv-row"));
    assert!(style_css.contains(".diagnostics-alert"));
}

#[test]
fn checkbox_labels_are_click_targets() {
    let app_js = read_file(&format!("{}/app/app.js", env!("CARGO_MANIFEST_DIR")));
    let style_css = read_file(&format!("{}/app/style.css", env!("CARGO_MANIFEST_DIR")));
    assert!(app_js.contains("function bindCheckboxLabel(label, input)"));
    assert!(app_js.contains("label.htmlFor = inputId;"));
    assert!(app_js.contains("if (checked) {\n          state.backgroundMode = true;\n        }"));
    assert!(app_js.contains("__wizardry_host_status_item_state"));
    assert!(style_css.contains(".field.checkbox-field label {"));
    assert!(style_css.contains("cursor: pointer;"));
}
