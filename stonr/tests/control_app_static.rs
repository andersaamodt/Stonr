use std::fs;

fn read_file(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("failed to read {path}: {error}"))
}

#[test]
fn app_hides_nostr_blog_preset_flow() {
    let app_js = read_file(&format!("{}/app/app.js", env!("CARGO_MANIFEST_DIR")));
    assert!(!app_js.contains("Apply nostr-blog preset"));
    assert!(app_js.contains("Owner authors"));
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
    assert!(app_js.contains("state.backgroundMode = checked;"));
    assert!(app_js.contains("__wizardry_host_status_item_state"));
    assert!(style_css.contains(".field.checkbox-field label {"));
    assert!(style_css.contains("cursor: pointer;"));
}

#[test]
fn events_refresh_uses_single_flight_and_non_blocking_stats() {
    let app_js = read_file(&format!("{}/app/app.js", env!("CARGO_MANIFEST_DIR")));
    assert!(app_js.contains("refreshInFlight: false"));
    assert!(app_js.contains("if (state.refreshInFlight)"));
    assert!(app_js.contains("eventsStatsPromise: null"));
    assert!(app_js.contains("function refreshEventsStats()"));
    assert!(app_js.contains("refreshEventsStats().catch(function (error)"));
}

#[test]
fn splash_uses_stonr_logo_asset() {
    let index_html = read_file(&format!("{}/app/index.html", env!("CARGO_MANIFEST_DIR")));
    assert!(index_html.contains("class=\"boot-splash\""));
    assert!(index_html.contains("assets/icons/web/icon-192.png"));
    assert!(!index_html.contains("<div class=\"boot-mark\">\n      <svg"));
}

#[test]
fn app_support_section_and_locks_exist() {
    let app_js = read_file(&format!("{}/app/app.js", env!("CARGO_MANIFEST_DIR")));
    let style_css = read_file(&format!("{}/app/style.css", env!("CARGO_MANIFEST_DIR")));
    assert!(app_js.contains("id: 'app-support'"));
    assert!(app_js.contains("Turn off support in App Support to unlock it."));
    assert!(app_js.contains("function renderAppSupportSection()"));
    assert!(style_css.contains(".app-support-listbox"));
    assert!(style_css.contains(".app-support-option.selected"));
}
