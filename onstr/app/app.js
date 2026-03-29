(function () {
  var state = {
    bridge: false,
    hostBootReadySent: false,
    activeTab: 'home',
    activeLibraryBucket: 'all',
    activeLibraryEventId: '',
    activeProfilePubkey: '',
    themes: [],
    theme: 'wizard',
    openMenu: null,
    refreshTimer: null
  };

  var TAB_IDS = ['home', 'discover'];
  var BUCKETS = ['all', 'liked', 'commented', 'starred', 'saved'];

  var COMMAND_ALLOWLIST = Object.freeze({
    'get-ui-prefs': true,
    'set-ui-pref': true,
    'list-themes': true,
    'profile-list': true,
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
    toast: document.getElementById('toast'),

    tabList: document.getElementById('primary-tabs'),

    themeLink: document.getElementById('theme-link'),
    themeSelect: document.getElementById('theme-select'),
    themeQuick: document.getElementById('theme-quick-select'),

    settingsOpen: document.getElementById('open-settings'),
    settingsOpenRelays: document.getElementById('open-settings-relays'),
    settingsClose: document.getElementById('close-settings'),
    settingsBackdrop: document.getElementById('drawer-backdrop'),

    composeOpen: document.getElementById('open-compose'),
    composeClose: document.getElementById('close-compose'),
    composeBackdrop: document.getElementById('compose-backdrop'),

    deleteClose: document.getElementById('delete-close'),
    deleteBackdrop: document.getElementById('delete-backdrop'),

    homeForm: document.getElementById('home-form'),
    homeAuthors: document.getElementById('home-authors'),
    homeKinds: document.getElementById('home-kinds'),
    homeSearch: document.getElementById('home-search'),
    homeLimit: document.getElementById('home-limit'),
    homeIncludeRemotes: document.getElementById('home-include-remotes'),
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
    relayInfoForm: document.getElementById('relay-info-form'),
    relayInfoUrl: document.getElementById('relay-info-url'),
    discoverLog: document.getElementById('discover-log'),

    peopleLoadFollowing: document.getElementById('people-load-following'),
    peopleLoadFollowers: document.getElementById('people-load-followers'),
    peoplePubkey: document.getElementById('people-pubkey'),
    peopleResults: document.getElementById('people-results'),

    composeType: document.getElementById('compose-type'),
    composeTypeGroup: document.getElementById('compose-type-group'),
    composeForm: document.getElementById('compose-form'),
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

    libraryBucketNav: document.getElementById('library-bucket-nav'),
    libraryListbox: document.getElementById('library-listbox'),
    libraryEventId: document.getElementById('library-event-id'),
    libraryReindex: document.getElementById('library-reindex'),
    libraryStar: document.getElementById('library-star'),
    libraryUnstar: document.getElementById('library-unstar'),
    librarySave: document.getElementById('library-save'),
    libraryUnsave: document.getElementById('library-unsave'),
    libraryIngestForm: document.getElementById('library-ingest-form'),
    libraryAuthoredPath: document.getElementById('library-authored-path'),

    relayListbox: document.getElementById('relay-listbox'),

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

  function revealBootUi() {
    document.body.classList.remove('onstr-booting');
    if (state.hostBootReadySent || !state.bridge) {
      return;
    }
    state.hostBootReadySent = true;
    requestAnimationFrame(function () {
      requestAnimationFrame(function () {
        execArgv(['__wizardry_host_boot_ready']).catch(function () {
          return;
        });
      });
    });
  }

  function closeOpenMenu() {
    if (!state.openMenu) {
      return;
    }
    state.openMenu.classList.add('hidden');
    state.openMenu = null;
  }

  function setActiveTab(tabId, focusTab) {
    if (TAB_IDS.indexOf(tabId) < 0) {
      return;
    }
    state.activeTab = tabId;

    TAB_IDS.forEach(function (id) {
      var tab = document.getElementById('tab-btn-' + id);
      var panel = document.getElementById('tab-' + id);
      var active = id === tabId;
      tab.classList.toggle('is-active', active);
      tab.setAttribute('aria-selected', active ? 'true' : 'false');
      tab.tabIndex = active ? 0 : -1;
      panel.classList.toggle('hidden', !active);
      if (active && focusTab) {
        tab.focus();
      }
    });

    saveUiPref('active_tab', tabId).catch(function () {
      return;
    });
  }

  function bindTabSemantics() {
    TAB_IDS.forEach(function (id) {
      var tab = document.getElementById('tab-btn-' + id);
      tab.addEventListener('click', function () {
        setActiveTab(id, false);
      });
    });

    els.tabList.addEventListener('keydown', function (event) {
      var key = event.key;
      if (['ArrowRight', 'ArrowLeft', 'Home', 'End'].indexOf(key) < 0) {
        return;
      }
      event.preventDefault();
      var current = TAB_IDS.indexOf(state.activeTab);
      if (key === 'Home') {
        setActiveTab(TAB_IDS[0], true);
        return;
      }
      if (key === 'End') {
        setActiveTab(TAB_IDS[TAB_IDS.length - 1], true);
        return;
      }
      var next = current + (key === 'ArrowRight' ? 1 : -1);
      if (next < 0) {
        next = TAB_IDS.length - 1;
      }
      if (next >= TAB_IDS.length) {
        next = 0;
      }
      setActiveTab(TAB_IDS[next], true);
    });
  }

  function openSettings(focusRelays) {
    els.settingsBackdrop.classList.remove('hidden');
    els.settingsBackdrop.setAttribute('aria-hidden', 'false');
    if (focusRelays) {
      els.networkRelayUrl.focus();
    } else {
      els.themeSelect.focus();
    }
  }

  function closeSettings() {
    els.settingsBackdrop.classList.add('hidden');
    els.settingsBackdrop.setAttribute('aria-hidden', 'true');
    els.settingsOpen.focus();
  }

  function openCompose() {
    els.composeBackdrop.classList.remove('hidden');
    els.composeBackdrop.setAttribute('aria-hidden', 'false');
    els.composeDraft.focus();
  }

  function closeCompose() {
    els.composeBackdrop.classList.add('hidden');
    els.composeBackdrop.setAttribute('aria-hidden', 'true');
    els.composeOpen.focus();
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
    [els.themeSelect, els.themeQuick].forEach(function (node) {
      node.innerHTML = '';
      names.forEach(function (name) {
        var opt = document.createElement('option');
        opt.value = name;
        opt.textContent = name;
        node.appendChild(opt);
      });
    });
  }

  function applyTheme(name) {
    if (!name) {
      return;
    }
    state.theme = name;
    els.themeLink.href = 'themes/' + name + '.css';
    els.themeSelect.value = name;
    els.themeQuick.value = name;
  }

  async function saveUiPref(key, value) {
    if (!state.bridge) {
      return;
    }
    await backend('set-ui-pref', [key, String(value || '')]);
  }

  function bindThemeControls() {
    function onThemeSelect(node) {
      var next = String(node.value || '').trim();
      if (!next) {
        return;
      }
      applyTheme(next);
      saveUiPref('theme', next).catch(function () {
        return;
      });
    }

    [els.themeSelect, els.themeQuick].forEach(function (node) {
      node.addEventListener('change', function () {
        onThemeSelect(node);
      });
      node.addEventListener('input', function () {
        onThemeSelect(node);
      });
      node.addEventListener('keydown', function (event) {
        if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') {
          return;
        }
        requestAnimationFrame(function () {
          onThemeSelect(node);
        });
      });
    });
  }

  function setComposeTypeUi() {
    var type = String(els.composeType.value || 'note');
    els.composeNoteFields.classList.toggle('hidden', type !== 'note');
    els.composeReplyFields.classList.toggle('hidden', type !== 'reply');
    els.composeLongformFields.classList.toggle('hidden', type !== 'longform');
    els.composeFileFields.classList.toggle('hidden', type !== 'file-metadata');
    els.composeContentRow.classList.toggle('hidden', type === 'file-metadata');

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

  function composeTagsWithName() {
    var raw = String(els.composeTags.value || '').trim();
    var name = String(els.composeName.value || '').trim();
    if (!name) {
      return raw;
    }
    if (!raw) {
      return 'title:' + name;
    }
    return raw + ',title:' + name;
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
      return ['compose-note', [content, composeTagsWithName(), draft]];
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

  function normalizeLibraryList(payload, bucket) {
    var out = [];
    if (payload && payload.bucket && Array.isArray(payload.events)) {
      payload.events.forEach(function (id) {
        out.push({ id: String(id), bucket: payload.bucket });
      });
      return out;
    }

    var lib = payload && payload.library ? payload.library : null;
    if (!lib) {
      return out;
    }

    if (bucket !== 'all' && Array.isArray(lib[bucket])) {
      lib[bucket].forEach(function (id) {
        out.push({ id: String(id), bucket: bucket });
      });
      return out;
    }

    var seen = {};
    ['liked', 'commented', 'starred', 'saved'].forEach(function (name) {
      var arr = Array.isArray(lib[name]) ? lib[name] : [];
      arr.forEach(function (id) {
        var key = String(id);
        if (!seen[key]) {
          seen[key] = [];
        }
        seen[key].push(name);
      });
    });

    Object.keys(seen).forEach(function (id) {
      out.push({ id: id, bucket: seen[id].join(',') });
    });

    return out;
  }

  function setLibraryActiveOption(eventId) {
    state.activeLibraryEventId = eventId || '';
    els.libraryEventId.value = state.activeLibraryEventId;
    var options = els.libraryListbox.querySelectorAll('.rail-list-option');
    options.forEach(function (node) {
      node.classList.toggle('is-active', node.getAttribute('data-event-id') === state.activeLibraryEventId);
      node.setAttribute('aria-selected', node.classList.contains('is-active') ? 'true' : 'false');
    });
  }

  function renderLibraryList(payload, bucket) {
    var rows = normalizeLibraryList(payload, bucket || state.activeLibraryBucket);
    els.libraryListbox.innerHTML = '';
    if (!rows.length) {
      var empty = document.createElement('div');
      empty.className = 'rail-list-option';
      empty.textContent = 'No events in this bucket.';
      els.libraryListbox.appendChild(empty);
      setLibraryActiveOption('');
      return;
    }

    rows.slice(0, 300).forEach(function (row) {
      var button = document.createElement('button');
      button.type = 'button';
      button.className = 'rail-list-option';
      button.setAttribute('role', 'option');
      button.setAttribute('data-event-id', row.id);

      var left = document.createElement('span');
      left.textContent = shortId(row.id);
      var right = document.createElement('span');
      right.textContent = row.bucket;
      right.style.color = 'var(--text-muted)';
      right.style.fontSize = '0.72rem';

      button.appendChild(left);
      button.appendChild(right);
      button.addEventListener('click', function () {
        setLibraryActiveOption(row.id);
      });

      els.libraryListbox.appendChild(button);
    });

    if (state.activeLibraryEventId && rows.some(function (row) { return row.id === state.activeLibraryEventId; })) {
      setLibraryActiveOption(state.activeLibraryEventId);
      return;
    }
    setLibraryActiveOption(rows[0].id);
  }

  function setLibraryBucket(bucket) {
    if (BUCKETS.indexOf(bucket) < 0) {
      bucket = 'all';
    }
    state.activeLibraryBucket = bucket;

    var buttons = els.libraryBucketNav.querySelectorAll('.bucket-btn');
    buttons.forEach(function (button) {
      var active = button.getAttribute('data-bucket') === bucket;
      button.classList.toggle('is-active', active);
      button.setAttribute('aria-selected', active ? 'true' : 'false');
    });

    saveUiPref('library_bucket', bucket).catch(function () {
      return;
    });
    runLibraryList(bucket).catch(function () {
      return;
    });
  }

  async function runLibraryList(bucket) {
    var effective = bucket || state.activeLibraryBucket;
    var arg = effective === 'all' ? '' : effective;
    var blob = await safeBackend('library-list', [arg], 'Failed to load library');
    var parsed = parseMaybeJson(blob);
    renderLibraryList(parsed, effective);
  }

  async function runLibraryMutation(command, label) {
    var id = String(els.libraryEventId.value || '').trim();
    if (!id) {
      toast('Event id is required.', 'bad');
      return;
    }
    await safeBackend(command, [id], 'Library update failed');
    toast(label + ' complete.', 'good');
    await runLibraryList(state.activeLibraryBucket);
  }

  async function runLibraryIngest() {
    var path = String(els.libraryAuthoredPath.value || '').trim();
    if (!path) {
      toast('Path is required.', 'bad');
      return;
    }
    var blob = await safeBackend('library-ingest-authored', [path], 'Failed to ingest authored events');
    writeLog(els.discoverLog, 'Ingest authored events', blob);
    await runLibraryList(state.activeLibraryBucket);
  }

  async function runLibraryReindex() {
    await safeBackend('library-reindex', [], 'Failed to reindex library');
    toast('Library reindexed.', 'good');
    await runLibraryList(state.activeLibraryBucket);
  }

  function renderRelayList(payload) {
    els.relayListbox.innerHTML = '';
    if (!payload || !payload.relays) {
      var empty = document.createElement('div');
      empty.className = 'rail-list-option';
      empty.textContent = 'No relay config yet.';
      els.relayListbox.appendChild(empty);
      return;
    }

    var relays = payload.relays;
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

    Object.keys(all).sort().forEach(function (url) {
      var row = document.createElement('button');
      row.type = 'button';
      row.className = 'rail-list-option';
      row.setAttribute('role', 'option');

      var left = document.createElement('span');
      left.textContent = shortId(url);
      left.title = url;

      var tags = [];
      if (all[url].home) {
        tags.push('H');
      }
      if (all[url].read) {
        tags.push('R');
      }
      if (all[url].write) {
        tags.push('W');
      }

      var right = document.createElement('span');
      right.textContent = tags.join(' ');
      right.style.color = 'var(--text-muted)';
      right.style.fontSize = '0.72rem';

      row.appendChild(left);
      row.appendChild(right);
      row.addEventListener('click', function () {
        els.networkRelayUrl.value = url;
        openSettings(true);
      });

      els.relayListbox.appendChild(row);
    });
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
      openCompose();
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

  function renderHomeFeed(parsed) {
    els.homeFeed.innerHTML = '';
    var events = [];
    if (Array.isArray(parsed)) {
      events = parsed;
    } else if (parsed && Array.isArray(parsed.events)) {
      events = parsed.events;
    } else if (parsed && Array.isArray(parsed.results)) {
      events = parsed.results;
    }

    if (!events.length) {
      els.homeFeed.textContent = 'No events returned.';
      return;
    }

    events.slice(0, 120).forEach(function (event) {
      var card = document.createElement('article');
      card.className = 'feed-item';

      var head = document.createElement('div');
      head.className = 'feed-head';

      var meta = document.createElement('div');
      meta.className = 'feed-meta';
      meta.textContent =
        'kind ' + String(event.kind || '?') +
        ' · ' + shortId(String(event.pubkey || '')) +
        ' · ' + shortId(String(event.id || ''));

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

      head.appendChild(meta);
      head.appendChild(menuWrap);

      var body = document.createElement('p');
      body.className = 'feed-body';
      body.textContent = String(event.content || '(empty)').slice(0, 2200);

      var actions = document.createElement('div');
      actions.className = 'feed-actions';

      var starBtn = document.createElement('button');
      starBtn.className = 'action';
      starBtn.type = 'button';
      starBtn.textContent = 'Star';
      starBtn.addEventListener('click', function () {
        if (!event.id) {
          return;
        }
        safeBackend('library-star', [String(event.id)], 'Failed to star event')
          .then(function () {
            toast('Starred event.', 'good');
            return runLibraryList(state.activeLibraryBucket);
          })
          .catch(function () {
            return;
          });
      });

      var saveBtn = document.createElement('button');
      saveBtn.className = 'action';
      saveBtn.type = 'button';
      saveBtn.textContent = 'Save';
      saveBtn.addEventListener('click', function () {
        if (!event.id) {
          return;
        }
        safeBackend('library-save', [String(event.id)], 'Failed to save event')
          .then(function () {
            toast('Saved event.', 'good');
            return runLibraryList(state.activeLibraryBucket);
          })
          .catch(function () {
            return;
          });
      });

      actions.appendChild(starBtn);
      actions.appendChild(saveBtn);

      card.appendChild(head);
      card.appendChild(body);
      card.appendChild(actions);
      els.homeFeed.appendChild(card);
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
    renderHomeFeed(parsed);
  }

  async function runDiscoverSearch() {
    var term = String(els.discoverTerm.value || '').trim();
    if (!term) {
      toast('Discover term is required.', 'bad');
      return;
    }
    var blob = await safeBackend('discover-search', [term, String(els.discoverLimit.value || '30').trim()], 'Search failed');
    writeLog(els.discoverLog, 'Discover search', blob);
  }

  async function runDiscoverCount() {
    var term = String(els.discoverTerm.value || '').trim();
    var blob = await safeBackend('discover-count', [term], 'Count failed');
    writeLog(els.discoverLog, 'Discover count', blob);
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
    writeLog(els.discoverLog, 'Filtered search', blob);
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
    els.peopleResults.innerHTML = '';
    if (!rows.length) {
      els.peopleResults.textContent = 'No ' + label + ' found.';
      return;
    }
    rows.forEach(function (entry) {
      var row = document.createElement('div');
      row.className = 'feed-item';
      var meta = document.createElement('div');
      meta.className = 'feed-meta';
      meta.textContent = shortId(entry);
      meta.title = entry;
      row.appendChild(meta);
      els.peopleResults.appendChild(row);
    });
  }

  async function runPeopleFollowing() {
    if (!state.activeProfilePubkey) {
      toast('Set an active profile first to load following.', 'bad');
      return;
    }

    var blob = await safeBackend(
      'timeline-fetch',
      [state.activeProfilePubkey, '3', '', '', '', '2', '1', ''],
      'Failed to load following'
    );
    var parsed = parseMaybeJson(blob);
    var events = parsed && Array.isArray(parsed.events) ? parsed.events : [];
    if (!events.length) {
      renderPeopleRows([], 'following');
      return;
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

    renderPeopleRows(uniq, 'following');
    toast('Following list loaded.', 'good');
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

  async function loadActiveProfile() {
    if (!state.bridge) {
      return;
    }
    try {
      var blob = await backend('profile-list', []);
      var parsed = parseMaybeJson(blob);
      if (!parsed || !Array.isArray(parsed.profiles)) {
        return;
      }
      var activeId = parsed.active_profile;
      if (!activeId) {
        return;
      }
      parsed.profiles.forEach(function (profile) {
        if (profile.id === activeId && profile.pubkey) {
          state.activeProfilePubkey = String(profile.pubkey);
        }
      });
      if (state.activeProfilePubkey && !els.peoplePubkey.value) {
        els.peoplePubkey.value = state.activeProfilePubkey;
      }
    } catch (_error) {
      return;
    }
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

    els.libraryBucketNav.addEventListener('click', function (event) {
      var button = event.target.closest('button.bucket-btn');
      if (!button) {
        return;
      }
      var bucket = button.getAttribute('data-bucket');
      setLibraryBucket(bucket);
    });

    els.libraryStar.addEventListener('click', function () {
      runLibraryMutation('library-star', 'Star').catch(function () {
        return;
      });
    });

    els.libraryUnstar.addEventListener('click', function () {
      runLibraryMutation('library-unstar', 'Unstar').catch(function () {
        return;
      });
    });

    els.librarySave.addEventListener('click', function () {
      runLibraryMutation('library-save', 'Save').catch(function () {
        return;
      });
    });

    els.libraryUnsave.addEventListener('click', function () {
      runLibraryMutation('library-unsave', 'Unsave').catch(function () {
        return;
      });
    });

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
  }

  function bindDrawers() {
    els.settingsOpen.addEventListener('click', function () {
      openSettings(false);
    });
    els.settingsOpenRelays.addEventListener('click', function () {
      openSettings(true);
    });
    els.settingsClose.addEventListener('click', closeSettings);

    els.composeOpen.addEventListener('click', openCompose);
    els.composeClose.addEventListener('click', closeCompose);

    els.deleteClose.addEventListener('click', closeDelete);

    [els.settingsBackdrop, els.composeBackdrop, els.deleteBackdrop].forEach(function (backdrop) {
      backdrop.addEventListener('click', function (event) {
        if (event.target !== backdrop) {
          return;
        }
        if (backdrop === els.settingsBackdrop) {
          closeSettings();
        }
        if (backdrop === els.composeBackdrop) {
          closeCompose();
        }
        if (backdrop === els.deleteBackdrop) {
          closeDelete();
        }
      });
    });

    document.addEventListener('keydown', function (event) {
      if (event.key !== 'Escape') {
        return;
      }
      closeOpenMenu();
      if (!els.deleteBackdrop.classList.contains('hidden')) {
        closeDelete();
        return;
      }
      if (!els.composeBackdrop.classList.contains('hidden')) {
        closeCompose();
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
    var interactive = document.querySelectorAll('button, input, select, textarea');
    interactive.forEach(function (node) {
      if (node === els.settingsOpen || node === els.themeQuick || node === els.themeSelect) {
        return;
      }
      node.disabled = true;
    });
    els.settingsOpen.disabled = false;
    els.themeQuick.disabled = false;
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
      if (!els.composeBackdrop.classList.contains('hidden')) {
        return;
      }
      if (!els.deleteBackdrop.classList.contains('hidden')) {
        return;
      }

      if (state.activeTab === 'home') {
        await runHomeFetch().catch(function () {
          return;
        });
      } else {
        await runDiscoverCount().catch(function () {
          return;
        });
      }
      await runLibraryList(state.activeLibraryBucket).catch(function () {
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
    setComposeType('note');

    document.addEventListener('click', function (event) {
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

    var prefBucket = String(prefs.library_bucket || 'all').trim();
    if (BUCKETS.indexOf(prefBucket) < 0) {
      prefBucket = 'all';
    }
    state.activeLibraryBucket = prefBucket;

    if (state.bridge) {
      await loadActiveProfile();
      await runRelayList().catch(function () {
        return;
      });
      await runLibraryList(state.activeLibraryBucket).catch(function () {
        return;
      });
      await runHomeFetch().catch(function () {
        return;
      });
    }

    setLibraryBucket(state.activeLibraryBucket);
    disableActionsWhenNoBridge();
    scheduleRefresh();
    revealBootUi();
  }

  init().catch(function (error) {
    console.error(error);
    toast(String((error && error.message) || error), 'bad');
    revealBootUi();
  });
})();
