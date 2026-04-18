use std::fs;
use std::process::Command;

fn read_file(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("failed to read {path}: {error}"))
}

fn run_node(script: &str) -> String {
    let output = Command::new("node")
        .arg("-e")
        .arg(script)
        .output()
        .unwrap_or_else(|error| panic!("failed to run node: {error}"));
    assert!(
        output.status.success(),
        "node script failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
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
    let app_js = read_file(&format!("{}/app/app.js", env!("CARGO_MANIFEST_DIR")));
    assert!(index_html.contains("class=\"boot-splash\""));
    assert!(index_html.contains("assets/forge-icon.png?v=stonr-control-20260417b"));
    assert!(!index_html.contains("assets/icons/meta/splash-mark.svg"));
    assert!(index_html.contains("style.css?v=stonr-control-20260416a"));
    assert!(index_html.contains("app.js?v=stonr-control-20260417b"));
    assert!(index_html.contains("window.__stonrBootFallbackTimer = setTimeout(function () {"));
    assert!(app_js.contains("function withTimeout(promise, ms)"));
    assert!(app_js.contains("if (window.__stonrBootFallbackTimer) {"));
    assert!(app_js.contains("state.bootWatchdogTimer = setTimeout(function () {"));
    assert!(app_js
        .contains("var prefs = await withTimeout(loadUiPrefs(), 1200).catch(function (error) {"));
    assert!(app_js.contains("loadAll().catch(function (error) {"));
    assert!(app_js.contains("withTimeout(execArgv(['__wizardry_host_boot_ready']), 1200)"));
    assert!(app_js.contains("await Promise.race(["));
}

#[test]
fn macos_icon_master_avoids_double_jail() {
    let app_root = format!("{}/app/assets/icons/meta", env!("CARGO_MANIFEST_DIR"));
    let plain = std::fs::read(format!("{}/plain-master.png", app_root)).unwrap();
    let apple = std::fs::read(format!("{}/apple-master.png", app_root)).unwrap();
    assert_ne!(apple, plain);
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

#[test]
fn app_support_helpers_remain_top_level_functions() {
    let app_js_path = format!("{}/app/app.js", env!("CARGO_MANIFEST_DIR"));
    let script_template = r#"
const fs = require('fs');
let src = fs.readFileSync('__APP_JS_PATH__', 'utf8');
src = src.replace(/\n\s*init\(\);\n/, '\n');
src = src.replace(
  /\}\)\(\);\s*$/,
  "window.__stonr_test_scope = {"
    + "syncFieldDependencies: typeof syncFieldDependencies,"
    + "appSupportLockByEnvKey: typeof appSupportLockByEnvKey,"
    + "appSupportLockedFieldValue: typeof appSupportLockedFieldValue,"
    + "appSupportLockReason: typeof appSupportLockReason"
    + "};})();"
);
function el() {
  return {
    disabled: false,
    hidden: false,
    value: '',
    textContent: '',
    innerHTML: '',
    dataset: {},
    style: { setProperty() {} },
    className: '',
    classList: { add() {}, remove() {}, toggle() {}, contains() { return false; } },
    appendChild() {},
    setAttribute() {},
    addEventListener() {},
    querySelector() { return null; },
    querySelectorAll() { return []; },
    getBoundingClientRect() { return { left: 0 }; },
    focus() {},
    releasePointerCapture() {},
    setPointerCapture() {}
  };
}
global.window = {
  location: { pathname: '/Users/test/stonr/app/index.html' },
  addEventListener() {},
  removeEventListener() {},
  wizardry: null
};
global.document = {
  querySelector() { return el(); },
  getElementById() { return el(); },
  createElement() { return el(); },
  createTextNode() { return {}; },
  createDocumentFragment() { return { appendChild() {} }; },
  addEventListener() {},
  body: { classList: { add() {}, remove() {}, contains() { return false; } } },
  documentElement: { style: { setProperty() {} } },
  visibilityState: 'visible'
};
global.requestAnimationFrame = function () { return 1; };
global.setTimeout = function () { return 1; };
global.clearTimeout = function () {};
global.setInterval = function () { return 1; };
global.clearInterval = function () {};
Function(src)();
console.log(JSON.stringify(window.__stonr_test_scope));
"#;
    let script = script_template.replace("__APP_JS_PATH__", &app_js_path);
    let output = run_node(&script);
    assert_eq!(
        output.trim(),
        r#"{"syncFieldDependencies":"function","appSupportLockByEnvKey":"function","appSupportLockedFieldValue":"function","appSupportLockReason":"function"}"#
    );
}

#[test]
fn relay_section_render_smoke_does_not_blank_main_pane() {
    let app_js_path = format!("{}/app/app.js", env!("CARGO_MANIFEST_DIR"));
    let script_template = r#"
const fs = require('fs');
let src = fs.readFileSync('__APP_JS_PATH__', 'utf8');
src = src.replace(/\n\s*init\(\);\n/, '\n');
src = src.replace(
  /\}\)\(\);\s*$/,
  "window.__stonr_render_test = { state: state, renderActiveSection: renderActiveSection, els: els };})();"
);
const elements = new Map();
function classList() {
  return { add() {}, remove() {}, toggle() {}, contains() { return false; } };
}
function makeElement(tagName, id) {
  const node = {
    id: id || '',
    tagName: String(tagName || 'div').toUpperCase(),
    children: [],
    disabled: false,
    hidden: false,
    value: '',
    checked: false,
    textContent: '',
    innerHTML: '',
    dataset: {},
    style: { setProperty() {} },
    className: '',
    classList: classList(),
    appendChild(child) { this.children.push(child); return child; },
    setAttribute() {},
    addEventListener() {},
    querySelector() { return null; },
    querySelectorAll() { return []; },
    getBoundingClientRect() { return { left: 0 }; },
    focus() {},
    releasePointerCapture() {},
    setPointerCapture() {}
  };
  return node;
}
function getElement(id) {
  if (!elements.has(id)) {
    elements.set(id, makeElement('div', id));
  }
  return elements.get(id);
}
global.window = {
  location: { pathname: '/Users/test/stonr/app/index.html' },
  addEventListener() {},
  removeEventListener() {},
  wizardry: null
};
global.document = {
  querySelector(selector) {
    if (selector === '.shell') return getElement('shell');
    if (selector === '.stage') return getElement('stage');
    if (selector === '.runtime-panel') return getElement('runtime-panel');
    return makeElement('div');
  },
  getElementById(id) { return getElement(id); },
  createElement(tagName) { return makeElement(tagName); },
  createTextNode(text) { return { textContent: text || '' }; },
  createDocumentFragment() { return { children: [], appendChild(child) { this.children.push(child); } }; },
  addEventListener() {},
  body: { classList: classList() },
  documentElement: { style: { setProperty() {} } },
  visibilityState: 'visible'
};
global.requestAnimationFrame = function () { return 1; };
global.setTimeout = function () { return 1; };
global.clearTimeout = function () {};
global.setInterval = function () { return 1; };
global.clearInterval = function () {};
Function(src)();
window.__stonr_render_test.state.bridge = false;
window.__stonr_render_test.state.activeSection = 'relay';
window.__stonr_render_test.renderActiveSection();
console.log(JSON.stringify({
  title: getElement('active-title').textContent,
  childCount: getElement('section-content').children.length
}));
"#;
    let script = script_template.replace("__APP_JS_PATH__", &app_js_path);
    let output = run_node(&script);
    assert_eq!(
        output.trim(),
        r#"{"title":"Relay Behavior","childCount":1}"#
    );
}
