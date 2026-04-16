(function () {
  var RECOMMENDED_RELAYS = Object.freeze([
    'wss://relay.damus.io',
    'wss://relay.primal.net',
    'wss://nos.lol',
    'wss://offchain.pub',
    'wss://relay.snort.social'
  ]);

  var state = {
    bridge: false,
    hostBootReadySent: false,
    activeTab: 'home',
    activeRailNav: 'home',
    selectedListName: 'inbox',
    activeFollowingPubkey: '',
    manualFollowingRows: [],
    selectedRelayUrl: '',
    activeProfileId: '',
    activeProfileName: '',
    activeProfilePubkey: '',
    selectedProfileId: '',
    recommendedRelaysNotice: '',
    relayReady: false,
    homeRelayUrl: '',
    railWidth: 352,
    relayRows: [],
    followingRows: [],
    libraryRows: [],
    homeEvents: [],
    discoverEvents: [],
    themes: [],
    theme: 'wizard',
    openMenu: null,
    refreshTimer: null,
    settingsReturnFocus: null,
    promptReturnFocus: null,
    promptResolver: null,
    railSelectionKind: 'nav',
    railSelectionValue: 'home',
    bootFinished: false,
    profiles: []
  };

  var TAB_IDS = ['home', 'discover', 'compose'];

  var COMMAND_ALLOWLIST = Object.freeze({
    'get-ui-prefs': true,
    'set-ui-pref': true,
    'list-themes': true,
    'profile-list': true,
    'profile-create': true,
    'profile-import': true,
    'profile-use': true,
    'timeline-fetch': true,
    'discover-search': true,
    'discover-count': true,
    'discover-relay-info': true,
    'compose-note': true,
    'compose-reply': true,
    'compose-longform': true,
    'compose-file-metadata': true,
    'compose-delete': true,
    'compose-list-drafts': true,
    'compose-preview': true,
    'compose-sign-draft': true,
    'publish-draft': true,
    'library-list': true,
    'library-star': true,
    'library-unstar': true,
    'library-save': true,
    'library-unsave': true,
    'library-list-folders': true,
    'library-create-folder': true,
    'library-list-add-event': true,
    'library-list-folder-events': true,
    'library-ingest-authored': true,
    'library-reindex': true,
    'relay-list': true,
    'relay-add': true,
    'relay-remove': true,
    'relay-set-home': true,
    'relay-probe': true,
    'media-upload-nip96': true,
    'stonr-print-config': true,
    'stonr-mirror-status': true,
    'stonr-retention-status': true,
    'doctor': true
  });

  var els = {
    body: document.body,
    app: document.getElementById('onstr-app'),
    splash: document.getElementById('splash'),
    toast: document.getElementById('toast'),

    railResizer: document.getElementById('rail-resizer'),
    railNavListbox: document.getElementById('rail-nav-listbox'),
    followingListbox: document.getElementById('following-listbox'),
    followingAdd: document.getElementById('following-add'),
    listCreate: document.getElementById('list-create'),
    recommendedRelaysNotice: document.getElementById('recommended-relays-notice'),
    recommendedRelaysDismiss: document.getElementById('recommended-relays-dismiss'),
    recommendedRelaysAdd: document.getElementById('recommended-relays-add'),
    recommendedRelaysOpenSettings: document.getElementById('recommended-relays-open-settings'),
    recommendedRelaysList: document.getElementById('recommended-relays-list'),
    settingsRecommendedRelaysList: document.getElementById('settings-recommended-relays-list'),
    settingsAddRecommendedRelays: document.getElementById('settings-add-recommended-relays'),

    themeLink: document.getElementById('theme-link'),
    themeSelect: document.getElementById('theme-select'),
    themePickerBtn: document.getElementById('theme-picker-btn'),
    themePickerMenu: document.getElementById('theme-picker-menu'),
    themePickerList: document.getElementById('theme-picker-list'),
    profilePickerBtn: document.getElementById('profile-picker-btn'),
    profilePickerMenu: document.getElementById('profile-picker-menu'),
    profilePickerList: document.getElementById('profile-picker-list'),

    settingsOpen: document.getElementById('open-settings'),
    settingsClose: document.getElementById('close-settings'),
    settingsBackdrop: document.getElementById('drawer-backdrop'),

    deleteClose: document.getElementById('delete-close'),
    deleteBackdrop: document.getElementById('delete-backdrop'),
    promptBackdrop: document.getElementById('prompt-backdrop'),
    promptDrawer: document.getElementById('prompt-drawer'),
    promptCancel: document.getElementById('prompt-cancel'),
    promptForm: document.getElementById('prompt-form'),
    promptTitle: document.getElementById('prompt-title'),
    promptLabel: document.getElementById('prompt-label'),
    promptLabelText: document.getElementById('prompt-label-text'),
    promptInput: document.getElementById('prompt-input'),
    promptSubmit: document.getElementById('prompt-submit'),

    homeForm: document.getElementById('home-form'),
    homeAuthors: document.getElementById('home-authors'),
    homeKinds: document.getElementById('home-kinds'),
    homeSearch: document.getElementById('home-search'),
    homeLimit: document.getElementById('home-limit'),
    homeIncludeRemotes: document.getElementById('home-include-remotes'),
    homeResultsSummary: document.getElementById('home-results-summary'),
    homeFeed: document.getElementById('home-feed'),
    homeLog: document.getElementById('home-log'),

    discoverForm: document.getElementById('discover-form'),
    discoverTerm: document.getElementById('discover-term'),
    discoverLimit: document.getElementById('discover-limit'),
    discoverCount: document.getElementById('discover-count'),
    discoverFilterSearch: document.getElementById('discover-filter-search'),
    discoverAuthors: document.getElementById('discover-authors'),
    discoverKinds: document.getElementById('discover-kinds'),
    discoverSince: document.getElementById('discover-since'),
    discoverUntil: document.getElementById('discover-until'),
    discoverResultsSummary: document.getElementById('discover-results-summary'),
    discoverFeed: document.getElementById('discover-feed'),
    relayInfoForm: document.getElementById('relay-info-form'),
    relayInfoUrl: document.getElementById('relay-info-url'),
    discoverLog: document.getElementById('discover-log'),

    peopleLoadFollowing: document.getElementById('people-load-following'),
    peopleLoadFollowers: document.getElementById('people-load-followers'),
    peoplePubkey: document.getElementById('people-pubkey'),
    peopleResults: document.getElementById('people-results'),

    composeType: document.getElementById('compose-type'),
    composeTypeGroup: document.getElementById('compose-type-group'),
    composeTypeHint: document.getElementById('compose-type-hint'),
    composeForm: document.getElementById('compose-form'),
    composeNameRow: document.getElementById('compose-name-row'),
    composeName: document.getElementById('compose-name'),
    composeNoteFields: document.getElementById('compose-note-fields'),
    composeReplyFields: document.getElementById('compose-reply-fields'),
    composeLongformFields: document.getElementById('compose-longform-fields'),
    composeFileFields: document.getElementById('compose-file-fields'),
    composeContent: document.getElementById('compose-content'),
    composeContentRow: document.getElementById('compose-content-row'),
    composeTags: document.getElementById('compose-tags'),
    composeReplyEvent: document.getElementById('compose-reply-event'),
    composeIdentifier: document.getElementById('compose-identifier'),
    composeSummary: document.getElementById('compose-summary'),
    composeUploadRelay: document.getElementById('compose-upload-relay'),
    composeUploadFile: document.getElementById('compose-upload-file'),
    composeUploadAction: document.getElementById('compose-upload-action'),
    composeFileUrl: document.getElementById('compose-file-url'),
    composeFileHash: document.getElementById('compose-file-hash'),
    composeFileMime: document.getElementById('compose-file-mime'),
    composeFileSize: document.getElementById('compose-file-size'),
    composeDraft: document.getElementById('compose-draft'),
    composePassword: document.getElementById('compose-password'),
    composeProfileId: document.getElementById('compose-profile-id'),
    composeRelays: document.getElementById('compose-relays'),
    composePreview: document.getElementById('compose-preview'),
    composeList: document.getElementById('compose-list'),
    composeSign: document.getElementById('compose-sign'),
    composePublish: document.getElementById('compose-publish'),
    composeOutput: document.getElementById('compose-output'),

    deleteForm: document.getElementById('delete-form'),
    deleteEventId: document.getElementById('delete-event-id'),
    deleteDraft: document.getElementById('delete-draft'),
    deleteReason: document.getElementById('delete-reason'),
    deletePassword: document.getElementById('delete-password'),
    deleteProfileId: document.getElementById('delete-profile-id'),
    deleteRelays: document.getElementById('delete-relays'),
    deleteLog: document.getElementById('delete-log'),

    libraryListbox: document.getElementById('library-listbox'),
    libraryReindex: document.getElementById('library-reindex'),
    libraryIngestForm: document.getElementById('library-ingest-form'),
    libraryAuthoredPath: document.getElementById('library-authored-path'),

    relayListbox: document.getElementById('relay-listbox'),
    relaySelectionTitle: document.getElementById('relay-selection-title'),
    relaySelectionMeta: document.getElementById('relay-selection-meta'),

    profileSummary: document.getElementById('profile-summary'),
    profileListbox: document.getElementById('profile-listbox'),
    profileCreateForm: document.getElementById('profile-create-form'),
    profileCreateName: document.getElementById('profile-create-name'),
    profileCreatePassword: document.getElementById('profile-create-password'),
    profileCreateSecret: document.getElementById('profile-create-secret'),
    profileCreateSetActive: document.getElementById('profile-create-set-active'),
    profileImportForm: document.getElementById('profile-import-form'),
    profileImportName: document.getElementById('profile-import-name'),
    profileImportPassword: document.getElementById('profile-import-password'),
    profileImportNcryptsec: document.getElementById('profile-import-ncryptsec'),
    profileUse: document.getElementById('profile-use'),
    profileLog: document.getElementById('profile-log'),

    networkRelayUrl: document.getElementById('network-relay-url'),
    networkRelayMode: document.getElementById('network-relay-mode'),
    networkRelayAdd: document.getElementById('network-relay-add'),
    networkRelayRemove: document.getElementById('network-relay-remove'),
    networkRelayHome: document.getElementById('network-relay-home'),
    networkRelayProbe: document.getElementById('network-relay-probe'),
    networkRelayList: document.getElementById('network-relay-list'),
    networkDoctor: document.getElementById('network-doctor'),
    networkStonrEnv: document.getElementById('network-stonr-env'),
    networkStonrConfig: document.getElementById('network-stonr-config'),
    networkStonrMirror: document.getElementById('network-stonr-mirror'),
    networkStonrRetention: document.getElementById('network-stonr-retention'),
    networkLog: document.getElementById('network-log'),

    doctorOutput: document.getElementById('doctor-output'),
    settingsRunDoctor: document.getElementById('settings-run-doctor')
  };

  function inferAppDir() {
    var path = decodeURIComponent(window.location.pathname || '');
    return path.replace(/\/index\.html$/, '');
  }

  function backendScript() {
    return inferAppDir() + '/scripts/onstr-backend.sh';
  }

  function nowStamp() {
    return new Date().toLocaleTimeString();
  }

  function toast(message, kind) {
    els.toast.textContent = message;
    els.toast.className = 'toast show ' + (kind || '');
    clearTimeout(toast.timer);
    toast.timer = setTimeout(function () {
      els.toast.className = 'toast';
    }, 2400);
  }

  function bridgeAvailable() {
    return !!(window.wizardry && window.wizardry.exec);
  }

  async function execArgv(argv) {
    if (!state.bridge) {
      throw new Error('Wizardry desktop bridge is unavailable.');
    }
    var result = await window.wizardry.exec(argv);
    if (typeof result.exit_code !== 'undefined' && result.exit_code !== 0) {
      throw new Error(String(result.stderr || result.stdout || 'command failed').trim());
    }
    return result;
  }

  async function backend(command, args) {
    if (!COMMAND_ALLOWLIST[command]) {
      throw new Error('Blocked backend command: ' + command);
    }
    var argv = ['sh', backendScript(), command].concat(args || []);
    var result = await execArgv(argv);
    return String(result.stdout || '');
  }

  async function safeBackend(command, args, onError) {
    try {
      return await backend(command, args);
    } catch (error) {
      var message = String((error && error.message) || onError || 'command failed');
      toast(message, 'bad');
      throw error;
    }
  }

  function parseKv(blob) {
    var out = {};
    String(blob || '').split('\n').forEach(function (line) {
      var idx = line.indexOf('=');
      if (idx <= 0) {
        return;
      }
      out[line.slice(0, idx)] = line.slice(idx + 1);
    });
    return out;
  }

  function parseMaybeJson(text) {
    var source = String(text || '').trim();
    if (!source) {
      return null;
    }
    try {
      return JSON.parse(source);
    } catch (_error) {
      return null;
    }
  }

  function writeLog(node, label, payload) {
    var parsed = parseMaybeJson(payload);
    if (parsed !== null) {
      node.textContent = '[' + nowStamp() + '] ' + label + '\n' + JSON.stringify(parsed, null, 2);
      return parsed;
    }
    node.textContent = '[' + nowStamp() + '] ' + label + '\n' + String(payload || '').trim();
    return null;
  }

  function shortId(value) {
    var text = String(value || '');
    if (text.length <= 16) {
      return text;
    }
    return text.slice(0, 8) + '…' + text.slice(-6);
  }

  function optionDomId(prefix, value) {
    return (
      prefix +
      '-' +
      String(value || '')
        .toLowerCase()
        .replace(/[^a-z0-9_-]+/g, '-')
        .replace(/^-+|-+$/g, '')
        .slice(0, 64)
    );
  }

  function setListboxActiveDescendant(listbox, optionId) {
    if (!listbox) {
      return;
    }
    if (optionId) {
      listbox.setAttribute('aria-activedescendant', optionId);
      return;
    }
    listbox.removeAttribute('aria-activedescendant');
  }

  function listboxOptions(listbox) {
    return Array.prototype.slice.call(listbox.querySelectorAll('.rail-list-option[role="option"]'));
  }

  function renderRailNavigation() {
    if (!els.railNavListbox) {
      return;
    }
    var items = [
      { id: 'compose', label: 'Compose', icon: 'assets/compose-outline.svg' },
      { id: 'home', label: 'Home', icon: 'assets/home-outline.svg' },
      { id: 'feed', label: 'Feed', icon: 'assets/feed-outline.svg' },
      { id: 'discover', label: 'Discover', icon: 'assets/discover-outline.svg' }
    ];
    els.railNavListbox.innerHTML = '';
    items.forEach(function (item) {
      var row = document.createElement('button');
      row.type = 'button';
      row.className = 'rail-list-option';
      row.setAttribute('role', 'option');
      row.setAttribute('data-rail-nav', item.id);
      row.id = optionDomId('rail-nav-option', item.id);
      row.tabIndex = -1;

      var copy = document.createElement('span');
      copy.className = 'rail-option-copy';

      var icon = document.createElement('span');
      icon.className = 'rail-nav-icon';
      icon.setAttribute('aria-hidden', 'true');
      icon.style.setProperty('--icon-url', 'url("' + item.icon + '")');

      var label = document.createElement('span');
      label.className = 'rail-option-label';
      label.textContent = item.label;

      copy.appendChild(icon);
      copy.appendChild(label);
      row.appendChild(copy);
      row.addEventListener('click', function () {
        setActiveRailNav(item.id, true);
      });
      els.railNavListbox.appendChild(row);
    });
    setActiveRailNav(state.activeRailNav || 'home', false);
  }

  function setRailSelection(kind, value) {
    state.railSelectionKind = String(kind || '').trim();
    state.railSelectionValue = String(value || '').trim();
    syncRailSelection();
  }

  function clearListboxSelection(listbox) {
    if (!listbox) {
      return;
    }
    listboxOptions(listbox).forEach(function (node) {
      node.classList.remove('is-active');
      node.setAttribute('aria-selected', 'false');
    });
    setListboxActiveDescendant(listbox, '');
  }

  function syncListboxSelection(listbox, attrName, expectedValue, active) {
    if (!listbox) {
      return;
    }
    if (!active) {
      clearListboxSelection(listbox);
      return;
    }
    var activeOptionId = '';
    listboxOptions(listbox).forEach(function (node) {
      var selected = node.getAttribute(attrName) === expectedValue;
      node.classList.toggle('is-active', selected);
      node.setAttribute('aria-selected', selected ? 'true' : 'false');
      if (selected) {
        activeOptionId = node.id;
      }
    });
    setListboxActiveDescendant(listbox, activeOptionId);
  }

  function syncRailSelection() {
    syncListboxSelection(
      els.railNavListbox,
      'data-rail-nav',
      state.railSelectionKind === 'nav' ? state.railSelectionValue : '',
      state.railSelectionKind === 'nav'
    );
    syncListboxSelection(
      els.followingListbox,
      'data-following-pubkey',
      state.railSelectionKind === 'following' ? state.railSelectionValue : '',
      state.railSelectionKind === 'following'
    );
    syncListboxSelection(
      els.libraryListbox,
      'data-list-name',
      state.railSelectionKind === 'list' ? state.railSelectionValue : '',
      state.railSelectionKind === 'list'
    );
  }

  function setActiveRailNav(viewId, userInitiated) {
    var view = String(viewId || '').trim();
    if (['home', 'feed', 'discover', 'compose'].indexOf(view) < 0) {
      view = 'home';
    }
    state.activeRailNav = view;
    if (userInitiated || state.railSelectionKind === 'nav' || !state.railSelectionKind) {
      setRailSelection('nav', view);
    } else {
      syncRailSelection();
    }
    if (!userInitiated) {
      return;
    }
    if (view === 'discover' || view === 'compose') {
      setActiveTab(view, false);
      return;
    }
    setActiveTab('home', false);
    if (view === 'feed' && els.homeFeed && typeof els.homeFeed.scrollIntoView === 'function') {
      els.homeFeed.scrollIntoView({ block: 'start', inline: 'nearest', behavior: 'smooth' });
    }
  }

  function relayDisplayLabel(url) {
    return String(url || '').replace(/^wss?:\/\//, '') || 'Relay';
  }

  function relayRoleLabel(meta) {
    var parts = [];
    if (meta && meta.home) {
      parts.push('Home');
    }
    if (meta && meta.read) {
      parts.push('Read');
    }
    if (meta && meta.write) {
      parts.push('Write');
    }
    return parts.join(' · ') || 'Configured relay';
  }

  function normalizePubkey(value) {
    var candidate = String(value || '').trim().toLowerCase();
    if (!/^[0-9a-f]{64}$/.test(candidate)) {
      return '';
    }
    return candidate;
  }

  function parsePubkeyCsv(value) {
    if (!value) {
      return [];
    }
    var seen = {};
    return String(value)
      .split(',')
      .map(function (entry) {
        return normalizePubkey(entry);
      })
      .filter(Boolean)
      .filter(function (entry) {
        if (seen[entry]) {
          return false;
        }
        seen[entry] = true;
        return true;
      });
  }

  function mergeFollowingRows(remoteRows, manualRows) {
    var out = [];
    var seen = {};
    (Array.isArray(remoteRows) ? remoteRows : []).concat(Array.isArray(manualRows) ? manualRows : []).forEach(function (pubkey) {
      var value = normalizePubkey(pubkey);
      if (!value || seen[value]) {
        return;
      }
      seen[value] = true;
      out.push(value);
    });
    return out;
  }

  function formatTimestamp(unix) {
    var value = Number(unix || 0);
    if (!value) {
      return 'Unknown time';
    }
    try {
      return new Date(value * 1000).toLocaleString([], {
        dateStyle: 'medium',
        timeStyle: 'short'
      });
    } catch (_error) {
      return String(unix || '');
    }
  }

  function eventKindLabel(kind) {
    var value = Number(kind || 0);
    var labels = {
      1: 'Note',
      3: 'Contacts',
      6: 'Repost',
      7: 'Reaction',
      1063: 'Attachment',
      30023: 'Long-form'
    };
    return labels[value] || ('Kind ' + String(kind || '?'));
  }

  function feedEmpty(node, message) {
    node.innerHTML = '';
    var empty = document.createElement('div');
    empty.className = 'feed-empty';
    empty.textContent = message;
    node.appendChild(empty);
  }

  function makeRailIcon(assetPath) {
    var icon = document.createElement('span');
    icon.className = 'rail-nav-icon';
    icon.setAttribute('aria-hidden', 'true');
    icon.style.setProperty('--icon-url', 'url("' + assetPath + '")');
    return icon;
  }

  function relayConfigured(relays) {
    if (!relays || typeof relays !== 'object') {
      return false;
    }
    return !!(
      String(relays.home || '').trim() ||
      (Array.isArray(relays.read) && relays.read.some(function (relay) { return String(relay || '').trim(); })) ||
      (Array.isArray(relays.write) && relays.write.some(function (relay) { return String(relay || '').trim(); }))
    );
  }

  function activeProfileLabel() {
    if (!state.activeProfileId) {
      return 'No active profile yet.';
    }
    var name = String(state.activeProfileName || '').trim();
    var pubkey = String(state.activeProfilePubkey || '').trim();
    if (name && pubkey) {
      return name + ' · ' + shortId(pubkey);
    }
    return name || pubkey || state.activeProfileId;
  }

  function activeProfileFooterLabel() {
    if (!state.activeProfileId) {
      return 'Create identity...';
    }
    return String(state.activeProfileName || '').trim() || shortId(String(state.activeProfilePubkey || state.activeProfileId || ''));
  }

  function renderRecommendedRelayLists() {
    [els.recommendedRelaysList, els.settingsRecommendedRelaysList].forEach(function (node) {
      if (!node) {
        return;
      }
      node.innerHTML = '';
      RECOMMENDED_RELAYS.forEach(function (relay) {
        var item = document.createElement('li');
        item.textContent = relayDisplayLabel(relay);
        item.title = relay;
        node.appendChild(item);
      });
    });
  }

  async function saveRecommendedRelaysNotice(value) {
    state.recommendedRelaysNotice = String(value || '').trim();
    if (!state.bridge) {
      renderRecommendedRelaysNotice();
      return;
    }
    await saveUiPref('recommended_relays_notice', state.recommendedRelaysNotice);
    renderRecommendedRelaysNotice();
  }

  function renderRecommendedRelaysNotice() {
    if (!els.recommendedRelaysNotice) {
      return;
    }
    var shouldShow = state.bridge && !state.relayReady && state.recommendedRelaysNotice !== 'dismissed' && state.recommendedRelaysNotice !== 'accepted';
    els.recommendedRelaysNotice.classList.toggle('hidden', !shouldShow);
  }

  async function addRecommendedRelays() {
    var relaysBlob = await safeBackend('relay-list', [], 'Failed to inspect current relays');
    var parsed = parseMaybeJson(relaysBlob);
    var relays = parsed && parsed.relays ? parsed.relays : { home: '', read: [], write: [] };
    var existing = {};

    [relays.home].concat(relays.read || []).concat(relays.write || []).forEach(function (relay) {
      var url = String(relay || '').trim();
      if (url) {
        existing[url] = true;
      }
    });

    for (var i = 0; i < RECOMMENDED_RELAYS.length; i += 1) {
      var relay = RECOMMENDED_RELAYS[i];
      if (existing[relay]) {
        continue;
      }
      await safeBackend('relay-add', [relay, 'both'], 'Failed to add recommended relay');
      existing[relay] = true;
    }

    var homeRelay = String(relays.home || '').trim();
    if (!homeRelay) {
      await safeBackend('relay-set-home', [RECOMMENDED_RELAYS[0]], 'Failed to set recommended home relay');
    }

    await saveRecommendedRelaysNotice('accepted');
    await runRelayList().catch(function () {
      return;
    });
    if (state.activeTab === 'home') {
      await runHomeFetch().catch(function () {
        return;
      });
    }
    toast('Recommended relays added.', 'good');
  }

  function renderSetupPanel() {
    renderRecommendedRelaysNotice();
  }

  function renderHomeEmptyState(message) {
    state.homeEvents = [];
    if (els.homeResultsSummary) {
      els.homeResultsSummary.textContent = message;
    }
    feedEmpty(els.homeFeed, message);
  }

  function withTimeout(promise, ms) {
    return new Promise(function (resolve, reject) {
      var settled = false;
      var timer = setTimeout(function () {
        if (settled) {
          return;
        }
        settled = true;
        reject(new Error('operation timed out'));
      }, ms);

      Promise.resolve(promise).then(function (value) {
        if (settled) {
          return;
        }
        settled = true;
        clearTimeout(timer);
        resolve(value);
      }).catch(function (error) {
        if (settled) {
          return;
        }
        settled = true;
        clearTimeout(timer);
        reject(error);
      });
    });
  }

  function revealUi() {
    if (state.bootFinished) {
      return;
    }
    state.bootFinished = true;
    if (els.app) {
      els.app.classList.remove('hidden');
      els.app.setAttribute('aria-hidden', 'false');
    }
    if (els.splash) {
      els.splash.classList.add('hidden');
      els.splash.hidden = true;
      els.splash.setAttribute('aria-hidden', 'true');
    }
    if (els.body) {
      els.body.classList.remove('onstr-booting');
    }
  }

  function notifyHostBootReady(attempt) {
    var tries = typeof attempt === 'number' ? attempt : 0;
    if (state.hostBootReadySent) {
      revealUi();
      return;
    }
    if (!bridgeAvailable()) {
      revealUi();
      return;
    }
    state.hostBootReadySent = true;
    requestAnimationFrame(function () {
      requestAnimationFrame(function () {
        withTimeout(execArgv(['__wizardry_host_boot_ready']), 1200).then(function () {
          revealUi();
        }).catch(function () {
          state.hostBootReadySent = false;
          if (tries >= 160) {
            revealUi();
            return;
          }
          setTimeout(function () {
            notifyHostBootReady(tries + 1);
          }, 50);
        });
      });
    });
  }

  function finishBoot() {
    requestAnimationFrame(function () {
      requestAnimationFrame(function () {
        notifyHostBootReady(0);
      });
    });
  }

  function startInitialRefresh() {
    if (!state.bridge) {
      return;
    }
    runFollowingList().catch(function () {
      return;
    });
    if (state.relayReady) {
      runHomeFetch().catch(function () {
        return;
      });
    }
  }

  function closeOpenMenu() {
    if (!state.openMenu) {
      return;
    }
    state.openMenu.classList.add('hidden');
    state.openMenu = null;
  }

  function bindListboxKeyboard(listbox, attrName, onSelect, onActivate) {
    if (!listbox) {
      return;
    }
    listbox.addEventListener('keydown', function (event) {
      var key = event.key;
      if (['ArrowDown', 'ArrowUp', 'Home', 'End', 'Enter', ' '].indexOf(key) < 0) {
        return;
      }
      var options = listboxOptions(listbox);
      if (!options.length) {
        return;
      }
      var current = options.findIndex(function (node) {
        return node.classList.contains('is-active');
      });
      if (current < 0) {
        current = 0;
      }

      if (key === 'Enter' || key === ' ') {
        event.preventDefault();
        var selected = options[current];
        var selectedValue = String(selected.getAttribute(attrName) || '').trim();
        if (!selectedValue) {
          return;
        }
        if (onActivate) {
          onActivate(selectedValue);
          return;
        }
        onSelect(selectedValue);
        return;
      }

      event.preventDefault();
      var next = current;
      if (key === 'Home') {
        next = 0;
      } else if (key === 'End') {
        next = options.length - 1;
      } else {
        next = current + (key === 'ArrowDown' ? 1 : -1);
      }
      if (next < 0) {
        next = 0;
      }
      if (next >= options.length) {
        next = options.length - 1;
      }
      var value = String(options[next].getAttribute(attrName) || '').trim();
      if (!value) {
        return;
      }
      onSelect(value);
    });
  }

  function setActiveTab(tabId, focusTab) {
    if (TAB_IDS.indexOf(tabId) < 0) {
      return;
    }
    state.activeTab = tabId;

    TAB_IDS.forEach(function (id) {
      var panel = document.getElementById('tab-' + id);
      var active = id === tabId;
      panel.classList.toggle('hidden', !active);
      if (active && focusTab && typeof panel.focus === 'function') {
        panel.focus();
      }
    });
    if (tabId === 'discover' || tabId === 'compose') {
      setActiveRailNav(tabId, false);
    } else if (state.activeRailNav === 'discover' || state.activeRailNav === 'compose') {
      setActiveRailNav('home', false);
    }

    saveUiPref('active_tab', tabId).catch(function () {
      return;
    });
  }

  function bindTabSemantics() {
    return;
  }

  function openSettings(trigger) {
    closeThemeMenu(false);
    closeProfileMenu(false);
    state.settingsReturnFocus = trigger || els.settingsOpen;
    els.settingsBackdrop.classList.remove('hidden');
    els.settingsBackdrop.setAttribute('aria-hidden', 'false');
    els.settingsOpen.classList.add('active');
    if (!state.activeProfileId && els.profileCreateName) {
      els.profileCreateName.focus();
    } else {
      els.themeSelect.focus();
    }
  }

  function closeSettings() {
    els.settingsBackdrop.classList.add('hidden');
    els.settingsBackdrop.setAttribute('aria-hidden', 'true');
    els.settingsOpen.classList.remove('active');
    if (state.settingsReturnFocus && typeof state.settingsReturnFocus.focus === 'function') {
      state.settingsReturnFocus.focus();
      return;
    }
    els.settingsOpen.focus();
  }

  function openCompose(focusNode) {
    setActiveTab('compose', false);
    setRailSelection('nav', 'compose');
    if (focusNode && typeof focusNode.focus === 'function') {
      focusNode.focus();
      return;
    }
    els.composeDraft.focus();
  }

  function openDelete(eventId) {
    var draft = 'delete-' + String(eventId || '').slice(0, 12);
    els.deleteEventId.value = eventId;
    els.deleteDraft.value = draft;
    els.deleteReason.value = '';
    els.deleteBackdrop.classList.remove('hidden');
    els.deleteBackdrop.setAttribute('aria-hidden', 'false');
    els.deletePassword.focus();
  }

  function closeDelete() {
    els.deleteBackdrop.classList.add('hidden');
    els.deleteBackdrop.setAttribute('aria-hidden', 'true');
  }

  function closePrompt(value) {
    if (els.promptBackdrop) {
      els.promptBackdrop.classList.add('hidden');
      els.promptBackdrop.setAttribute('aria-hidden', 'true');
    }
    var resolver = state.promptResolver;
    var returnFocus = state.promptReturnFocus;
    state.promptResolver = null;
    state.promptReturnFocus = null;
    if (returnFocus && typeof returnFocus.focus === 'function') {
      returnFocus.focus();
    }
    if (resolver) {
      resolver(value);
    }
  }

  function openTextPrompt(options) {
    options = options || {};
    if (!els.promptBackdrop || !els.promptInput) {
      return Promise.resolve(null);
    }
    if (state.promptResolver) {
      closePrompt(null);
    }
    state.promptReturnFocus = document.activeElement;
    if (els.promptTitle) {
      els.promptTitle.textContent = String(options.title || 'Input');
    }
    if (els.promptLabelText) {
      els.promptLabelText.textContent = String(options.label || 'Value');
    }
    if (els.promptDrawer) {
      els.promptDrawer.classList.toggle('prompt-drawer-wide', !!options.wide);
    }
    if (els.promptLabel) {
      els.promptLabel.classList.toggle('wide-label', !!options.wide);
    }
    els.promptInput.value = String(options.value || '');
    els.promptInput.placeholder = String(options.placeholder || '');
    els.promptBackdrop.classList.remove('hidden');
    els.promptBackdrop.setAttribute('aria-hidden', 'false');
    requestAnimationFrame(function () {
      els.promptInput.focus();
      els.promptInput.select();
    });
    return new Promise(function (resolve) {
      state.promptResolver = resolve;
    });
  }

  function listThemeNamesFromBlob(blob) {
    return String(blob || '')
      .split('\n')
      .map(function (line) {
        return line.trim();
      })
      .filter(function (line) {
        return !!line;
      });
  }

  function setThemeOptions(options) {
    var names = options.slice();
    if (!names.length) {
      names = ['wizard'];
    }
    state.themes = names;
    if (els.themeSelect) {
      els.themeSelect.innerHTML = '';
      names.forEach(function (name) {
        var opt = document.createElement('option');
        opt.value = name;
        opt.textContent = name;
        els.themeSelect.appendChild(opt);
      });
    }
    renderThemeMenuList();
  }

  function themeLabel(name) {
    return String(name || '')
      .split(/[-_]+/)
      .filter(Boolean)
      .map(function (part) {
        return part.charAt(0).toUpperCase() + part.slice(1);
      })
      .join(' ') || 'Wizard';
  }

  function renderThemeMenuList() {
    if (!els.themePickerList) {
      return;
    }
    els.themePickerList.innerHTML = '';
    state.themes.forEach(function (name) {
      var button = document.createElement('button');
      button.type = 'button';
      button.className = 'footer-theme-item' + (name === state.theme ? ' active' : '');
      button.setAttribute('data-theme-name', name);
      button.setAttribute('aria-pressed', name === state.theme ? 'true' : 'false');

      var label = document.createElement('span');
      label.textContent = themeLabel(name);
      button.appendChild(label);

      var check = document.createElement('span');
      check.className = 'footer-theme-check';
      check.setAttribute('aria-hidden', 'true');
      check.textContent = '✓';
      button.appendChild(check);

      els.themePickerList.appendChild(button);
    });
  }

  function themeMenuOpen() {
    return !!(els.themePickerMenu && !els.themePickerMenu.classList.contains('hidden'));
  }

  function profileMenuOpen() {
    return !!(els.profilePickerMenu && !els.profilePickerMenu.classList.contains('hidden'));
  }

  function openThemeMenu() {
    if (!els.themePickerMenu || !els.themePickerBtn) {
      return;
    }
    closeProfileMenu(false);
    closeOpenMenu();
    renderThemeMenuList();
    els.themePickerMenu.classList.remove('hidden');
    els.themePickerBtn.classList.add('active');
    els.themePickerBtn.setAttribute('aria-expanded', 'true');
  }

  function closeThemeMenu(restoreFocus) {
    if (!els.themePickerMenu || !els.themePickerBtn) {
      return;
    }
    els.themePickerMenu.classList.add('hidden');
    els.themePickerBtn.classList.remove('active');
    els.themePickerBtn.setAttribute('aria-expanded', 'false');
    if (restoreFocus && typeof els.themePickerBtn.focus === 'function') {
      els.themePickerBtn.focus();
    }
  }

  function renderProfileMenuList() {
    if (!els.profilePickerList) {
      return;
    }
    els.profilePickerList.innerHTML = '';

    state.profiles.forEach(function (profile) {
      var button = document.createElement('button');
      button.type = 'button';
      button.className = 'footer-theme-item' + (profile.id === state.activeProfileId ? ' active' : '');
      button.setAttribute('data-profile-id', String(profile.id || ''));
      button.setAttribute('aria-pressed', profile.id === state.activeProfileId ? 'true' : 'false');

      var name = document.createElement('span');
      name.className = 'footer-profile-name';
      name.textContent = String(profile.name || 'Unnamed profile');
      button.appendChild(name);

      var meta = document.createElement('span');
      meta.className = 'footer-profile-meta';
      meta.textContent = profile.id === state.activeProfileId ? 'Active' : shortId(String(profile.pubkey || profile.id || ''));
      button.appendChild(meta);

      els.profilePickerList.appendChild(button);
    });

    var createButton = document.createElement('button');
    createButton.type = 'button';
    createButton.className = 'footer-theme-item footer-profile-new';
    createButton.setAttribute('data-profile-action', 'create');
    createButton.textContent = 'New Nostr Profile';
    els.profilePickerList.appendChild(createButton);
  }

  function openProfileMenu() {
    if (!els.profilePickerMenu || !els.profilePickerBtn) {
      return;
    }
    closeThemeMenu(false);
    closeOpenMenu();
    renderProfileMenuList();
    els.profilePickerMenu.classList.remove('hidden');
    els.profilePickerBtn.classList.add('active');
    els.profilePickerBtn.setAttribute('aria-expanded', 'true');
  }

  function closeProfileMenu(restoreFocus) {
    if (!els.profilePickerMenu || !els.profilePickerBtn) {
      return;
    }
    els.profilePickerMenu.classList.add('hidden');
    els.profilePickerBtn.classList.remove('active');
    els.profilePickerBtn.setAttribute('aria-expanded', 'false');
    if (restoreFocus && typeof els.profilePickerBtn.focus === 'function') {
      els.profilePickerBtn.focus();
    }
  }

  function toggleProfileMenu() {
    if (profileMenuOpen()) {
      closeProfileMenu(true);
      return;
    }
    openProfileMenu();
  }

  function toggleThemeMenu() {
    if (themeMenuOpen()) {
      closeThemeMenu(true);
      return;
    }
    openThemeMenu();
  }

  function maybeCloseThemeMenuFromPointer(event) {
    if (!themeMenuOpen() && !profileMenuOpen()) {
      return;
    }
    if (
      event.target &&
      event.target.closest &&
      (
        event.target.closest('#theme-picker-menu') ||
        event.target.closest('#theme-picker-btn') ||
        event.target.closest('#profile-picker-menu') ||
        event.target.closest('#profile-picker-btn')
      )
    ) {
      return;
    }
    closeThemeMenu(false);
    closeProfileMenu(false);
  }

  function applyTheme(name) {
    if (!name) {
      return;
    }
    state.theme = name;
    els.themeLink.href = 'themes/' + name + '.css';
    if (els.themeSelect) {
      els.themeSelect.value = name;
    }
    if (els.themePickerBtn) {
      els.themePickerBtn.textContent = themeLabel(name);
    }
    renderThemeMenuList();
  }

  function renderActiveProfileButton() {
    if (!els.profilePickerBtn) {
      return;
    }
    els.profilePickerBtn.textContent = activeProfileFooterLabel();
  }

  async function saveUiPref(key, value) {
    if (!state.bridge) {
      return;
    }
    await backend('set-ui-pref', [key, String(value || '')]);
  }

  function normalizeRailWidth(value) {
    var width = Number(value);
    if (!Number.isFinite(width) || width <= 0) {
      width = 352;
    }
    return Math.round(Math.min(560, Math.max(272, width)));
  }

  function applyRailWidth(width) {
    var next = normalizeRailWidth(width);
    state.railWidth = next;
    if (document.documentElement) {
      document.documentElement.style.setProperty('--rail-width', next + 'px');
    }
    return next;
  }

  function persistRailWidth(width) {
    applyRailWidth(width);
    saveUiPref('rail_width', String(state.railWidth)).catch(function () {
      return;
    });
  }

  function bindThemeControls() {
    function onThemeSelect(next) {
      if (!next) {
        return;
      }
      applyTheme(next);
      saveUiPref('theme', next).catch(function () {
        return;
      });
    }

    if (els.themeSelect) {
      els.themeSelect.addEventListener('change', function () {
        onThemeSelect(String(els.themeSelect.value || '').trim());
      });
      els.themeSelect.addEventListener('input', function () {
        onThemeSelect(String(els.themeSelect.value || '').trim());
      });
      els.themeSelect.addEventListener('keydown', function (event) {
        if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') {
          return;
        }
        requestAnimationFrame(function () {
          onThemeSelect(String(els.themeSelect.value || '').trim());
        });
      });
    }
    if (els.themePickerBtn) {
      els.themePickerBtn.addEventListener('click', function (event) {
        event.preventDefault();
        toggleThemeMenu();
      });
    }
    if (els.themePickerList) {
      els.themePickerList.addEventListener('click', function (event) {
        var item = event.target.closest('button[data-theme-name]');
        if (!item) {
          return;
        }
        onThemeSelect(String(item.getAttribute('data-theme-name') || '').trim());
        closeThemeMenu(true);
      });
    }
    if (els.profilePickerBtn) {
      els.profilePickerBtn.addEventListener('click', function (event) {
        event.preventDefault();
        toggleProfileMenu();
      });
    }
    if (els.profilePickerList) {
      els.profilePickerList.addEventListener('click', function (event) {
        var createItem = event.target.closest('button[data-profile-action="create"]');
        if (createItem) {
          closeProfileMenu(false);
          openSettings(els.profilePickerBtn);
          requestAnimationFrame(function () {
            if (els.profileCreateName) {
              els.profileCreateName.focus();
            }
          });
          return;
        }
        var item = event.target.closest('button[data-profile-id]');
        if (!item) {
          return;
        }
        setSelectedProfile(String(item.getAttribute('data-profile-id') || '').trim());
        runProfileUse().catch(function () {
          return;
        });
        closeProfileMenu(false);
      });
    }
  }

  function bindRailResizer() {
    if (!els.railResizer) {
      return;
    }

    var workspace = document.querySelector('.workspace');
    var dragging = false;
    var dragPointerId = null;

    function setDragCursor(active) {
      document.body.style.cursor = active ? 'col-resize' : '';
      document.body.style.userSelect = active ? 'none' : '';
      document.documentElement.style.cursor = active ? 'col-resize' : '';
      document.documentElement.style.userSelect = active ? 'none' : '';
    }

    function workspaceRect() {
      return workspace ? workspace.getBoundingClientRect() : null;
    }

    function dividerX() {
      var rect = workspaceRect();
      if (!rect) {
        return 0;
      }
      return rect.left + state.railWidth;
    }

    function nearDivider(clientX) {
      return Math.abs(Number(clientX || 0) - dividerX()) <= 8;
    }

    function updateWidth(clientX) {
      var rect = workspaceRect();
      if (!rect) {
        return;
      }
      var minWidth = 272;
      var maxWidth = Math.max(minWidth, Math.floor(rect.width - 320));
      var next = Math.round(clientX - rect.left);
      next = Math.min(maxWidth, Math.max(minWidth, next));
      applyRailWidth(next);
    }

    function startDrag(event) {
      if (event.button !== 0) {
        return false;
      }
      dragging = true;
      dragPointerId = event.pointerId;
      event.preventDefault();
      if (els.railResizer.setPointerCapture) {
        els.railResizer.setPointerCapture(event.pointerId);
      }
      setDragCursor(true);
      updateWidth(event.clientX);
      return true;
    }

    function endDrag(event) {
      if (!dragging || (event.pointerId && event.pointerId !== dragPointerId)) {
        return;
      }
      dragging = false;
      dragPointerId = null;
      setDragCursor(false);
      persistRailWidth(state.railWidth);
    }

    if (workspace) {
      workspace.addEventListener('pointermove', function (event) {
        if (dragging && event.pointerId === dragPointerId) {
          event.preventDefault();
          updateWidth(event.clientX);
          return;
        }
        if (nearDivider(event.clientX)) {
          workspace.style.cursor = 'col-resize';
          return;
        }
        workspace.style.cursor = '';
      });

      workspace.addEventListener('pointerleave', function () {
        if (!dragging) {
          workspace.style.cursor = '';
        }
      });

      workspace.addEventListener('pointerdown', function (event) {
        if (!nearDivider(event.clientX)) {
          return;
        }
        startDrag(event);
      });
    }

    els.railResizer.addEventListener('pointerdown', function (event) {
      startDrag(event);
    });

    els.railResizer.addEventListener('pointermove', function (event) {
      if (!dragging || event.pointerId !== dragPointerId) {
        return;
      }
      event.preventDefault();
      updateWidth(event.clientX);
    });

    els.railResizer.addEventListener('pointerup', endDrag);
    els.railResizer.addEventListener('pointercancel', endDrag);
    els.railResizer.addEventListener('lostpointercapture', function () {
      if (!dragging) {
        return;
      }
      dragging = false;
      dragPointerId = null;
      setDragCursor(false);
      if (workspace) {
        workspace.style.cursor = '';
      }
      persistRailWidth(state.railWidth);
    });
  }

  function setComposeTypeUi() {
    var type = String(els.composeType.value || 'note');
    var hints = {
      note: 'Write a standard note, save a draft, then sign and publish when you are ready.',
      reply: 'Reply keeps the target event id attached so you can answer a specific post cleanly.',
      longform: 'Long-form drafts need a title, identifier, and body before you save them.',
      'file-metadata': 'Upload a file or fill in the NIP-94 metadata fields, then publish the attachment event.'
    };
    if (els.composeNameRow) {
      els.composeNameRow.classList.toggle('hidden', type !== 'longform');
    }
    els.composeNoteFields.classList.toggle('hidden', type !== 'note');
    els.composeReplyFields.classList.toggle('hidden', type !== 'reply');
    els.composeLongformFields.classList.toggle('hidden', type !== 'longform');
    els.composeFileFields.classList.toggle('hidden', type !== 'file-metadata');
    els.composeContentRow.classList.toggle('hidden', type === 'file-metadata');
    if (els.composeTypeHint) {
      els.composeTypeHint.textContent = hints[type] || hints.note;
    }

    var pills = els.composeTypeGroup.querySelectorAll('.type-pill');
    pills.forEach(function (pill) {
      var active = pill.getAttribute('data-type') === type;
      pill.classList.toggle('is-active', active);
      pill.setAttribute('aria-selected', active ? 'true' : 'false');
    });
  }

  function setComposeType(type) {
    els.composeType.value = type;
    setComposeTypeUi();
  }

  function composeArgsForType(type) {
    var draft = String(els.composeDraft.value || '').trim();
    var name = String(els.composeName.value || '').trim();
    if (!draft) {
      throw new Error('Draft name is required.');
    }

    if (type === 'note') {
      var content = String(els.composeContent.value || '').trim();
      if (!content) {
        throw new Error('Note body is required.');
      }
      return ['compose-note', [content, String(els.composeTags.value || '').trim(), draft]];
    }

    if (type === 'reply') {
      var replyContent = String(els.composeContent.value || '').trim();
      var eventId = String(els.composeReplyEvent.value || '').trim();
      if (!replyContent || !eventId) {
        throw new Error('Reply body and event id are required.');
      }
      return ['compose-reply', [replyContent, eventId, draft]];
    }

    if (type === 'longform') {
      var identifier = String(els.composeIdentifier.value || '').trim();
      var longformContent = String(els.composeContent.value || '').trim();
      if (!name || !identifier || !longformContent) {
        throw new Error('Post name, identifier, and body are required for long-form drafts.');
      }
      return ['compose-longform', [name, identifier, longformContent, String(els.composeSummary.value || '').trim(), draft]];
    }

    if (type === 'file-metadata') {
      var fileUrl = String(els.composeFileUrl.value || '').trim();
      var fileHash = String(els.composeFileHash.value || '').trim();
      var fileMime = String(els.composeFileMime.value || '').trim();
      var fileSize = String(els.composeFileSize.value || '').trim();
      if (!fileUrl || !fileHash || !fileMime || !fileSize) {
        throw new Error('URL, hash, mime, and size are required for file metadata drafts.');
      }
      return ['compose-file-metadata', [fileUrl, fileHash, fileMime, fileSize, draft]];
    }

    throw new Error('Unsupported compose type: ' + type);
  }

  function parseUploadBody(blob) {
    var parsed = parseMaybeJson(blob);
    if (!parsed || !parsed.body) {
      return null;
    }
    var payload = parseMaybeJson(parsed.body);
    if (!payload) {
      return null;
    }

    var meta = {
      url: '',
      hash: '',
      mime: '',
      size: ''
    };

    function maybeApply(source) {
      if (!source || typeof source !== 'object') {
        return;
      }
      meta.url = meta.url || source.url || source.download_url || source.location || '';
      meta.hash = meta.hash || source.x || source.sha256 || source.hash || '';
      meta.mime = meta.mime || source.m || source.mime || source.content_type || '';
      meta.size = meta.size || String(source.size || source.length || source.bytes || '').trim();
    }

    maybeApply(payload);
    if (Array.isArray(payload.files) && payload.files[0]) {
      maybeApply(payload.files[0]);
    }

    if (payload.nip94_event && Array.isArray(payload.nip94_event.tags)) {
      payload.nip94_event.tags.forEach(function (tag) {
        if (!Array.isArray(tag) || tag.length < 2) {
          return;
        }
        if (tag[0] === 'url') {
          meta.url = meta.url || tag[1];
        }
        if (tag[0] === 'x') {
          meta.hash = meta.hash || tag[1];
        }
        if (tag[0] === 'm') {
          meta.mime = meta.mime || tag[1];
        }
        if (tag[0] === 'size') {
          meta.size = meta.size || tag[1];
        }
      });
    }

    if (!meta.url && !meta.hash) {
      return null;
    }
    return meta;
  }

  async function runComposeUpload() {
    var relay = String(els.composeUploadRelay.value || '').trim();
    var file = String(els.composeUploadFile.value || '').trim();
    var password = String(els.composePassword.value || '');
    var profile = String(els.composeProfileId.value || '').trim();
    if (!relay || !file) {
      toast('Upload relay and file path are required.', 'bad');
      return;
    }

    var blob = await safeBackend('media-upload-nip96', [relay, file, password, profile], 'Media upload failed');
    writeLog(els.composeOutput, 'Upload result', blob);

    var meta = parseUploadBody(blob);
    if (meta) {
      els.composeFileUrl.value = meta.url || els.composeFileUrl.value;
      els.composeFileHash.value = meta.hash || els.composeFileHash.value;
      els.composeFileMime.value = meta.mime || els.composeFileMime.value;
      els.composeFileSize.value = meta.size || els.composeFileSize.value;
      setComposeType('file-metadata');
      toast('Attachment metadata filled from upload.', 'good');
      return;
    }

    toast('Upload completed. Fill metadata fields if relay omitted NIP-94 info.', 'good');
  }

  async function runComposeDraftCreate() {
    var type = String(els.composeType.value || 'note');
    var tuple = composeArgsForType(type);
    var blob = await safeBackend(tuple[0], tuple[1], 'Failed to create draft');
    writeLog(els.composeOutput, 'Draft created', blob);
    toast('Draft created.', 'good');
  }

  async function runComposePreview() {
    var draft = String(els.composeDraft.value || '').trim();
    if (!draft) {
      toast('Draft name is required.', 'bad');
      return;
    }
    var blob = await safeBackend('compose-preview', [draft], 'Failed to preview draft');
    writeLog(els.composeOutput, 'Draft preview', blob);
  }

  async function runComposeList() {
    var blob = await safeBackend('compose-list-drafts', [], 'Failed to list drafts');
    writeLog(els.composeOutput, 'Draft list', blob);
  }

  async function runComposeSign() {
    var draft = String(els.composeDraft.value || '').trim();
    var password = String(els.composePassword.value || '');
    var profile = String(els.composeProfileId.value || '').trim();
    if (!draft || !password) {
      toast('Draft and password are required to sign.', 'bad');
      return;
    }
    var blob = await safeBackend('compose-sign-draft', [draft, password, profile], 'Failed to sign draft');
    writeLog(els.composeOutput, 'Draft signed', blob);
    toast('Draft signed.', 'good');
  }

  async function runComposePublish() {
    var draft = String(els.composeDraft.value || '').trim();
    var password = String(els.composePassword.value || '');
    var profile = String(els.composeProfileId.value || '').trim();
    var relays = String(els.composeRelays.value || '').trim();
    if (!draft || !password) {
      toast('Draft and password are required to publish.', 'bad');
      return;
    }
    var blob = await safeBackend('publish-draft', [draft, password, profile, relays], 'Failed to publish draft');
    writeLog(els.composeOutput, 'Publish result', blob);
    toast('Publish request sent.', 'good');
  }

  async function runDeletePublish() {
    var eventId = String(els.deleteEventId.value || '').trim();
    var draft = String(els.deleteDraft.value || '').trim();
    var reason = String(els.deleteReason.value || '').trim();
    var password = String(els.deletePassword.value || '');
    var profile = String(els.deleteProfileId.value || '').trim();
    var relays = String(els.deleteRelays.value || '').trim();

    if (!eventId || !draft || !password) {
      toast('Event id, draft name, and password are required.', 'bad');
      return;
    }

    await safeBackend('compose-delete', [eventId, reason, draft], 'Failed to compose delete');
    var blob = await safeBackend('publish-draft', [draft, password, profile, relays], 'Failed to publish delete event');
    writeLog(els.deleteLog, 'Delete published', blob);
    toast('Delete event published.', 'good');
    closeDelete();
    await runHomeFetch().catch(function () {
      return;
    });
  }

  function setActiveListOption(listName) {
    state.selectedListName = String(listName || '').trim();
    if (state.selectedListName) {
      setRailSelection('list', state.selectedListName);
      setActiveTab('home', false);
      runLibraryListView(state.selectedListName).catch(function () {
        return;
      });
      return;
    }
    if (state.railSelectionKind === 'list') {
      setRailSelection('nav', state.activeRailNav || 'home');
      return;
    }
    syncRailSelection();
  }

  function renderLibraryList(payload) {
    var rows = payload && Array.isArray(payload.lists) ? payload.lists : [];
    state.libraryRows = rows.slice(0, 250);
    els.libraryListbox.innerHTML = '';
    if (!state.libraryRows.length) {
      var empty = document.createElement('div');
      empty.className = 'rail-list-option is-empty';
      var emptyCopy = document.createElement('span');
      emptyCopy.className = 'rail-option-copy';
      emptyCopy.appendChild(makeRailIcon('assets/folder-open.svg'));
      var emptyLabel = document.createElement('span');
      emptyLabel.className = 'rail-option-label';
      emptyLabel.textContent = 'No lists yet.';
      emptyCopy.appendChild(emptyLabel);
      empty.appendChild(emptyCopy);
      els.libraryListbox.appendChild(empty);
      state.selectedListName = '';
      if (state.railSelectionKind === 'list') {
        setRailSelection('nav', state.activeRailNav || 'home');
        return;
      }
      syncRailSelection();
      return;
    }

    state.libraryRows.forEach(function (row) {
      var listName = String(row && row.name ? row.name : '').trim();
      if (!listName) {
        return;
      }
      var button = document.createElement('button');
      button.type = 'button';
      button.className = 'rail-list-option';
      button.setAttribute('role', 'option');
      button.setAttribute('data-list-name', listName);
      button.id = optionDomId('library-option', listName);
      button.tabIndex = -1;
      button.title = 'Drop events here to add and star';

      var copy = document.createElement('span');
      copy.className = 'rail-option-copy';
      copy.appendChild(makeRailIcon('assets/folder-open.svg'));

      var label = document.createElement('span');
      label.className = 'rail-option-label';
      label.textContent = listName;
      label.title = listName;

      var badge = document.createElement('span');
      badge.className = 'rail-option-badge';
      badge.textContent = String(Number(row.count || 0));

      copy.appendChild(label);
      button.appendChild(copy);
      button.appendChild(badge);
      button.addEventListener('click', function () {
        setActiveListOption(listName);
      });
      button.addEventListener('dragover', function (event) {
        event.preventDefault();
      });
      button.addEventListener('drop', function (event) {
        event.preventDefault();
        var eventId = String(event.dataTransfer && event.dataTransfer.getData('text/x-nostr-event-id') || '').trim();
        if (!eventId) {
          return;
        }
        addEventToList(listName, eventId).catch(function () {
          return;
        });
      });

      els.libraryListbox.appendChild(button);
    });

    if (state.selectedListName && state.libraryRows.some(function (row) { return String(row.name || '') === state.selectedListName; })) {
      if (state.railSelectionKind === 'list') {
        setRailSelection('list', state.selectedListName);
      } else {
        syncRailSelection();
      }
      return;
    }
    state.selectedListName = String(state.libraryRows[0].name || '');
    if (state.railSelectionKind === 'list' && state.selectedListName) {
      setRailSelection('list', state.selectedListName);
      return;
    }
    syncRailSelection();
  }

  async function runLibraryList() {
    var blob = await safeBackend('library-list-folders', [], 'Failed to load lists');
    var parsed = parseMaybeJson(blob);
    renderLibraryList(parsed);
  }

  function renderListEventFeed(listName, eventIds) {
    var ids = Array.isArray(eventIds) ? eventIds.slice() : [];
    var cache = {};
    state.homeEvents.concat(state.discoverEvents).forEach(function (event) {
      var id = String(event && event.id || '').trim();
      if (id) {
        cache[id] = event;
      }
    });

    var payload = {
      events: ids.map(function (id) {
        if (cache[id]) {
          return cache[id];
        }
        return {
          id: id,
          pubkey: '',
          created_at: '',
          kind: 1,
          content: 'Saved event ' + shortId(id)
        };
      })
    };

    renderEventFeed(
      els.homeFeed,
      els.homeResultsSummary,
      payload,
      'No saved events in ' + listName + '.',
      'homeEvents'
    );
    if (els.homeResultsSummary) {
      els.homeResultsSummary.textContent = listName + ' · ' + String(ids.length) + ' saved event' + (ids.length === 1 ? '' : 's') + '.';
    }
  }

  async function runLibraryListView(listName) {
    var name = String(listName || state.selectedListName || '').trim();
    if (!name) {
      return;
    }
    var blob = await safeBackend('library-list-folder-events', [name], 'Failed to load list');
    var parsed = parseMaybeJson(blob);
    var events = parsed && Array.isArray(parsed.events) ? parsed.events : [];
    renderListEventFeed(name, events);
  }

  async function createLibraryList() {
    var raw = await openTextPrompt({
      title: 'Create List',
      label: 'List name',
      placeholder: 'List name'
    });
    if (raw === null) {
      return;
    }
    var name = String(raw || '').trim();
    if (!name) {
      toast('List name is required.', 'bad');
      return;
    }
    await safeBackend('library-create-folder', [name], 'Failed to create list');
    state.selectedListName = name;
    await runLibraryList();
    toast('List created.', 'good');
  }

  async function addEventToList(listName, eventId) {
    var list = String(listName || state.selectedListName || 'inbox').trim() || 'inbox';
    var id = String(eventId || '').trim();
    if (!id) {
      return;
    }
    await safeBackend('library-list-add-event', [list, id], 'Failed to add event to list');
    toast('Added to ' + list + '.', 'good');
    await runLibraryList();
  }

  async function runLibraryIngest() {
    var path = String(els.libraryAuthoredPath.value || '').trim();
    if (!path) {
      toast('Path is required.', 'bad');
      return;
    }
    var blob = await safeBackend('library-ingest-authored', [path], 'Failed to ingest authored events');
    writeLog(els.networkLog, 'Ingest authored events', blob);
    await runLibraryList();
  }

  async function runLibraryReindex() {
    await safeBackend('library-reindex', [], 'Failed to reindex library');
    toast('Library reindexed.', 'good');
    await runLibraryList();
  }

  function relayRowByUrl(url) {
    return state.relayRows.find(function (row) { return row.url === url; }) || null;
  }

  function renderRelaySelectionCard() {
    if (!els.relaySelectionTitle || !els.relaySelectionMeta) {
      return;
    }
    var row = relayRowByUrl(state.selectedRelayUrl);
    if (!row) {
      els.relaySelectionTitle.textContent = 'No relay selected';
      els.relaySelectionMeta.textContent = 'No relay metadata.';
      return;
    }
    els.relaySelectionTitle.textContent = relayDisplayLabel(row.url);
    els.relaySelectionTitle.title = row.url;
    els.relaySelectionMeta.textContent = relayRoleLabel(row) + ' · ' + row.url;
  }

  function setSelectedRelay(url) {
    state.selectedRelayUrl = String(url || '').trim();
    if (state.selectedRelayUrl && els.networkRelayUrl) {
      els.networkRelayUrl.value = state.selectedRelayUrl;
    }
    var activeOptionId = '';
    listboxOptions(els.relayListbox).forEach(function (node) {
      var active = node.getAttribute('data-relay-url') === state.selectedRelayUrl;
      node.classList.toggle('is-active', active);
      node.setAttribute('aria-selected', active ? 'true' : 'false');
      if (active) {
        activeOptionId = node.id;
      }
    });
    setListboxActiveDescendant(els.relayListbox, activeOptionId);
    renderRelaySelectionCard();
  }

  function renderRelayList(payload) {
    var relays = payload && payload.relays ? payload.relays : null;
    state.relayReady = relayConfigured(relays);
    state.homeRelayUrl = relays && relays.home ? String(relays.home || '').trim() : '';
    state.relayRows = [];
    renderSetupPanel();
    els.relayListbox.innerHTML = '';
    if (!relays || !state.relayReady) {
      var empty = document.createElement('div');
      empty.className = 'rail-list-option is-empty';
      empty.textContent = 'No relay configured yet.';
      els.relayListbox.appendChild(empty);
      setListboxActiveDescendant(els.relayListbox, '');
      setSelectedRelay('');
      return;
    }

    var all = {};
    [relays.home].concat(relays.read || []).concat(relays.write || []).forEach(function (url) {
      if (!url) {
        return;
      }
      all[url] = {
        home: relays.home === url,
        read: (relays.read || []).indexOf(url) >= 0,
        write: (relays.write || []).indexOf(url) >= 0
      };
    });

    state.relayRows = Object.keys(all).sort().map(function (url) {
      return {
        url: url,
        home: all[url].home,
        read: all[url].read,
        write: all[url].write
      };
    });

    state.relayRows.forEach(function (relay) {
      var row = document.createElement('button');
      row.type = 'button';
      row.className = 'rail-list-option';
      row.setAttribute('role', 'option');
      row.setAttribute('data-relay-url', relay.url);
      row.id = optionDomId('relay-option', relay.url);
      row.tabIndex = -1;

      var copy = document.createElement('span');
      copy.className = 'rail-option-copy';

      var label = document.createElement('span');
      label.className = 'rail-option-label';
      label.textContent = relayDisplayLabel(relay.url);
      label.title = relay.url;

      var meta = document.createElement('span');
      meta.className = 'rail-option-meta';
      meta.textContent = relayRoleLabel(relay);

      var badge = document.createElement('span');
      badge.className = 'rail-option-badge';
      badge.textContent = relay.home ? 'Home' : relay.read && relay.write ? 'Read/Write' : relay.read ? 'Read' : 'Write';

      copy.appendChild(label);
      copy.appendChild(meta);
      row.appendChild(copy);
      row.appendChild(badge);
      row.addEventListener('click', function () {
        setSelectedRelay(relay.url);
      });

      els.relayListbox.appendChild(row);
    });

    if (state.selectedRelayUrl && state.relayRows.some(function (row) { return row.url === state.selectedRelayUrl; })) {
      setSelectedRelay(state.selectedRelayUrl);
      return;
    }
    setSelectedRelay(state.relayRows[0].url);
  }

  function deletableByActiveProfile(event) {
    if (!state.activeProfilePubkey) {
      return false;
    }
    return String(event.pubkey || '').trim() === String(state.activeProfilePubkey || '').trim();
  }

  function makeFeedMenu(event, menuButton) {
    var wrap = document.createElement('div');
    wrap.className = 'menu-pop hidden';

    var reply = document.createElement('button');
    reply.type = 'button';
    reply.textContent = 'Reply';
    reply.setAttribute('data-action', 'reply');
    reply.addEventListener('click', function () {
      closeOpenMenu();
      setComposeType('reply');
      els.composeReplyEvent.value = String(event.id || '');
      els.composeContent.value = '';
      openCompose(els.composeContent);
    });

    wrap.appendChild(reply);

    if (deletableByActiveProfile(event)) {
      var del = document.createElement('button');
      del.type = 'button';
      del.textContent = 'Delete';
      del.setAttribute('data-action', 'delete');
      del.addEventListener('click', function () {
        closeOpenMenu();
        openDelete(String(event.id || ''));
      });
      wrap.appendChild(del);
    }

    menuButton.addEventListener('click', function (evt) {
      evt.stopPropagation();
      if (state.openMenu && state.openMenu !== wrap) {
        state.openMenu.classList.add('hidden');
      }
      var opening = wrap.classList.contains('hidden');
      wrap.classList.toggle('hidden', !opening);
      state.openMenu = opening ? wrap : null;
    });

    return wrap;
  }

  function extractEvents(parsed) {
    var events = [];
    if (Array.isArray(parsed)) {
      events = parsed;
    } else if (parsed && Array.isArray(parsed.events)) {
      events = parsed.events;
    } else if (parsed && Array.isArray(parsed.results)) {
      events = parsed.results;
    }
    return events;
  }

  function renderEventFeed(targetNode, summaryNode, parsed, emptyMessage, cacheKey) {
    targetNode.innerHTML = '';
    var events = extractEvents(parsed);
    state[cacheKey] = events.slice(0, 120);

    if (!events.length) {
      if (summaryNode) {
        summaryNode.textContent = emptyMessage;
      }
      feedEmpty(targetNode, emptyMessage);
      return;
    }

    if (summaryNode) {
      summaryNode.textContent = 'Showing ' + String(Math.min(events.length, 120)) + ' event' + (events.length === 1 ? '' : 's') + '.';
    }

    events.slice(0, 120).forEach(function (event) {
      var card = document.createElement('article');
      card.className = 'feed-item';
      card.draggable = !!event.id;
      if (event.id) {
        card.addEventListener('dragstart', function (dragEvent) {
          dragEvent.dataTransfer.setData('text/x-nostr-event-id', String(event.id));
          dragEvent.dataTransfer.effectAllowed = 'copy';
        });
      }

      var head = document.createElement('div');
      head.className = 'feed-head';

      var heading = document.createElement('div');
      heading.className = 'feed-heading';

      var title = document.createElement('div');
      title.className = 'feed-title';
      title.textContent = shortId(String(event.pubkey || ''));
      title.title = String(event.pubkey || '');

      var support = document.createElement('div');
      support.className = 'feed-support';
      support.textContent = formatTimestamp(event.created_at);

      var meta = document.createElement('div');
      meta.className = 'feed-meta';
      meta.textContent =
        eventKindLabel(event.kind) +
        ' · ' + shortId(String(event.id || ''));
      meta.title = String(event.id || '');

      heading.appendChild(title);
      heading.appendChild(support);
      heading.appendChild(meta);

      var menuWrap = document.createElement('div');
      menuWrap.className = 'menu-wrap';

      var menuButton = document.createElement('button');
      menuButton.className = 'menu-btn';
      menuButton.type = 'button';
      menuButton.title = 'Post actions';
      menuButton.setAttribute('aria-label', 'Post actions');
      menuButton.textContent = '⋯';

      menuWrap.appendChild(menuButton);
      menuWrap.appendChild(makeFeedMenu(event, menuButton));

      head.appendChild(heading);
      head.appendChild(menuWrap);

      var body = document.createElement('p');
      body.className = 'feed-body';
      body.textContent = String(event.content || '(empty)').slice(0, 2200);

      var actions = document.createElement('div');
      actions.className = 'feed-actions';

      var starBtn = document.createElement('button');
      starBtn.className = 'feed-icon-btn';
      starBtn.type = 'button';
      starBtn.title = 'Star to inbox';
      starBtn.setAttribute('aria-label', 'Star to inbox');
      starBtn.innerHTML = (
        '<svg viewBox="0 0 16 16" aria-hidden="true" focusable="false">' +
        '<path d="m8 2.1 1.7 3.5 3.9.6-2.8 2.7.7 3.9L8 10.9l-3.5 1.9.7-3.9-2.8-2.7 3.9-.6L8 2.1Z" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/>' +
        '</svg>'
      );
      starBtn.addEventListener('click', function () {
        if (!event.id) {
          return;
        }
        addEventToList('inbox', String(event.id))
          .then(function () {
            return;
          })
          .catch(function () {
            return;
          });
      });

      actions.appendChild(starBtn);

      card.appendChild(head);
      card.appendChild(body);
      card.appendChild(actions);
      targetNode.appendChild(card);
    });
  }

  async function runHomeFetch() {
    var args = [
      String(els.homeAuthors.value || '').trim(),
      String(els.homeKinds.value || '').trim(),
      String(els.homeSearch.value || '').trim(),
      '',
      '',
      String(els.homeLimit.value || '50').trim(),
      els.homeIncludeRemotes.checked ? '1' : '0',
      ''
    ];
    var blob = await safeBackend('timeline-fetch', args, 'Failed to fetch timeline');
    var parsed = writeLog(els.homeLog, 'Timeline fetch', blob);
    if (parsed && parsed.needs_relay) {
      state.homeEvents = [];
      renderHomeEmptyState('Add a relay in Settings to load your timeline.');
      return;
    }
    renderEventFeed(els.homeFeed, els.homeResultsSummary, parsed, 'No events returned.', 'homeEvents');
  }

  async function runDiscoverSearch() {
    var term = String(els.discoverTerm.value || '').trim();
    if (!term) {
      toast('Discover term is required.', 'bad');
      return;
    }
    var blob = await safeBackend('discover-search', [term, String(els.discoverLimit.value || '30').trim()], 'Search failed');
    var parsed = writeLog(els.discoverLog, 'Discover search', blob);
    if (parsed && parsed.needs_relay) {
      state.discoverEvents = [];
      feedEmpty(els.discoverFeed, 'Add a relay in Settings to search relay content.');
      if (els.discoverResultsSummary) {
        els.discoverResultsSummary.textContent = 'Add a relay in Settings to search relay content.';
      }
      return;
    }
    renderEventFeed(els.discoverFeed, els.discoverResultsSummary, parsed, 'No discover results returned.', 'discoverEvents');
  }

  async function runDiscoverCount() {
    var term = String(els.discoverTerm.value || '').trim();
    var blob = await safeBackend('discover-count', [term], 'Count failed');
    var parsed = writeLog(els.discoverLog, 'Discover count', blob);
    if (parsed && parsed.needs_relay) {
      if (els.discoverResultsSummary) {
        els.discoverResultsSummary.textContent = 'Add a relay in Settings to search relay content.';
      }
      return;
    }
    if (els.discoverResultsSummary && parsed && typeof parsed.count !== 'undefined') {
      els.discoverResultsSummary.textContent = 'Count returned ' + String(parsed.count) + ' matching event' + (Number(parsed.count) === 1 ? '' : 's') + '.';
    }
  }

  async function runDiscoverFilterSearch() {
    var args = [
      String(els.discoverAuthors.value || '').trim(),
      String(els.discoverKinds.value || '').trim(),
      String(els.discoverTerm.value || '').trim(),
      String(els.discoverSince.value || '').trim(),
      String(els.discoverUntil.value || '').trim(),
      String(els.discoverLimit.value || '30').trim(),
      '1',
      ''
    ];
    var blob = await safeBackend('timeline-fetch', args, 'Filtered search failed');
    var parsed = writeLog(els.discoverLog, 'Filtered search', blob);
    if (parsed && parsed.needs_relay) {
      state.discoverEvents = [];
      feedEmpty(els.discoverFeed, 'Add a relay in Settings to search relay content.');
      if (els.discoverResultsSummary) {
        els.discoverResultsSummary.textContent = 'Add a relay in Settings to search relay content.';
      }
      return;
    }
    renderEventFeed(els.discoverFeed, els.discoverResultsSummary, parsed, 'No discover results returned.', 'discoverEvents');
  }

  async function runRelayInfo() {
    var url = String(els.relayInfoUrl.value || '').trim();
    if (!url) {
      toast('Relay URL is required.', 'bad');
      return;
    }
    var blob = await safeBackend('discover-relay-info', [url], 'Relay probe failed');
    writeLog(els.discoverLog, 'Relay info', blob);
  }

  function renderPeopleRows(rows, label) {
    if (!rows.length) {
      feedEmpty(els.peopleResults, 'No ' + label + ' found.');
      return;
    }
    els.peopleResults.innerHTML = '';
    rows.forEach(function (entry) {
      var row = document.createElement('div');
      row.className = 'feed-item';
      var heading = document.createElement('div');
      heading.className = 'feed-heading';
      var title = document.createElement('div');
      title.className = 'feed-title';
      title.textContent = shortId(entry);
      title.title = entry;
      var support = document.createElement('div');
      support.className = 'feed-support';
      support.textContent = entry;
      heading.appendChild(title);
      heading.appendChild(support);
      row.appendChild(heading);
      els.peopleResults.appendChild(row);
    });
  }

  function renderFollowingList(rows) {
    if (!els.followingListbox) {
      return;
    }
    state.followingRows = Array.isArray(rows) ? rows.slice(0, 200) : [];
    els.followingListbox.innerHTML = '';
    if (!state.followingRows.length) {
      var empty = document.createElement('div');
      empty.className = 'rail-list-option is-empty';
      var emptyCopy = document.createElement('span');
      emptyCopy.className = 'rail-option-copy';
      emptyCopy.appendChild(makeRailIcon('assets/person-outline.svg'));
      var emptyLabel = document.createElement('span');
      emptyLabel.className = 'rail-option-label';
      emptyLabel.textContent = 'No follows.';
      emptyCopy.appendChild(emptyLabel);
      empty.appendChild(emptyCopy);
      els.followingListbox.appendChild(empty);
      state.activeFollowingPubkey = '';
      if (state.railSelectionKind === 'following') {
        setRailSelection('nav', state.activeRailNav || 'home');
        return;
      }
      syncRailSelection();
      return;
    }

    state.followingRows.forEach(function (pubkey) {
      var row = document.createElement('button');
      row.type = 'button';
      row.className = 'rail-list-option';
      row.setAttribute('role', 'option');
      row.setAttribute('data-following-pubkey', pubkey);
      row.id = optionDomId('following-option', pubkey);
      row.tabIndex = -1;
      row.setAttribute('aria-selected', 'false');

      var copy = document.createElement('span');
      copy.className = 'rail-option-copy';
      copy.appendChild(makeRailIcon('assets/person-outline.svg'));

      var label = document.createElement('span');
      label.className = 'rail-option-label';
      label.textContent = shortId(pubkey);
      label.title = pubkey;

      copy.appendChild(label);
      row.appendChild(copy);
      row.addEventListener('click', function () {
        setFollowingActiveOption(pubkey);
      });
      els.followingListbox.appendChild(row);
    });

    if (state.activeFollowingPubkey && state.followingRows.indexOf(state.activeFollowingPubkey) >= 0) {
      if (state.railSelectionKind === 'following') {
        setRailSelection('following', state.activeFollowingPubkey);
      } else {
        syncRailSelection();
      }
      return;
    }
    state.activeFollowingPubkey = state.followingRows[0];
    if (state.railSelectionKind === 'following' && state.activeFollowingPubkey) {
      setRailSelection('following', state.activeFollowingPubkey);
      return;
    }
    syncRailSelection();
  }

  function setFollowingActiveOption(pubkey) {
    state.activeFollowingPubkey = pubkey || '';
    if (state.activeFollowingPubkey) {
      setRailSelection('following', state.activeFollowingPubkey);
    } else if (state.railSelectionKind === 'following') {
      setRailSelection('nav', state.activeRailNav || 'home');
    } else {
      syncRailSelection();
    }
    if (els.peoplePubkey && state.activeFollowingPubkey) {
      els.peoplePubkey.value = state.activeFollowingPubkey;
    }
  }

  function extractFollowingPubkeys(events) {
    if (!Array.isArray(events) || !events.length) {
      return [];
    }
    var latest = events[0];
    var follows = [];
    (latest.tags || []).forEach(function (tag) {
      if (!Array.isArray(tag) || tag.length < 2) {
        return;
      }
      if (tag[0] === 'p' && tag[1]) {
        follows.push(String(tag[1]));
      }
    });

    var uniq = [];
    var seen = {};
    follows.forEach(function (pubkey) {
      if (seen[pubkey]) {
        return;
      }
      seen[pubkey] = true;
      uniq.push(pubkey);
    });
    return uniq;
  }

  async function fetchFollowingPubkeys() {
    if (!state.activeProfilePubkey || !state.relayReady) {
      return [];
    }
    var blob = await safeBackend(
      'timeline-fetch',
      [state.activeProfilePubkey, '3', '', '', '', '2', '1', ''],
      'Failed to load following'
    );
    var parsed = parseMaybeJson(blob);
    if (parsed && parsed.needs_relay) {
      return [];
    }
    var events = parsed && Array.isArray(parsed.events) ? parsed.events : [];
    return extractFollowingPubkeys(events);
  }

  async function runFollowingList() {
    var follows = mergeFollowingRows(await fetchFollowingPubkeys(), state.manualFollowingRows);
    renderFollowingList(follows);
  }

  async function runPeopleFollowing() {
    if (!state.activeProfilePubkey) {
      toast('Set an active profile first to load following.', 'bad');
      return;
    }
    var follows = mergeFollowingRows(await fetchFollowingPubkeys(), state.manualFollowingRows);
    renderFollowingList(follows);
    renderPeopleRows(follows, 'following');
    toast('Following list loaded.', 'good');
  }

  async function addFollowingPubkey() {
    var input = await openTextPrompt({
      title: 'Add Follow',
      label: 'Pubkey (hex)',
      placeholder: '64-character hex pubkey',
      wide: true
    });
    if (input === null) {
      return;
    }
    var pubkey = normalizePubkey(input);
    if (!pubkey) {
      toast('Pubkey must be 64 hex characters.', 'bad');
      return;
    }
    if (state.manualFollowingRows.indexOf(pubkey) < 0) {
      state.manualFollowingRows = state.manualFollowingRows.concat([pubkey]);
    }
    if (state.bridge) {
      await saveUiPref('manual_follows', state.manualFollowingRows.join(','));
    }
    renderFollowingList(mergeFollowingRows(state.followingRows, state.manualFollowingRows));
    toast('Follow added.', 'good');
  }

  async function runPeopleFollowers() {
    var target = String(els.peoplePubkey.value || '').trim() || state.activeProfilePubkey;
    if (!target) {
      toast('Enter a pubkey or set an active profile.', 'bad');
      return;
    }

    var blob = await safeBackend(
      'timeline-fetch',
      ['', '3', '', '', '', '200', '1', target],
      'Failed to load followers'
    );
    var parsed = parseMaybeJson(blob);
    if (parsed && parsed.needs_relay) {
      toast('Add a relay first.', 'bad');
      return;
    }
    var events = parsed && Array.isArray(parsed.events) ? parsed.events : [];
    var seen = {};
    var followers = [];

    events.forEach(function (event) {
      var author = String(event.pubkey || '').trim();
      if (!author || seen[author]) {
        return;
      }
      seen[author] = true;
      followers.push(author);
    });

    renderPeopleRows(followers, 'followers');
    toast('Followers lookup complete.', 'good');
  }

  async function runRelayAction(command, args, label) {
    var blob = await safeBackend(command, args, label + ' failed');
    var parsed = writeLog(els.networkLog, label, blob);
    if (parsed && parsed.relays) {
      renderRelayList(parsed);
      return parsed;
    }
    return parsed;
  }

  async function runRelayList() {
    var parsed = await runRelayAction('relay-list', [], 'Relay list');
    if (parsed) {
      renderRelayList(parsed);
    }
  }

  async function runDoctor(targetNode) {
    var blob = await safeBackend('doctor', [], 'Doctor failed');
    writeLog(targetNode, 'Doctor', blob);
  }

  async function runStonrInfo(command, label) {
    var envPath = String(els.networkStonrEnv.value || '.env').trim() || '.env';
    var blob = await safeBackend(command, [envPath], label + ' failed');
    writeLog(els.networkLog, label, blob);
  }

  function setSelectedProfile(profileId) {
    state.selectedProfileId = String(profileId || '').trim();
    if (!els.profileListbox) {
      return;
    }
    var activeOptionId = '';
    var options = listboxOptions(els.profileListbox);
    options.forEach(function (node) {
      var active = node.getAttribute('data-profile-id') === state.selectedProfileId;
      node.classList.toggle('is-active', active);
      node.setAttribute('aria-selected', active ? 'true' : 'false');
      if (active) {
        activeOptionId = node.id;
      }
    });
    setListboxActiveDescendant(els.profileListbox, activeOptionId);
  }

  function renderProfileList(parsed) {
    if (!els.profileListbox) {
      return;
    }
    var profiles = parsed && Array.isArray(parsed.profiles) ? parsed.profiles.slice() : [];
    els.profileListbox.innerHTML = '';

    if (!profiles.length) {
      var empty = document.createElement('div');
      empty.className = 'rail-list-option is-empty';
      empty.textContent = 'No profiles yet.';
      els.profileListbox.appendChild(empty);
      setListboxActiveDescendant(els.profileListbox, '');
      setSelectedProfile('');
      return;
    }

    profiles.forEach(function (profile) {
      var button = document.createElement('button');
      button.type = 'button';
      button.className = 'rail-list-option';
      button.setAttribute('role', 'option');
      button.setAttribute('data-profile-id', String(profile.id || ''));
      button.id = optionDomId('profile-option', String(profile.id || ''));
      button.tabIndex = -1;

      var copy = document.createElement('span');
      copy.className = 'rail-option-copy';

      var label = document.createElement('span');
      label.className = 'rail-option-label';
      label.textContent = String(profile.name || 'Unnamed profile');
      label.title = String(profile.pubkey || '');

      var meta = document.createElement('span');
      meta.className = 'rail-option-meta';
      meta.textContent = shortId(String(profile.pubkey || profile.id || ''));

      var badge = document.createElement('span');
      badge.className = 'rail-option-badge';
      badge.textContent = profile.id === state.activeProfileId ? 'Active' : 'Profile';

      copy.appendChild(label);
      copy.appendChild(meta);
      button.appendChild(copy);
      button.appendChild(badge);
      button.addEventListener('click', function () {
        setSelectedProfile(profile.id);
      });
      button.addEventListener('dblclick', function () {
        setSelectedProfile(profile.id);
        runProfileUse().catch(function () {
          return;
        });
      });

      els.profileListbox.appendChild(button);
    });

    var preferred = state.selectedProfileId || state.activeProfileId || '';
    if (!profiles.some(function (profile) { return profile.id === preferred; })) {
      preferred = profiles[0].id;
    }
    setSelectedProfile(preferred);
  }

  async function loadProfiles() {
    if (!state.bridge) {
      return;
    }
    state.activeProfileId = '';
    state.activeProfileName = '';
    state.activeProfilePubkey = '';
    state.profiles = [];
    try {
      var blob = await backend('profile-list', []);
      var parsed = parseMaybeJson(blob);
      if (!parsed || !Array.isArray(parsed.profiles)) {
        renderProfileList({ profiles: [] });
        if (els.profileSummary) {
          els.profileSummary.textContent = activeProfileLabel();
        }
        renderActiveProfileButton();
        renderProfileMenuList();
        renderSetupPanel();
        return;
      }

      state.activeProfileId = String(parsed.active_profile || '').trim();
      state.profiles = parsed.profiles.slice();
      parsed.profiles.forEach(function (profile) {
        if (profile.id === state.activeProfileId) {
          state.activeProfileName = String(profile.name || '').trim();
          state.activeProfilePubkey = String(profile.pubkey || '').trim();
        }
      });

      if (els.profileSummary) {
        els.profileSummary.textContent = activeProfileLabel();
      }
      renderActiveProfileButton();
      renderProfileMenuList();
      if (state.activeProfilePubkey && !els.peoplePubkey.value) {
        els.peoplePubkey.value = state.activeProfilePubkey;
      }
      renderProfileList(parsed);
      renderSetupPanel();
    } catch (_error) {
      renderProfileList({ profiles: [] });
      if (els.profileSummary) {
        els.profileSummary.textContent = activeProfileLabel();
      }
      renderActiveProfileButton();
      renderProfileMenuList();
      renderSetupPanel();
      return;
    }
  }

  async function runProfileCreate() {
    var name = String(els.profileCreateName.value || '').trim();
    var password = String(els.profileCreatePassword.value || '');
    var secret = String(els.profileCreateSecret.value || '').trim();
    var setActive = els.profileCreateSetActive && els.profileCreateSetActive.checked ? '1' : '0';
    if (!name || !password) {
      toast('Profile name and password are required.', 'bad');
      return;
    }
    var blob = await safeBackend('profile-create', [name, password, secret, setActive], 'Profile create failed');
    writeLog(els.profileLog, 'Profile created', blob);
    els.profileCreatePassword.value = '';
    els.profileCreateSecret.value = '';
    if (setActive === '1') {
      els.profileCreateName.value = '';
    }
    await loadProfiles();
    toast('Profile created.', 'good');
  }

  async function runProfileImport() {
    var name = String(els.profileImportName.value || '').trim();
    var password = String(els.profileImportPassword.value || '');
    var ncryptsec = String(els.profileImportNcryptsec.value || '').trim();
    if (!name || !password || !ncryptsec) {
      toast('Import name, password, and ncryptsec are required.', 'bad');
      return;
    }
    var blob = await safeBackend('profile-import', [name, password, ncryptsec, '1'], 'Profile import failed');
    writeLog(els.profileLog, 'Profile imported', blob);
    els.profileImportPassword.value = '';
    els.profileImportNcryptsec.value = '';
    els.profileImportName.value = '';
    await loadProfiles();
    toast('Profile imported.', 'good');
  }

  async function runProfileUse() {
    var profileId = String(state.selectedProfileId || '').trim();
    if (!profileId) {
      toast('Select a profile first.', 'bad');
      return;
    }
    var blob = await safeBackend('profile-use', [profileId], 'Profile switch failed');
    writeLog(els.profileLog, 'Active profile', blob);
    await loadProfiles();
    toast('Active profile updated.', 'good');
  }

  function bindComposeTypePills() {
    var pills = els.composeTypeGroup.querySelectorAll('.type-pill');
    pills.forEach(function (pill) {
      pill.addEventListener('click', function () {
        var type = pill.getAttribute('data-type');
        setComposeType(type);
      });
    });
  }

  function bindForms() {
    if (els.recommendedRelaysDismiss) {
      els.recommendedRelaysDismiss.addEventListener('click', function () {
        saveRecommendedRelaysNotice('dismissed').catch(function () {
          return;
        });
      });
    }

    if (els.recommendedRelaysAdd) {
      els.recommendedRelaysAdd.addEventListener('click', function () {
        addRecommendedRelays().catch(function () {
          return;
        });
      });
    }

    if (els.recommendedRelaysOpenSettings) {
      els.recommendedRelaysOpenSettings.addEventListener('click', function () {
        openSettings(els.recommendedRelaysOpenSettings);
      });
    }

    if (els.settingsAddRecommendedRelays) {
      els.settingsAddRecommendedRelays.addEventListener('click', function () {
        addRecommendedRelays().catch(function () {
          return;
        });
      });
    }

    if (els.listCreate) {
      els.listCreate.addEventListener('click', function () {
        createLibraryList().catch(function () {
          return;
        });
      });
    }
    if (els.followingAdd) {
      els.followingAdd.addEventListener('click', function () {
        addFollowingPubkey().catch(function () {
          return;
        });
      });
    }

    els.homeForm.addEventListener('submit', function (event) {
      event.preventDefault();
      runHomeFetch().catch(function () {
        return;
      });
    });

    els.discoverForm.addEventListener('submit', function (event) {
      event.preventDefault();
      runDiscoverSearch().catch(function () {
        return;
      });
    });

    els.discoverCount.addEventListener('click', function () {
      runDiscoverCount().catch(function () {
        return;
      });
    });

    els.discoverFilterSearch.addEventListener('click', function () {
      runDiscoverFilterSearch().catch(function () {
        return;
      });
    });

    els.relayInfoForm.addEventListener('submit', function (event) {
      event.preventDefault();
      runRelayInfo().catch(function () {
        return;
      });
    });

    els.peopleLoadFollowing.addEventListener('click', function () {
      runPeopleFollowing().catch(function () {
        return;
      });
    });

    els.peopleLoadFollowers.addEventListener('click', function () {
      runPeopleFollowers().catch(function () {
        return;
      });
    });

    bindComposeTypePills();

    if (els.profileCreateForm) {
      els.profileCreateForm.addEventListener('submit', function (event) {
        event.preventDefault();
        runProfileCreate().catch(function () {
          return;
        });
      });
    }

    if (els.profileImportForm) {
      els.profileImportForm.addEventListener('submit', function (event) {
        event.preventDefault();
        runProfileImport().catch(function () {
          return;
        });
      });
    }

    if (els.profileUse) {
      els.profileUse.addEventListener('click', function () {
        runProfileUse().catch(function () {
          return;
        });
      });
    }

    els.composeForm.addEventListener('submit', function (event) {
      event.preventDefault();
      try {
        runComposeDraftCreate().catch(function () {
          return;
        });
      } catch (error) {
        toast(String(error.message || error), 'bad');
      }
    });

    els.composeUploadAction.addEventListener('click', function () {
      runComposeUpload().catch(function () {
        return;
      });
    });

    els.composePreview.addEventListener('click', function () {
      runComposePreview().catch(function () {
        return;
      });
    });

    els.composeList.addEventListener('click', function () {
      runComposeList().catch(function () {
        return;
      });
    });

    els.composeSign.addEventListener('click', function () {
      runComposeSign().catch(function () {
        return;
      });
    });

    els.composePublish.addEventListener('click', function () {
      runComposePublish().catch(function () {
        return;
      });
    });

    els.deleteForm.addEventListener('submit', function (event) {
      event.preventDefault();
      runDeletePublish().catch(function () {
        return;
      });
    });

    if (els.promptForm) {
      els.promptForm.addEventListener('submit', function (event) {
        event.preventDefault();
        closePrompt(String(els.promptInput.value || ''));
      });
    }

    els.libraryIngestForm.addEventListener('submit', function (event) {
      event.preventDefault();
      runLibraryIngest().catch(function () {
        return;
      });
    });

    els.libraryReindex.addEventListener('click', function () {
      runLibraryReindex().catch(function () {
        return;
      });
    });

    els.networkRelayAdd.addEventListener('click', function () {
      var url = String(els.networkRelayUrl.value || '').trim();
      if (!url) {
        toast('Relay URL is required.', 'bad');
        return;
      }
      runRelayAction('relay-add', [url, String(els.networkRelayMode.value || 'both')], 'Relay add').catch(function () {
        return;
      });
    });

    els.networkRelayRemove.addEventListener('click', function () {
      var url = String(els.networkRelayUrl.value || '').trim();
      if (!url) {
        toast('Relay URL is required.', 'bad');
        return;
      }
      runRelayAction('relay-remove', [url], 'Relay remove').catch(function () {
        return;
      });
    });

    els.networkRelayHome.addEventListener('click', function () {
      var url = String(els.networkRelayUrl.value || '').trim();
      if (!url) {
        toast('Relay URL is required.', 'bad');
        return;
      }
      runRelayAction('relay-set-home', [url], 'Relay set home').catch(function () {
        return;
      });
    });

    els.networkRelayProbe.addEventListener('click', function () {
      var url = String(els.networkRelayUrl.value || '').trim();
      runRelayAction('relay-probe', [url], 'Relay probe').catch(function () {
        return;
      });
    });

    els.networkRelayList.addEventListener('click', function () {
      runRelayList().catch(function () {
        return;
      });
    });

    els.networkDoctor.addEventListener('click', function () {
      runDoctor(els.networkLog).catch(function () {
        return;
      });
    });

    els.networkStonrConfig.addEventListener('click', function () {
      runStonrInfo('stonr-print-config', 'Stonr config').catch(function () {
        return;
      });
    });

    els.networkStonrMirror.addEventListener('click', function () {
      runStonrInfo('stonr-mirror-status', 'Stonr mirror status').catch(function () {
        return;
      });
    });

    els.networkStonrRetention.addEventListener('click', function () {
      runStonrInfo('stonr-retention-status', 'Stonr retention status').catch(function () {
        return;
      });
    });

    els.settingsRunDoctor.addEventListener('click', function () {
      runDoctor(els.doctorOutput).catch(function () {
        return;
      });
    });

    bindListboxKeyboard(els.relayListbox, 'data-relay-url', function (value) {
      setSelectedRelay(value);
    }, function (value) {
      setSelectedRelay(value);
    });

    bindListboxKeyboard(els.railNavListbox, 'data-rail-nav', function (value) {
      setActiveRailNav(value, true);
    }, function (value) {
      setActiveRailNav(value, true);
    });

    bindListboxKeyboard(els.libraryListbox, 'data-list-name', function (value) {
      setActiveListOption(value);
    }, function (value) {
      setActiveListOption(value);
    });

    bindListboxKeyboard(els.followingListbox, 'data-following-pubkey', function (value) {
      setFollowingActiveOption(value);
    }, function (value) {
      setFollowingActiveOption(value);
    });

    bindListboxKeyboard(els.profileListbox, 'data-profile-id', function (value) {
      setSelectedProfile(value);
    }, function (value) {
      setSelectedProfile(value);
      runProfileUse().catch(function () {
        return;
      });
    });
  }

  function bindDrawers() {
    els.settingsOpen.addEventListener('click', function () {
      if (els.settingsBackdrop.classList.contains('hidden')) {
        openSettings(els.settingsOpen);
      } else {
        closeSettings();
      }
    });
    els.settingsClose.addEventListener('click', closeSettings);

    els.deleteClose.addEventListener('click', closeDelete);
    if (els.promptCancel) {
      els.promptCancel.addEventListener('click', function () {
        closePrompt(null);
      });
    }

    [els.settingsBackdrop, els.deleteBackdrop, els.promptBackdrop].forEach(function (backdrop) {
      backdrop.addEventListener('click', function (event) {
        if (event.target !== backdrop) {
          return;
        }
        if (backdrop === els.settingsBackdrop) {
          closeSettings();
        }
        if (backdrop === els.deleteBackdrop) {
          closeDelete();
        }
        if (backdrop === els.promptBackdrop) {
          closePrompt(null);
        }
      });
    });

    document.addEventListener('keydown', function (event) {
      if (event.key !== 'Escape') {
        return;
      }
      if (themeMenuOpen()) {
        closeThemeMenu(true);
        return;
      }
      if (profileMenuOpen()) {
        closeProfileMenu(true);
        return;
      }
      closeOpenMenu();
      if (!els.deleteBackdrop.classList.contains('hidden')) {
        closeDelete();
        return;
      }
      if (!els.promptBackdrop.classList.contains('hidden')) {
        closePrompt(null);
        return;
      }
      if (!els.settingsBackdrop.classList.contains('hidden')) {
        closeSettings();
      }
    });
  }

  function disableActionsWhenNoBridge() {
    if (state.bridge) {
      return;
    }

    function allowLocalUi(node) {
      if (!node) {
        return false;
      }
      if (
        node === els.settingsOpen ||
        node === els.settingsClose ||
        node === els.deleteClose ||
        node === els.promptCancel ||
        node === els.promptInput ||
        node === els.promptSubmit ||
        node === els.themePickerBtn ||
        node === els.themeSelect ||
        node === els.profilePickerBtn
      ) {
        return true;
      }
      if (node.getAttribute && node.getAttribute('role') === 'tab') {
        return true;
      }
      if (node.closest && (node.closest('#theme-picker-menu') || node.closest('#profile-picker-menu') || node.closest('#compose-type-group'))) {
        return true;
      }
      return false;
    }

    var interactive = document.querySelectorAll('button, input, select, textarea');
    interactive.forEach(function (node) {
      if (allowLocalUi(node)) {
        return;
      }
      node.disabled = true;
    });
    els.settingsOpen.disabled = false;
    if (els.settingsClose) {
      els.settingsClose.disabled = false;
    }
    if (els.deleteClose) {
      els.deleteClose.disabled = false;
    }
    if (els.promptCancel) {
      els.promptCancel.disabled = false;
    }
    if (els.promptInput) {
      els.promptInput.disabled = false;
    }
    if (els.promptSubmit) {
      els.promptSubmit.disabled = false;
    }
    if (els.themePickerBtn) {
      els.themePickerBtn.disabled = false;
    }
    if (els.profilePickerBtn) {
      els.profilePickerBtn.disabled = false;
    }
    els.themeSelect.disabled = false;
  }

  async function loadPreferences() {
    if (!state.bridge) {
      return {};
    }
    try {
      var blob = await backend('get-ui-prefs', []);
      return parseKv(blob);
    } catch (_error) {
      return {};
    }
  }

  async function loadThemes() {
    if (!state.bridge) {
      setThemeOptions(['wizard']);
      return;
    }
    try {
      var blob = await backend('list-themes', []);
      setThemeOptions(listThemeNamesFromBlob(blob));
    } catch (_error) {
      setThemeOptions(['wizard']);
    }
  }

  function scheduleRefresh() {
    if (state.refreshTimer) {
      clearInterval(state.refreshTimer);
      state.refreshTimer = null;
    }
    if (!state.bridge) {
      return;
    }

    async function refreshVisible() {
      if (document.hidden) {
        return;
      }
      if (!els.deleteBackdrop.classList.contains('hidden')) {
        return;
      }
      if (!els.promptBackdrop.classList.contains('hidden')) {
        return;
      }
      if (!els.settingsBackdrop.classList.contains('hidden')) {
        return;
      }

      if (state.activeTab === 'home') {
        await runHomeFetch().catch(function () {
          return;
        });
      } else if (state.activeTab === 'discover') {
        if (String(els.discoverTerm.value || '').trim()) {
          await runDiscoverSearch().catch(function () {
            return;
          });
        }
      }
      await runLibraryList().catch(function () {
        return;
      });
      await runFollowingList().catch(function () {
        return;
      });
    }

    state.refreshTimer = setInterval(function () {
      refreshVisible().catch(function () {
        return;
      });
    }, 30000);

    document.addEventListener('visibilitychange', function () {
      if (!document.hidden) {
        refreshVisible().catch(function () {
          return;
        });
      }
    });

    window.addEventListener('focus', function () {
      refreshVisible().catch(function () {
        return;
      });
    });
  }

  async function init() {
    state.bridge = bridgeAvailable();
    bindTabSemantics();
    bindForms();
    bindDrawers();
    bindThemeControls();
    bindRailResizer();
    setComposeType('note');
    renderRecommendedRelayLists();

    document.addEventListener('click', function (event) {
      maybeCloseThemeMenuFromPointer(event);
      if (!state.openMenu) {
        return;
      }
      if (event.target.closest('.menu-wrap')) {
        return;
      }
      closeOpenMenu();
    });

    await loadThemes();
    var prefs = await loadPreferences();

    var prefTheme = String(prefs.theme || state.theme || 'wizard').trim();
    if (state.themes.indexOf(prefTheme) < 0 && state.themes.length) {
      prefTheme = state.themes[0];
    }
    applyTheme(prefTheme || 'wizard');

    var prefTab = String(prefs.active_tab || 'home').trim();
    if (TAB_IDS.indexOf(prefTab) < 0) {
      prefTab = 'home';
    }
    setActiveTab(prefTab, false);

    applyRailWidth(prefs.rail_width || 352);
    state.recommendedRelaysNotice = String(prefs.recommended_relays_notice || '').trim();
    state.manualFollowingRows = parsePubkeyCsv(prefs.manual_follows || '');
    renderRailNavigation();
    renderActiveProfileButton();
    renderProfileMenuList();
    renderSetupPanel();
    renderRelaySelectionCard();
    renderFollowingList(state.manualFollowingRows);
    renderHomeEmptyState('Loading timeline...');
    if (els.discoverFeed) {
      feedEmpty(els.discoverFeed, 'Run a search to inspect relay results.');
    }

    if (state.bridge) {
      await loadProfiles();
      await runRelayList().catch(function () {
        return;
      });
      await runLibraryList().catch(function () {
        return;
      });
      if (!state.relayReady) {
        renderHomeEmptyState('Add at least one relay in Settings to load your timeline.');
      }
    } else {
      renderHomeEmptyState('Open this app in the Wizardry desktop host to load backend data.');
      if (els.discoverResultsSummary) {
        els.discoverResultsSummary.textContent = 'Open this app in the Wizardry desktop host to run relay searches.';
      }
      feedEmpty(els.discoverFeed, 'Open this app in the Wizardry desktop host to run relay searches.');
    }

    disableActionsWhenNoBridge();
    scheduleRefresh();
    finishBoot();
    startInitialRefresh();
  }

  init().catch(function (error) {
    console.error(error);
    toast(String((error && error.message) || error), 'bad');
    finishBoot();
  });
})();
