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
    let app_js = read_file(&format!("{}/../app/app.js", env!("CARGO_MANIFEST_DIR")));

    assert!(index_html.contains("id=\"setup-panel\""));
    assert!(index_html.contains("id=\"setup-open-settings\""));
    assert!(index_html.contains("class=\"rail-section rail-setup\""));
    assert!(index_html.contains("id=\"rail-resizer\""));
    assert!(index_html.contains("id=\"splash\" class=\"boot-splash\""));
    assert!(index_html.contains("class=\"boot-splash-icon\" src=\"assets/forge-icon.png\""));
    assert!(index_html.contains("id=\"onstr-app\" class=\"workspace hidden\" aria-hidden=\"true\""));
    assert!(style_css.contains(".rail-resizer"));
    assert!(style_css.contains(".setup-panel"));
    assert!(style_css.contains(".rail-setup"));
    assert!(style_css.contains(".rail-setup-status-list"));
    assert!(style_css.contains(".setup-status-list"));
    assert!(style_css.contains(".boot-splash"));
    assert!(style_css.contains(".boot-splash-icon"));
    assert!(style_css.contains(".boot-splash.hidden"));
    assert!(style_css.contains(".stage {"));
    assert!(style_css.contains(".stage-head {"));
    assert!(style_css.contains(".tab-panel {"));
    assert!(style_css.contains(".stage-section {"));
    assert!(style_css.contains("overflow-x: hidden;"));
    assert!(style_css.contains("min-width: min(9.8rem, 100%);"));
    assert!(style_css.contains(".form-row > *,"));
    assert!(style_css.contains("flex-wrap: wrap;"));
    assert!(style_css.contains("label input,"));
    assert!(style_css.contains(".form-row > label,"));
    assert!(style_css.contains(".setup-status-card,"));
    assert!(app_js.contains("function notifyHostBootReady(attempt)"));
    assert!(app_js.contains("function finishBoot()"));
    assert!(app_js.contains("function withTimeout(promise, ms)"));
    assert!(app_js.contains("function startInitialRefresh()"));
    assert!(app_js.contains("renderHomeEmptyState('Loading timeline...');"));
    assert!(app_js.contains("runHomeFetch().catch(function () {"));
    assert!(app_js.contains("els.splash.classList.add('hidden');"));
    assert!(app_js.contains("withTimeout(execArgv(['__wizardry_host_boot_ready']), 1200)"));
}

#[test]
fn rail_listboxes_stay_focusable_without_nested_selection_cards() {
    let index_html = read_file(&format!("{}/../app/index.html", env!("CARGO_MANIFEST_DIR")));
    let app_js = read_file(&format!("{}/../app/app.js", env!("CARGO_MANIFEST_DIR")));
    let style_css = read_file(&format!("{}/../app/style.css", env!("CARGO_MANIFEST_DIR")));

    assert!(index_html.contains("id=\"rail-nav-listbox\" class=\"rail-listbox rail-nav-listbox\" role=\"listbox\" tabindex=\"0\""));
    assert!(!index_html.contains("id=\"open-compose\""));
    assert!(!index_html.contains("id=\"primary-tabs\""));
    assert!(index_html.contains("id=\"following-listbox\" class=\"rail-listbox rail-content-listbox\" role=\"listbox\" tabindex=\"0\""));
    assert!(index_html.contains("id=\"library-listbox\" class=\"rail-listbox rail-content-listbox\" role=\"listbox\" tabindex=\"0\""));
    assert!(index_html.contains("id=\"relay-listbox\" class=\"rail-listbox settings-listbox\" role=\"listbox\" tabindex=\"0\""));
    assert!(index_html.contains("id=\"profile-picker-btn\""));
    assert!(index_html.contains("id=\"profile-picker-menu\""));
    assert!(!index_html.contains("id=\"library-selection-card\""));
    assert!(!index_html.contains("id=\"rail-open-settings\""));
    assert!(app_js.contains("bindListboxKeyboard(els.railNavListbox"));
    assert!(app_js.contains("bindListboxKeyboard(els.followingListbox"));
    assert!(app_js.contains("bindListboxKeyboard(els.relayListbox"));
    assert!(app_js.contains("bindListboxKeyboard(els.libraryListbox"));
    assert!(app_js.contains("railSelectionKind: 'nav'"));
    assert!(app_js.contains("function syncRailSelection()"));
    assert!(app_js.contains("function renderProfileMenuList()"));
    assert!(app_js.contains("function renderActiveProfileButton()"));
    assert!(app_js.contains("button[data-profile-action=\"create\"]"));
    assert!(app_js.contains("{ id: 'compose', label: 'Compose', icon: 'assets/compose-outline.svg' }"));
    assert!(app_js.contains("scrollIntoView({ block: 'start', inline: 'nearest', behavior: 'smooth' })"));
    assert!(app_js.contains("starBtn.className = 'feed-icon-btn';"));
    assert!(!app_js.contains("listBtn.textContent = 'Add To List';"));
    assert!(app_js.contains("function nearDivider(clientX)"));
    assert!(app_js.contains("workspace.addEventListener('pointerdown'"));
    assert!(app_js.contains("function runLibraryListView(listName)"));
    assert!(app_js.contains("safeBackend('library-list-folder-events', [name], 'Failed to load list')"));
    assert!(app_js.contains("setRailSelection('following', state.activeFollowingPubkey);"));
    assert!(app_js.contains("setRailSelection('list', state.selectedListName);"));
    assert!(style_css.contains("-webkit-mask-image: var(--icon-url);"));
    assert!(style_css.contains("mask-image: var(--icon-url);"));
    assert!(style_css.contains(".footer-profile-anchor"));
    assert!(style_css.contains("grid-template-columns: auto minmax(0, 1fr) minmax(0, 1fr);"));
    assert!(style_css.contains(".feed-icon-btn"));
    assert!(style_css.contains("overflow-wrap: anywhere;"));
    assert!(style_css.contains("word-break: break-word;"));
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
    assert!(backend_sh.contains("manual_follows=$(pref_get manual_follows"));
    assert!(backend_sh.contains("printf 'manual_follows=%s\\n'"));
    assert!(backend_sh.contains("ONSTR_LIST_ROOT=$HOME/.onstr"));
    assert!(backend_sh.contains("library-list-folders"));
    assert!(backend_sh.contains("library-create-folder"));
    assert!(backend_sh.contains("library-list-add-event"));
    assert!(backend_sh.contains("library-list-folder-events"));
}

#[test]
fn note_compose_does_not_inject_title_shorthand_tags() {
    let app_js = read_file(&format!("{}/../app/app.js", env!("CARGO_MANIFEST_DIR")));

    assert!(!app_js.contains("composeTagsWithName"));
    assert!(app_js.contains("return ['compose-note', [content, String(els.composeTags.value || '').trim(), draft]];"));
    assert!(app_js.contains("return ['compose-longform', [name, identifier, longformContent, String(els.composeSummary.value || '').trim(), draft]];"));
}
