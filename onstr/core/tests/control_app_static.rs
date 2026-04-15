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
    assert!(index_html.contains("id=\"rail-resizer\""));
    assert!(style_css.contains(".rail-resizer"));
    assert!(style_css.contains(".setup-panel"));
    assert!(style_css.contains(".setup-status-list"));
}

#[test]
fn rail_listboxes_stay_focusable_with_selection_cards() {
    let index_html = read_file(&format!("{}/../app/index.html", env!("CARGO_MANIFEST_DIR")));
    let app_js = read_file(&format!("{}/../app/app.js", env!("CARGO_MANIFEST_DIR")));

    assert!(index_html.contains("id=\"following-listbox\" class=\"rail-listbox\" role=\"listbox\" tabindex=\"0\""));
    assert!(index_html.contains("id=\"library-listbox\" class=\"rail-listbox\" role=\"listbox\" tabindex=\"0\""));
    assert!(index_html.contains("id=\"relay-listbox\" class=\"rail-listbox settings-listbox\" role=\"listbox\" tabindex=\"0\""));
    assert!(index_html.contains("id=\"relay-selection-card\""));
    assert!(index_html.contains("id=\"library-selection-card\""));
    assert!(app_js.contains("bindListboxKeyboard(els.followingListbox"));
    assert!(app_js.contains("bindListboxKeyboard(els.relayListbox"));
    assert!(app_js.contains("bindListboxKeyboard(els.libraryListbox"));
}

#[test]
fn library_maintenance_lives_in_settings_not_the_rail() {
    let index_html = read_file(&format!("{}/../app/index.html", env!("CARGO_MANIFEST_DIR")));

    assert!(index_html.contains("Library Maintenance"));
    assert!(index_html.contains("id=\"library-reindex\""));
    assert!(index_html.contains("id=\"library-ingest-form\""));
    assert!(!index_html.contains("id=\"library-event-id\""));
}

#[test]
fn first_run_surface_offers_recommended_relays_notice() {
    let index_html = read_file(&format!("{}/../app/index.html", env!("CARGO_MANIFEST_DIR")));
    let app_js = read_file(&format!("{}/../app/app.js", env!("CARGO_MANIFEST_DIR")));

    assert!(index_html.contains("id=\"recommended-relays-notice\""));
    assert!(index_html.contains("id=\"recommended-relays-add\""));
    assert!(index_html.contains("id=\"settings-add-recommended-relays\""));
    assert!(index_html.contains("id=\"settings-recommended-relays-list\""));
    assert!(!index_html.contains("id=\"rail-add-recommended-relays\""));
    assert!(!index_html.contains("id=\"rail-recommended-relays-list\""));
    assert!(app_js.contains("RECOMMENDED_RELAYS = Object.freeze"));
    assert!(app_js.contains("function addRecommendedRelays()"));
}

#[test]
fn backend_prefs_expose_recommended_relays_notice_state() {
    let backend_sh = read_file(&format!("{}/../app/scripts/onstr-backend.sh", env!("CARGO_MANIFEST_DIR")));

    assert!(backend_sh.contains("rail_width=$(pref_get rail_width"));
    assert!(backend_sh.contains("printf 'rail_width=%s\\n'"));
    assert!(backend_sh.contains("recommended_relays_notice=$(pref_get recommended_relays_notice"));
    assert!(backend_sh.contains("printf 'recommended_relays_notice=%s\\n'"));
}

#[test]
fn note_compose_does_not_inject_title_shorthand_tags() {
    let app_js = read_file(&format!("{}/../app/app.js", env!("CARGO_MANIFEST_DIR")));

    assert!(!app_js.contains("composeTagsWithName"));
    assert!(!app_js.contains("title:"));
}
