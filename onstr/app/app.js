(function () {
  var state = {
    bridge: false,
    hostBootReadySent: false,
    activeTab: 'home',
    themes: [],
    theme: 'wizard',
    busy: false
  };

  var TAB_META = {
    home: {
      title: 'Home',
      subtitle: 'Timeline and thread drill-down.'
    },
    discover: {
      title: 'Discover',
      subtitle: 'Search and count relay content.'
    },
    compose: {
      title: 'Compose',
      subtitle: 'Create standard Nostr events with drafts and publish targets.'
    },
    library: {
      title: 'Library',
      subtitle: 'Manage liked, starred, saved, and commented content.'
    },
    network: {
      title: 'Network',
      subtitle: 'Relay sets, capability probes, and health checks.'
    }
  };

  var TAB_IDS = ['home', 'discover', 'compose', 'library', 'network'];

  var COMMAND_ALLOWLIST = Object.freeze({
    'get-ui-prefs': true,
    'set-ui-pref': true,
    'list-themes': true,
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
    'stonr-print-config': true,
    'stonr-mirror-status': true,
    'stonr-retention-status': true,
    'doctor': true
  });

  var els = {
    railStatus: document.getElementById('rail-status'),
    activeTitle: document.getElementById('active-title'),
    activeSubtitle: document.getElementById('active-subtitle'),
    bridgePill: document.getElementById('bridge-pill'),
    refreshActive: document.getElementById('refresh-active'),
    tabList: document.getElementById('primary-tabs'),
    toast: document.getElementById('toast'),

    themeLink: document.getElementById('theme-link'),
    themeSelect: document.getElementById('theme-select'),
    themeQuick: document.getElementById('theme-quick-select'),

    settingsOpen: document.getElementById('open-settings'),
    settingsClose: document.getElementById('close-settings'),
    settingsBackdrop: document.getElementById('drawer-backdrop'),
    bridgeCopy: document.getElementById('bridge-copy'),
    doctorOutput: document.getElementById('doctor-output'),
    settingsRunDoctor: document.getElementById('settings-run-doctor'),

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

    composeType: document.getElementById('compose-type'),
    composeForm: document.getElementById('compose-form'),
    composeNoteFields: document.getElementById('compose-note-fields'),
    composeReplyFields: document.getElementById('compose-reply-fields'),
    composeLongformFields: document.getElementById('compose-longform-fields'),
    composeFileFields: document.getElementById('compose-file-fields'),
    composeDeleteFields: document.getElementById('compose-delete-fields'),
    composeContent: document.getElementById('compose-content'),
    composeTags: document.getElementById('compose-tags'),
    composeReplyEvent: document.getElementById('compose-reply-event'),
    composeTitle: document.getElementById('compose-title'),
    composeIdentifier: document.getElementById('compose-identifier'),
    composeSummary: document.getElementById('compose-summary'),
    composeFileUrl: document.getElementById('compose-file-url'),
    composeFileHash: document.getElementById('compose-file-hash'),
    composeFileMime: document.getElementById('compose-file-mime'),
    composeFileSize: document.getElementById('compose-file-size'),
    composeDeleteId: document.getElementById('compose-delete-id'),
    composeDeleteReason: document.getElementById('compose-delete-reason'),
    composeDraft: document.getElementById('compose-draft'),
    composePassword: document.getElementById('compose-password'),
    composeProfileId: document.getElementById('compose-profile-id'),
    composeRelays: document.getElementById('compose-relays'),
    composePreview: document.getElementById('compose-preview'),
    composeList: document.getElementById('compose-list'),
    composeSign: document.getElementById('compose-sign'),
    composePublish: document.getElementById('compose-publish'),
    composeLog: document.getElementById('compose-log'),

    libraryForm: document.getElementById('library-form'),
    libraryBucket: document.getElementById('library-bucket'),
    libraryReindex: document.getElementById('library-reindex'),
    libraryEventId: document.getElementById('library-event-id'),
    libraryStar: document.getElementById('library-star'),
    libraryUnstar: document.getElementById('library-unstar'),
    librarySave: document.getElementById('library-save'),
    libraryUnsave: document.getElementById('library-unsave'),
    libraryIngestForm: document.getElementById('library-ingest-form'),
    libraryAuthoredPath: document.getElementById('library-authored-path'),
    libraryLog: document.getElementById('library-log'),

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
    networkLog: document.getElementById('network-log')
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

  function setBridgeUi() {
    if (state.bridge) {
      els.bridgePill.textContent = 'Bridge ready';
      els.bridgePill.className = 'pill good';
      els.railStatus.textContent = 'Bridge: connected';
      els.bridgeCopy.textContent = 'Desktop bridge connected. Backend persistence is active.';
    } else {
      els.bridgePill.textContent = 'Bridge unavailable';
      els.bridgePill.className = 'pill bad';
      els.railStatus.textContent = 'Bridge: unavailable';
      els.bridgeCopy.textContent = 'Hosted mode detected. Commands are disabled until this runs in the native host.';
    }
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

  async function safeBackend(command, args, onError) {
    try {
      return await backend(command, args);
    } catch (error) {
      var message = String((error && error.message) || onError || 'command failed');
      toast(message, 'bad');
      throw error;
    }
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

    var meta = TAB_META[tabId];
    els.activeTitle.textContent = meta.title;
    els.activeSubtitle.textContent = meta.subtitle;
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

  function openSettings() {
    els.settingsBackdrop.classList.remove('hidden');
    els.settingsBackdrop.setAttribute('aria-hidden', 'false');
    els.themeSelect.focus();
  }

  function closeSettings() {
    els.settingsBackdrop.classList.add('hidden');
    els.settingsBackdrop.setAttribute('aria-hidden', 'true');
    els.settingsOpen.focus();
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
    var type = els.composeType.value;
    els.composeNoteFields.classList.toggle('hidden', type !== 'note');
    els.composeReplyFields.classList.toggle('hidden', type !== 'reply');
    els.composeLongformFields.classList.toggle('hidden', type !== 'longform');
    els.composeFileFields.classList.toggle('hidden', type !== 'file-metadata');
    els.composeDeleteFields.classList.toggle('hidden', type !== 'delete');
    els.composeContent.parentElement.classList.toggle('hidden', type === 'file-metadata');
  }

  function eventTargetId() {
    return String(els.libraryEventId.value || '').trim();
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

    events.slice(0, 80).forEach(function (event) {
      var card = document.createElement('article');
      card.className = 'feed-item';

      var meta = document.createElement('div');
      meta.className = 'feed-meta';
      meta.textContent =
        'kind ' + String(event.kind || '?') +
        ' · ' + String(event.pubkey || '').slice(0, 12) +
        ' · ' + String(event.id || '').slice(0, 12);

      var body = document.createElement('p');
      body.className = 'feed-body';
      body.textContent = String(event.content || '(empty)').slice(0, 700);

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
          .then(function (blob) {
            writeLog(els.libraryLog, 'Starred event', blob);
            toast('Starred event.', 'good');
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
          .then(function (blob) {
            writeLog(els.libraryLog, 'Saved event', blob);
            toast('Saved event.', 'good');
          })
          .catch(function () {
            return;
          });
      });

      var replyBtn = document.createElement('button');
      replyBtn.className = 'action';
      replyBtn.type = 'button';
      replyBtn.textContent = 'Reply Draft';
      replyBtn.addEventListener('click', function () {
        if (!event.id) {
          return;
        }
        els.composeType.value = 'reply';
        els.composeReplyEvent.value = String(event.id);
        els.composeContent.value = '';
        setComposeTypeUi();
        setActiveTab('compose', true);
      });

      actions.appendChild(starBtn);
      actions.appendChild(saveBtn);
      actions.appendChild(replyBtn);

      card.appendChild(meta);
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
      els.homeIncludeRemotes.checked ? '1' : '0'
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
      '1'
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

  function composeArgsForType(type) {
    var draft = String(els.composeDraft.value || '').trim();
    if (!draft) {
      throw new Error('Draft name is required.');
    }

    if (type === 'note') {
      var content = String(els.composeContent.value || '').trim();
      if (!content) {
        throw new Error('Note content is required.');
      }
      return ['compose-note', [content, String(els.composeTags.value || '').trim(), draft]];
    }

    if (type === 'reply') {
      var replyContent = String(els.composeContent.value || '').trim();
      var eventId = String(els.composeReplyEvent.value || '').trim();
      if (!replyContent || !eventId) {
        throw new Error('Reply content and event id are required.');
      }
      return ['compose-reply', [replyContent, eventId, draft]];
    }

    if (type === 'longform') {
      var title = String(els.composeTitle.value || '').trim();
      var identifier = String(els.composeIdentifier.value || '').trim();
      var longformContent = String(els.composeContent.value || '').trim();
      if (!title || !identifier || !longformContent) {
        throw new Error('Title, identifier, and content are required for long-form drafts.');
      }
      return ['compose-longform', [title, identifier, longformContent, String(els.composeSummary.value || '').trim(), draft]];
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

    if (type === 'delete') {
      var deleteId = String(els.composeDeleteId.value || '').trim();
      if (!deleteId) {
        throw new Error('Delete event id is required.');
      }
      return ['compose-delete', [deleteId, String(els.composeDeleteReason.value || '').trim(), draft]];
    }

    throw new Error('Unsupported compose type: ' + type);
  }

  async function runComposeDraftCreate() {
    var type = String(els.composeType.value || 'note');
    var command;
    var args;
    var tuple = composeArgsForType(type);
    command = tuple[0];
    args = tuple[1];
    var blob = await safeBackend(command, args, 'Failed to create draft');
    writeLog(els.composeLog, 'Draft created', blob);
    toast('Draft created.', 'good');
  }

  async function runComposePreview() {
    var draft = String(els.composeDraft.value || '').trim();
    if (!draft) {
      toast('Draft name is required.', 'bad');
      return;
    }
    var blob = await safeBackend('compose-preview', [draft], 'Failed to preview draft');
    writeLog(els.composeLog, 'Draft preview', blob);
  }

  async function runComposeList() {
    var blob = await safeBackend('compose-list-drafts', [], 'Failed to list drafts');
    writeLog(els.composeLog, 'Draft list', blob);
  }

  async function runComposeSign() {
    var draft = String(els.composeDraft.value || '').trim();
    var password = String(els.composePassword.value || '');
    var profile = String(els.composeProfileId.value || '').trim();
    if (!draft || !password) {
      toast('Draft and password are required to sign.', 'bad');
      return;
    }
    var args = [draft, password, profile];
    var blob = await safeBackend('compose-sign-draft', args, 'Failed to sign draft');
    writeLog(els.composeLog, 'Draft signed', blob);
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
    writeLog(els.composeLog, 'Publish result', blob);
  }

  async function runLibraryList() {
    var bucket = String(els.libraryBucket.value || 'all');
    var arg = bucket === 'all' ? '' : bucket;
    var blob = await safeBackend('library-list', [arg], 'Failed to load library');
    writeLog(els.libraryLog, 'Library list', blob);
  }

  async function runLibraryMutation(command, label) {
    var id = eventTargetId();
    if (!id) {
      toast('Event id is required.', 'bad');
      return;
    }
    var blob = await safeBackend(command, [id], 'Library update failed');
    writeLog(els.libraryLog, label, blob);
  }

  async function runLibraryIngest() {
    var path = String(els.libraryAuthoredPath.value || '').trim();
    if (!path) {
      toast('Path is required.', 'bad');
      return;
    }
    var blob = await safeBackend('library-ingest-authored', [path], 'Failed to ingest authored events');
    writeLog(els.libraryLog, 'Ingest authored events', blob);
  }

  async function runLibraryReindex() {
    var blob = await safeBackend('library-reindex', [], 'Failed to reindex library');
    writeLog(els.libraryLog, 'Library reindex', blob);
  }

  async function runRelayAction(command, args, label) {
    var blob = await safeBackend(command, args, label + ' failed');
    writeLog(els.networkLog, label, blob);
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

    els.composeType.addEventListener('change', setComposeTypeUi);

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

    els.libraryForm.addEventListener('submit', function (event) {
      event.preventDefault();
      runLibraryList().catch(function () {
        return;
      });
    });

    els.libraryStar.addEventListener('click', function () {
      runLibraryMutation('library-star', 'Star event').catch(function () {
        return;
      });
    });

    els.libraryUnstar.addEventListener('click', function () {
      runLibraryMutation('library-unstar', 'Unstar event').catch(function () {
        return;
      });
    });

    els.librarySave.addEventListener('click', function () {
      runLibraryMutation('library-save', 'Save event').catch(function () {
        return;
      });
    });

    els.libraryUnsave.addEventListener('click', function () {
      runLibraryMutation('library-unsave', 'Unsave event').catch(function () {
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
      runRelayAction('relay-list', [], 'Relay list').catch(function () {
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

  function bindSettingsDrawer() {
    els.settingsOpen.addEventListener('click', openSettings);
    els.settingsClose.addEventListener('click', closeSettings);
    els.settingsBackdrop.addEventListener('click', function (event) {
      if (event.target === els.settingsBackdrop) {
        closeSettings();
      }
    });
    document.addEventListener('keydown', function (event) {
      if (event.key !== 'Escape') {
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
    var interactive = document.querySelectorAll('button.action, button.icon-btn, input, select, textarea');
    interactive.forEach(function (node) {
      if (node === els.settingsOpen || node === els.themeQuick || node === els.themeSelect) {
        return;
      }
      if (node.classList.contains('tab')) {
        return;
      }
      node.disabled = true;
    });
    els.settingsOpen.disabled = false;
    els.themeQuick.disabled = false;
    els.themeSelect.disabled = false;
  }

  async function refreshActiveTab() {
    if (state.activeTab === 'home') {
      return runHomeFetch();
    }
    if (state.activeTab === 'discover') {
      return runDiscoverCount();
    }
    if (state.activeTab === 'compose') {
      return runComposeList();
    }
    if (state.activeTab === 'library') {
      return runLibraryList();
    }
    return runRelayAction('relay-list', [], 'Relay list');
  }

  async function loadPreferences() {
    if (!state.bridge) {
      return {};
    }
    try {
      var blob = await backend('get-ui-prefs', []);
      return parseKv(blob);
    } catch (error) {
      console.error(error);
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
    } catch (error) {
      console.error(error);
      setThemeOptions(['wizard']);
    }
  }

  async function init() {
    state.bridge = bridgeAvailable();
    setBridgeUi();
    bindTabSemantics();
    bindSettingsDrawer();
    bindForms();
    bindThemeControls();
    setComposeTypeUi();

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

    els.refreshActive.addEventListener('click', function () {
      refreshActiveTab().catch(function () {
        return;
      });
    });

    if (state.bridge) {
      await runRelayAction('relay-list', [], 'Relay list').catch(function () {
        return;
      });
    }

    disableActionsWhenNoBridge();
    revealBootUi();
  }

  init().catch(function (error) {
    console.error(error);
    toast(String((error && error.message) || error), 'bad');
    revealBootUi();
  });
})();
