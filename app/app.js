(function () {
  var state = {
    appDir: inferAppDir(),
    bridge: false,
    hostBootReadySent: false,
    refreshTimer: null,
    refreshInFlight: false,
    refreshQueued: false,
    envPathTimer: null,
    envPath: '',
    envValues: {},
    activeSection: 'relay',
    config: null,
    status: null,
    doctorKv: {},
    doctor: '',
    log: '',
    toastTimer: null,
    saveStatusTimer: null,
    saveStatusHideTimer: null,
    saveStatusTicket: 0,
    saveStatusShownAt: 0,
    relayBusyAction: '',
    retentionBusy: false,
    railWidth: 224,
    dragPointerId: null,
    fieldSaveTimers: {},
    fieldSaveTargets: {},
    pendingFieldSavePromises: {},
    configEditSeq: 0,
    saveQueue: Promise.resolve(),
    nextSaveTicket: 0,
    appliedSaveTicket: 0,
    fieldNodes: {},
    initialDependencySyncDone: false,
    listSaveTimers: {},
    eventsSearchTimer: null,
    eventsSearch: '',
    eventsLoading: false,
    eventsTotalLoading: false,
    eventsLoadedOnce: false,
    eventsStatsLoadedAt: 0,
    eventsStatsPromise: null,
    eventsError: '',
    events: [],
    eventsTotal: 0,
    eventsBytes: 0,
    diagnosticsLoading: false,
    diagnosticsLoadedOnce: false,
    diagnosticsMirror: [],
    diagnosticsRetention: null,
    diagnosticsError: '',
    doctorRefreshedAt: 0,
    backgroundMode: false,
    menuBarIcon: false,
    hostStatusItemRelayRunning: null,
    startupServiceEnabled: false,
    startupServiceManager: 'none',
    startupServiceBusy: false,
    startupServicePendingAction: '',
    autoStartRelayOnOpen: false,
    autoStartRelayChecked: false,
    moderationLists: {
      'pubkeys-allow': '',
      'pubkeys-deny': '',
      'file-hashes-deny': ''
    }
  };
  var DOCTOR_REFRESH_INTERVAL_MS = 30000;
  var LIVE_REFRESH_INTERVAL_MS = 4000;

  var relayLoginDependsOn = ['ENABLE_NIP42'];

  var sections = [
    {
      id: 'events',
      label: 'Events',
      eyebrow: 'Events',
      title: 'Stored Events',
      detail: 'Browse recent stored events and filter them by keyword.',
      fields: [],
      custom: 'events'
    },
    {
      id: 'general',
      label: 'General',
      eyebrow: 'General',
      title: 'General Settings',
      detail: 'Desktop app behavior and window lifecycle.',
      fields: [],
      custom: 'general'
    },
    {
      id: 'relay',
      label: 'Relay',
      eyebrow: 'Core',
      title: 'Relay Behavior',
      detail: 'Core relay identity, query/publish behavior, and live delivery.',
      fields: [
        groupField('Identity'),
        textField('RELAY_NAME', 'policy.relay_name', 'Relay name', '', null, null, 'Name shown to clients when they browse or save this relay.'),
        textField('RELAY_DESCRIPTION', 'policy.relay_description', 'Relay description', '', null, null, 'Short summary shown beside the relay name in client UIs.'),
        groupField('Core Behavior'),
        boolField('ENABLE_QUERY', 'policy.enable_query', 'Read access (recommended)', 'Clients can read stored events.', null, 'Allow clients to read stored events with REQ filters.'),
        boolField('ENABLE_PUBLISH', 'policy.enable_publish', 'Write access (recommended)', 'Clients can publish events.', null, 'Allow clients to publish new events to this relay.'),
        boolField('ENABLE_LIVE_SUBSCRIPTIONS', 'policy.enable_live_subscriptions', 'Live updates (recommended)', 'Subscriptions stay open for new matching events.', null, 'Keep subscriptions open and push new matching events as they arrive.'),
        boolField('VERIFY_SIG', 'verify_sig', 'Signature checks (recommended)', 'Invalid signatures are rejected.', null, 'Reject events whose signatures do not match their claimed author.'),
        boolField('ENABLE_MIRRORING', 'policy.enable_mirroring', 'Import from relays', 'Imports events from configured upstream relays.', null, 'Pull events from upstream relays into local storage.')
      ]
    },
    {
      id: 'policies',
      label: 'NIP policies',
      eyebrow: 'NIPs',
      title: 'NIP Policies',
      detail: 'Turn each NIP on or off, then tune the extra behavior that NIP unlocks.',
      fields: [
        withFieldUi(boolField(
          'SUPPORT_NIP11',
          'policy.support_nip11',
          'Relay profile document',
          '',
          null,
          'Publishes relay metadata and capability discovery for clients.'
        ), { nipMaster: true }),
        withFieldUi(boolField(
          'ENABLE_NIP11',
          'policy.enable_nip11',
          'Publish relay profile endpoint',
          '',
          null,
          {
            tooltip: '',
            viewCommand: 'open-relay-profile',
            viewLabel: 'View',
            viewHint: 'Open the live relay profile document.'
          }
        ), { nipChild: true, hideNipPill: true }),
        withFieldUi(boolField(
          'SUPPORT_NIP12',
          'policy.support_nip12',
          'Tag-based event filters',
          '',
          null,
          ''
        ), { nipMaster: true }),
        withFieldUi(boolField(
          'ENABLE_TAG_QUERIES',
          'policy.enable_tag_queries',
          'Allow reads filtered by tags',
          'Lets clients filter reads by tags like `#e`, `#p`, and `#t`.',
          null,
          ''
        ), { nipChild: true, hideNipPill: true }),
        withFieldUi(boolField(
          'SUPPORT_NIP45',
          'policy.support_nip45',
          'COUNT query support',
          '',
          null,
          'Enables count-only queries without returning full event payloads.'
        ), { nipMaster: true }),
        withFieldUi(boolField(
          'ENABLE_COUNT',
          'policy.enable_count',
          'Allow COUNT queries',
          'Lets clients ask how many events match a filter without downloading events.',
          null,
          ''
        ), { nipChild: true, hideNipPill: true }),
        withFieldUi(boolField(
          'SUPPORT_NIP50',
          'policy.support_nip50',
          'Text search',
          '',
          null,
          ''
        ), { nipMaster: true }),
        withFieldUi(boolField(
          'ENABLE_SEARCH',
          'policy.enable_search',
          'Allow relay text search',
          '',
          null,
          ''
        ), { nipChild: true, hideNipPill: true }),
        withFieldUi(boolField(
          'SUPPORT_NIP09',
          'policy.support_nip09',
          'Deletion events',
          '',
          null,
          ''
        ), { nipMaster: true }),
        withFieldUi(boolField(
          'SUPPORT_NIP40',
          'policy.support_nip40',
          'Expiration handling',
          '',
          null,
          ''
        ), { nipMaster: true }),
        withFieldUi(boolField(
          'SUPPORT_NIP42',
          'policy.support_nip42',
          'Relay login',
          '',
          null,
          ''
        ), { nipMaster: true }),
        withFieldUi(boolField(
          'SUPPORT_NIP94',
          'policy.support_nip94',
          'File metadata events',
          '',
          null,
          ''
        ), { nipMaster: true }),
        withFieldUi(boolField(
          'SUPPORT_NIP96',
          'policy.support_nip96',
          'Compatibility file API',
          '',
          null,
          'Enables the legacy `/files` compatibility API surface.'
        ), { nipMaster: true }),
        withFieldUi(boolField(
          'SUPPORT_NIP98',
          'policy.support_nip98',
          'HTTP request authentication',
          '',
          null,
          'Accepts and verifies signed HTTP authentication headers.'
        ), { nipMaster: true }),
        withFieldUi(boolField(
          'SUPPORT_NIP_B7',
          'policy.support_nip_b7',
          'Blossom blob API',
          '',
          null,
          'Enables Blossom routes for hash-addressed blob storage.'
        ), { nipMaster: true })
      ]
    },
    {
      id: 'pinned',
      label: 'Pinned',
      eyebrow: 'Pinned',
      title: 'Pinned Content',
      detail: 'Pin owner/followed content and choose whether to store only pinned-author site traffic.',
      fields: [
        groupField('Pinned Author Scope'),
        radioField('MIRROR_MODE', 'mirror_mode', 'Pinned author ingest scope', [
          { value: 'broad', label: 'Store normal relay traffic' },
          { value: 'site', label: 'Only store owner-author site traffic' }
        ], null, 'Strict mode uses Owner authors below as the allowed site-author list.'),
        withFieldUi(boolField('MIRROR_SITE_INCLUDE_COMMENTS', 'mirror_site_include_comments', 'Include comments for owner-author site posts', '', [{ envKey: 'MIRROR_MODE', equals: 'site' }], 'Also import kind 1 comments that reference owner-author site posts by `a` tag.'), { collapseWhenUnavailable: true }),
        groupField('Pinned Sets'),
        textareaField('OWNER_PUBKEYS', 'owner_pubkeys', 'Owner authors (used by strict site mode, privileged + always kept)', 'One pubkey per line', formatLineList, null, 'Owner pubkeys define strict site-author scope, bypass write auth/rate limits, and are always retained.'),
        textareaField('FOLLOW_PUBKEYS', 'follow_pubkeys', 'Follow authors (mirror + always kept)', 'One pubkey per line', formatLineList, null, 'These pubkeys are added to mirror author filters and retained from eviction.'),
        textareaField('PIN_EVENT_IDS', 'pin_event_ids', 'Pin specific event IDs', 'One event ID per line', formatLineList, null, 'Exact events that should never be removed by retention.'),
        boolField('PIN_PROTECT_FROM_DELETES', 'pin_protect_from_deletes', 'Ignore delete events against pinned content', '', null, 'Keep owner/follow/pinned content visible even when NIP-09 delete events target it.')
      ]
    },
    {
      id: 'network',
      label: 'Network',
      eyebrow: 'Network',
      title: 'Network And Import',
      detail: 'Bind addresses, upstream feeds, and import filter state.',
      fields: [
        groupField('Local Paths And Ports'),
        browseTextField('STORE_ROOT', 'store_root', 'Data folder', '', null, null, 'Root folder for events, blobs, indexes, logs, and runtime files.'),
        textField('BIND_HTTP', 'bind_http', 'HTTP address', 'Example: 127.0.0.1:7777', null, null, 'Local host and port for relay info, file APIs, and other HTTP routes.'),
        textField('BIND_WS', 'bind_ws', 'WebSocket address', 'Example: 127.0.0.1:7778', null, null, 'Local host and port where Nostr clients connect for relay traffic.'),
        textField('PUBLIC_RELAY_URL', 'policy.public_relay_url', 'Public relay URL', 'Example: wss://relay.example.com', null, null, 'Public relay URL clients should use. Provision TLS with centralized Wizardry/Headquarters SSL setup, then set this URL.'),
        groupField('Upstream Relays'),
        textareaField('RELAYS_UPSTREAM', 'relays_upstream', 'Source relays', 'One relay URL per line', formatLineList, null, 'Relay URLs to import events from, one per line.'),
        textField('TOR_SOCKS', 'tor_socks', 'SOCKS proxy', 'Optional. Example: 127.0.0.1:9050', null, null, 'Optional SOCKS proxy for outbound relay traffic, including Tor.'),
        groupField('Mirror Filters'),
        textField('FILTER_AUTHORS', 'filter_authors', 'Authors to import', 'Comma-separated pubkeys', formatList, null, 'If set, only import events from these author pubkeys.'),
        textField('FILTER_KINDS', 'filter_kinds', 'Kinds to import', 'Comma-separated kind numbers, for example: 1,7,30023', formatNumberList, null, 'If set, only import these event kinds.'),
        textField('FILTER_TAG_A', 'filter_tag_a', 'Addresses to import', 'Comma-separated `kind:pubkey:d` addresses', formatList, null, 'If set, only import events whose `#a` tags match these addresses.'),
        textField('FILTER_TAG_T', 'filter_tag_t', 'Topics to import', 'Comma-separated topic tags', formatList, null, 'If set, only import events whose `#t` tags match these topics.'),
        textField('FILTER_SINCE_MODE', 'filter_since_mode', 'Import start point', 'Use `cursor` or `fixed:<unix time>`', formatSinceMode, null, 'Use `cursor` to resume where the importer left off, or `fixed:<unix time>` to start from a fixed timestamp.'),
        groupField('Kind Policy'),
        textField('ALLOW_KINDS', 'policy.allowed_kinds', 'Allowed kinds', 'Comma-separated kind numbers', formatNumberList, null, 'If set, only these event kinds are accepted.'),
        textField('DENY_KINDS', 'policy.blocked_kinds', 'Blocked kinds', 'Comma-separated kind numbers', formatNumberList, null, 'These event kinds are always rejected.')
      ]
    },
    {
      id: 'auth',
      label: 'Auth',
      eyebrow: 'Auth',
      title: 'Authentication',
      detail: 'Relay login and auth requirements for reads, writes, and file APIs.',
      fields: [
        groupField('Relay Login'),
        boolField('ENABLE_NIP42', 'policy.enable_nip42', 'Enable relay login flow', '', null, 'When ON, relay login is active for clients. When OFF, no NIP-42 login session can be established.'),
        boolField('REQUIRE_AUTH_FOR_QUERY', 'policy.require_auth_for_query', 'Require login for reads', '', relayLoginDependsOn, 'When ON, unauthenticated clients cannot read events. When OFF, reads are allowed without login.'),
        boolField('REQUIRE_AUTH_FOR_COUNT', 'policy.require_auth_for_count', 'Require login for counts', '', relayLoginDependsOn, 'When ON, unauthenticated clients cannot run COUNT queries. When OFF, COUNT requests do not require login.'),
        boolField('REQUIRE_AUTH_FOR_PUBLISH', 'policy.require_auth_for_publish', 'Require login for writes', '', relayLoginDependsOn, 'When ON, publishing requires an authenticated login. When OFF, clients can publish without logging in.'),
        boolField('AUTH_MUST_MATCH_EVENT_PUBKEY', 'policy.auth_must_match_event_pubkey', 'Require writer pubkey to match login', '', relayLoginDependsOn, 'When ON, logged-in pubkey must match the event author pubkey. When OFF, logged-in clients may publish for a different pubkey.'),
        numberField('AUTH_MAX_AGE_SECS', 'policy.auth_max_age_secs', 'Maximum login proof age', '', null, relayLoginDependsOn, 'Maximum age, in seconds, for an AUTH proof. Older proofs are rejected.'),
        groupField('HTTP And Blob Auth'),
        boolField('REQUIRE_NIP98_AUTH', 'policy.require_nip98_auth', 'Require auth for compatibility API', '', null, 'When ON, compatibility uploads and deletes require valid NIP-98 auth. When OFF, unsigned compatibility API requests are allowed.'),
        boolField('REQUIRE_BLOSSOM_AUTH', 'policy.require_blossom_auth', 'Require login for Blossom writes', '', ['ENABLE_BLOSSOM'], 'When ON, Blossom uploads, deletes, mirrors, and owner routes require authentication. When OFF, those writes do not require login.'),
        boolField('REQUIRE_BLOSSOM_GET_AUTH', 'policy.require_blossom_get_auth', 'Require login for Blossom downloads', '', ['ENABLE_BLOSSOM'], 'When ON, blob download-by-hash requires authentication. When OFF, hash downloads are public.')
      ]
    },
    {
      id: 'files',
      label: 'Files',
      eyebrow: 'Files',
      title: 'Files And Blob APIs',
      detail: 'Disk-backed file features, public URLs, and retention behavior.',
        fields: [
        groupField('Feature Switches'),
        boolField('ENABLE_FILE_METADATA', 'policy.enable_file_metadata', 'Store file metadata records', '', null, 'When ON, file metadata events such as kind 1063 are stored and served. When OFF, metadata records are not stored.'),
        boolField('ENABLE_BLOSSOM', 'policy.enable_blossom', 'Enable Blossom API routes', '', null, 'When ON, Blossom routes accept and serve blobs. When OFF, Blossom endpoints are not served.'),
        boolField('ENABLE_FILE_API', 'policy.enable_file_api', 'Enable compatibility API routes', '', null, 'When ON, `/files` compatibility routes are active. When OFF, those routes are not served.'),
        boolField('ENABLE_BLOSSOM_LIST', 'policy.enable_blossom_list', 'Allow owner blob listings', '', ['ENABLE_BLOSSOM'], 'When ON, owners can list blobs they own. When OFF, owner list endpoints are disabled.'),
        boolField('ENABLE_BLOSSOM_MIRROR', 'policy.enable_blossom_mirror', 'Allow remote blob import', '', ['ENABLE_BLOSSOM'], 'When ON, the relay may mirror remote blobs into local storage. When OFF, remote mirror import is blocked.'),
        groupField('Advertised URLs'),
        textField('FILE_API_URL', 'policy.file_api_url', 'Compatibility API URL', 'Leave blank to use the local default', null, ['ENABLE_FILE_API'], 'Public URL clients should use for the `/files` API.'),
        textField('BLOSSOM_PUBLIC_URL', 'policy.blossom_public_url', 'Blossom public URL', 'Leave blank to use the local default', null, ['ENABLE_BLOSSOM'], 'Public URL clients should use for Blossom routes.'),
        groupField('Storage Rules'),
        numberField('FILE_MAX_BYTES', 'policy.file_max_bytes', 'Max upload size', '', null, null, 'Largest file upload this relay will accept.'),
        textField('FILE_ALLOW_MIME', 'policy.file_allowed_mime', 'Allowed file types', 'Comma-separated MIME patterns, for example: image/*,application/pdf', formatList, null, 'If set, only these MIME patterns are allowed, for example `image/*` or `application/pdf`.'),
        textField('FILE_DENY_MIME', 'policy.file_blocked_mime', 'Blocked file types', 'Comma-separated MIME patterns', formatList, null, 'Always reject these MIME patterns, for example `video/*` or `application/x-msdownload`.'),
        selectField('FILE_KEEP_MODE', 'policy.file_keep_mode', 'Blob retention', '', [
          { value: 'referenced', label: 'Referenced only' },
          { value: 'all', label: 'Keep all blobs' }
        ], formatKeepMode, null, 'Choose whether unreferenced blobs are pruned or kept until manual cleanup.')
      ]
    },
    {
      id: 'limits',
      label: 'Limits',
      eyebrow: 'Limits',
      title: 'Limits And Quotas',
      detail: 'Result caps, created_at bounds, and file-backed rate limits.',
      fields: [
        groupField('Store Retention'),
        withExplicitSave(numberField('MAX_STORED_EVENT_BYTES', 'policy.max_stored_event_bytes', 'Max stored event size', '', null, null, 'When stored event files exceed this total size, it deletes the oldest stored events first. Leave blank for unlimited.'), 'store-retention'),
        withExplicitSave(numberField('MAX_STORED_EVENTS', 'policy.max_stored_events', 'Max stored events', '', null, null, 'When this relay stores more events than this, it deletes the oldest stored events first. Leave blank for unlimited.'), 'store-retention'),
        retentionApplyField(),
        noteField('When either limit is reached, the oldest stored events are deleted first and the newest events are kept.'),
        groupField('Rate Limits'),
        numberField('RATE_LIMIT_WINDOW_SECS', 'policy.rate_limit_window_secs', 'Rate-limit window', '', null, null, 'Time window used for the read, write, count, and upload limits below.'),
        numberField('MAX_QUERIES_PER_WINDOW', 'policy.max_queries_per_window', 'Reads per window', '', null, null, 'How many read queries one actor can make per rate-limit window.'),
        numberField('MAX_COUNTS_PER_WINDOW', 'policy.max_counts_per_window', 'Counts per window', '', null, null, 'How many COUNT requests one actor can make per rate-limit window.'),
        numberField('MAX_PUBLISHES_PER_WINDOW', 'policy.max_publishes_per_window', 'Writes per window', '', null, null, 'How many events one actor can publish per rate-limit window.'),
        numberField('MAX_UPLOADS_PER_WINDOW', 'policy.max_uploads_per_window', 'Uploads per window', '', null, null, 'How many file uploads one actor can start per rate-limit window.'),
        numberField('MAX_UPLOAD_BYTES_PER_WINDOW', 'policy.max_upload_bytes_per_window', 'Upload bytes per window', '', null, null, 'Total upload volume one actor can send per rate-limit window.'),
        groupField('Event Safety'),
        numberField('MAX_LIMIT', 'policy.max_limit', 'Max results per read', '', null, null, 'Maximum number of events returned from one read query.'),
        numberField('MAX_EVENT_BYTES', 'policy.max_event_bytes', 'Max event size', '', null, null, 'Largest serialized event this relay will accept.'),
        numberField('MAX_EVENT_AGE_SECS', 'policy.max_event_age_secs', 'Oldest accepted event', '', null, null, 'Reject events older than this age.'),
        numberField('MAX_EVENT_FUTURE_SECS', 'policy.max_event_future_secs', 'Future clock skew', '', null, null, 'Reject events dated too far into the future.'),
        groupField('Blob Quota'),
        numberField('MAX_BLOB_BYTES_PER_PUBKEY', 'policy.max_blob_bytes_per_pubkey', 'Blob quota per pubkey', '', null, null, 'Maximum stored blob space one pubkey may own on this relay.')
      ]
    },
    {
      id: 'moderation',
      label: 'Moderation',
      eyebrow: 'Moderation',
      title: 'Moderation And Curation',
      detail: 'Mix static env rules with live file-backed allow, deny, and hash lists.',
      fields: [
        groupField('Static Policy'),
        boolField('FILTER_PRIVATE_MESSAGES', 'policy.filter_private_messages', 'Filter Private Messages', '', null, 'Reject encrypted private-message kinds such as 4, 13, 14, 15, and 1059 before storing, mirroring, or forwarding them.'),
        textareaField('ALLOW_PUBKEYS', 'policy.allowed_pubkeys', 'Allowed authors', 'One pubkey per line', formatLineList, null, 'If set, only these pubkeys may publish or mirror into this relay.'),
        textareaField('DENY_PUBKEYS', 'policy.blocked_pubkeys', 'Blocked authors', 'One pubkey per line', formatLineList, null, 'These pubkeys are always rejected, even if they would otherwise be allowed.')
      ],
      custom: 'moderation'
    },
    {
      id: 'diagnostics',
      label: 'Diagnostics',
      eyebrow: 'Diagnostics',
      title: 'Diagnostics',
      detail: 'Read the relay log, inspect backend status, and run verification.',
      fields: [],
      custom: 'diagnostics'
    }
  ];

  var nipSummaries = {
    'NIP-09': 'Controls whether delete requests can remove previously stored events.',
    'NIP-11': 'Controls whether clients can read this relay\'s profile and capability document.',
    'NIP-12': 'Controls whether reads can use tag filters like `#e`, `#p`, and `#t`.',
    'NIP-40': 'Controls whether expiration tags can hide or reject expired events.',
    'NIP-42': 'Controls login/authentication flows for protected relay actions.',
    'NIP-45': 'Controls whether clients can request counts without downloading events.',
    'NIP-50': 'Controls whether relay-side text search is available.',
    'NIP-94': 'Controls acceptance and serving of file metadata events.',
    'NIP-96': 'Controls the legacy `/files` compatibility upload API.',
    'NIP-98': 'Controls signed HTTP auth checks for compatibility API requests.',
    'NIP-B7': 'Controls Blossom blob API routes for upload, read, and delete.'
  };

  var nipBriefSummaries = {
    'NIP-09': 'Delete requests',
    'NIP-11': 'Relay info document',
    'NIP-12': 'Tag-based filters',
    'NIP-40': 'Expiration handling',
    'NIP-42': 'Relay login',
    'NIP-45': 'Match counts without events',
    'NIP-50': 'Full-text search',
    'NIP-94': 'File metadata events',
    'NIP-96': 'Legacy file API',
    'NIP-98': 'Signed HTTP auth',
    'NIP-B7': 'Blossom blob API'
  };

  var nipUrls = {
    'NIP-09': 'https://github.com/nostr-protocol/nips/blob/master/09.md',
    'NIP-11': 'https://github.com/nostr-protocol/nips/blob/master/11.md',
    'NIP-12': 'https://github.com/nostr-protocol/nips/blob/master/12.md',
    'NIP-40': 'https://github.com/nostr-protocol/nips/blob/master/40.md',
    'NIP-42': 'https://github.com/nostr-protocol/nips/blob/master/42.md',
    'NIP-45': 'https://github.com/nostr-protocol/nips/blob/master/45.md',
    'NIP-50': 'https://github.com/nostr-protocol/nips/blob/master/50.md',
    'NIP-94': 'https://github.com/nostr-protocol/nips/blob/master/94.md',
    'NIP-96': 'https://github.com/nostr-protocol/nips/blob/master/96.md',
    'NIP-98': 'https://github.com/nostr-protocol/nips/blob/master/98.md',
    'NIP-B7': 'https://github.com/nostr-protocol/nips/blob/master/B7.md'
  };

  var defaultUpstreamRelays = [
    'wss://relay.damus.io',
    'wss://nos.lol',
    'wss://purplepag.es',
    'wss://relay.primal.net',
    'wss://relay.nostr.band',
    'wss://relay.snort.social',
    'wss://relay.nsec.app'
  ];

  var nipMasterByField = {
    ENABLE_NIP11: ['SUPPORT_NIP11'],
    ENABLE_COUNT: ['SUPPORT_NIP45'],
    ENABLE_TAG_QUERIES: ['SUPPORT_NIP12'],
    ENABLE_SEARCH: ['SUPPORT_NIP50'],
    ENABLE_NIP42: ['SUPPORT_NIP42'],
    REQUIRE_AUTH_FOR_QUERY: ['SUPPORT_NIP42'],
    REQUIRE_AUTH_FOR_COUNT: ['SUPPORT_NIP42'],
    REQUIRE_AUTH_FOR_PUBLISH: ['SUPPORT_NIP42'],
    AUTH_MUST_MATCH_EVENT_PUBKEY: ['SUPPORT_NIP42'],
    AUTH_MAX_AGE_SECS: ['SUPPORT_NIP42'],
    ENABLE_FILE_METADATA: ['SUPPORT_NIP94'],
    ENABLE_FILE_API: ['SUPPORT_NIP96'],
    FILE_API_URL: ['SUPPORT_NIP96'],
    REQUIRE_NIP98_AUTH: ['SUPPORT_NIP96', 'SUPPORT_NIP98'],
    ENABLE_BLOSSOM: ['SUPPORT_NIP_B7'],
    ENABLE_BLOSSOM_LIST: ['SUPPORT_NIP_B7'],
    ENABLE_BLOSSOM_MIRROR: ['SUPPORT_NIP_B7'],
    REQUIRE_BLOSSOM_AUTH: ['SUPPORT_NIP_B7'],
    REQUIRE_BLOSSOM_GET_AUTH: ['SUPPORT_NIP_B7'],
    BLOSSOM_PUBLIC_URL: ['SUPPORT_NIP_B7']
  };

  var fieldNipByField = {
    ENABLE_NIP11: 'NIP-11',
    ENABLE_COUNT: 'NIP-45',
    ENABLE_TAG_QUERIES: 'NIP-12',
    ENABLE_SEARCH: 'NIP-50',
    ENABLE_NIP42: 'NIP-42',
    REQUIRE_AUTH_FOR_QUERY: 'NIP-42',
    REQUIRE_AUTH_FOR_COUNT: 'NIP-42',
    REQUIRE_AUTH_FOR_PUBLISH: 'NIP-42',
    AUTH_MUST_MATCH_EVENT_PUBKEY: 'NIP-42',
    AUTH_MAX_AGE_SECS: 'NIP-42',
    REQUIRE_NIP98_AUTH: 'NIP-98',
    ENABLE_FILE_METADATA: 'NIP-94',
    ENABLE_FILE_API: 'NIP-96',
    FILE_API_URL: 'NIP-96',
    ENABLE_BLOSSOM: 'NIP-B7',
    ENABLE_BLOSSOM_LIST: 'NIP-B7',
    ENABLE_BLOSSOM_MIRROR: 'NIP-B7',
    REQUIRE_BLOSSOM_AUTH: 'NIP-B7',
    REQUIRE_BLOSSOM_GET_AUTH: 'NIP-B7',
    BLOSSOM_PUBLIC_URL: 'NIP-B7',
    SUPPORT_NIP09: 'NIP-09',
    SUPPORT_NIP11: 'NIP-11',
    SUPPORT_NIP12: 'NIP-12',
    SUPPORT_NIP40: 'NIP-40',
    SUPPORT_NIP42: 'NIP-42',
    SUPPORT_NIP45: 'NIP-45',
    SUPPORT_NIP50: 'NIP-50',
    SUPPORT_NIP94: 'NIP-94',
    SUPPORT_NIP96: 'NIP-96',
    SUPPORT_NIP98: 'NIP-98',
    SUPPORT_NIP_B7: 'NIP-B7'
  };

  var wideFieldEnvKeys = {
    STORE_ROOT: true,
    RELAYS_UPSTREAM: true,
    FILTER_AUTHORS: true,
    FILTER_KINDS: true,
    FILTER_TAG_A: true,
    FILTER_TAG_T: true,
    OWNER_PUBKEYS: true,
    FOLLOW_PUBKEYS: true,
    PIN_EVENT_IDS: true,
    ALLOW_KINDS: true,
    DENY_KINDS: true,
    FILE_API_URL: true,
    BLOSSOM_PUBLIC_URL: true,
    FILE_ALLOW_MIME: true,
    FILE_DENY_MIME: true,
    ALLOW_PUBKEYS: true,
    DENY_PUBKEYS: true
  };

  var els = {
    shell: document.querySelector('.shell'),
    shellDivider: document.getElementById('shell-divider'),
    stage: document.querySelector('.stage'),
    sectionList: document.getElementById('section-list'),
    sectionContent: document.getElementById('section-content'),
    runtimePanel: document.querySelector('.runtime-panel'),
    runtimeGrid: document.getElementById('runtime-grid'),
    relayPill: document.getElementById('relay-pill'),
    activeTitle: document.getElementById('active-title'),
    activeSubtitle: document.getElementById('active-subtitle'),
    toast: document.getElementById('toast'),
    saveStatus: document.getElementById('save-status'),
    settingsDrawer: document.getElementById('settings-drawer'),
    openSettings: document.getElementById('open-settings'),
    closeSettings: document.getElementById('close-settings'),
    bridgeCopy: document.getElementById('bridge-copy'),
    envPath: document.getElementById('env-path'),
    doctorOutput: document.getElementById('doctor-output'),
    openStoreRoot: document.getElementById('open-store-root'),
    relayToggle: document.getElementById('relay-toggle'),
    diagnosticsDoctor: null
  };

  init();

  function init() {
    state.bridge = bridgeAvailable();
    window.stonrHostAction = function (actionName) {
      handleHostAction(actionName);
    };
    [els.envPath, els.relayToggle, els.openStoreRoot].forEach(function (button) {
      button.disabled = !state.bridge;
    });
    els.openSettings.addEventListener('click', function () {
      els.settingsDrawer.classList.remove('hidden');
    });
    els.closeSettings.addEventListener('click', function () {
      els.settingsDrawer.classList.add('hidden');
    });
    els.settingsDrawer.addEventListener('click', function (event) {
      if (event.target === els.settingsDrawer) {
        els.settingsDrawer.classList.add('hidden');
      }
    });
    els.envPath.addEventListener('change', queueEnvPathSave);
    els.envPath.addEventListener('blur', queueEnvPathSave);
    els.relayToggle.addEventListener('click', function () {
      runRelayToggle();
    });
    els.openStoreRoot.addEventListener('click', function () {
      openStoreRoot();
    });
    els.shellDivider.addEventListener('pointerdown', startRailResize);
    window.addEventListener('pointermove', resizeRail);
    window.addEventListener('pointerup', stopRailResize);
    window.addEventListener('pointercancel', stopRailResize);
    window.addEventListener('resize', syncRailWidth);
    document.addEventListener('visibilitychange', function () {
      if (document.visibilityState === 'hidden') {
        flushPendingFieldSaves().catch(function (error) {
          console.error(error);
        });
      } else if (document.visibilityState === 'visible') {
        refreshLiveState().catch(function (error) {
          console.error(error);
        });
      }
    });
    window.addEventListener('pagehide', function () {
      flushPendingFieldSaves().catch(function (error) {
        console.error(error);
      });
    });
    document.addEventListener('keydown', handleGlobalKeydown, true);
    renderSectionList();
    startRefreshLoop();
    loadAll();
  }

  function handleHostAction(actionName) {
    var action = String(actionName || '').trim().toLowerCase();
    if (!action) {
      return;
    }
    if (action === 'refresh') {
      refreshLiveState().catch(function (error) {
        console.error(error);
      });
      return;
    }
    if (action === 'show-events') {
      setActiveSection('events', true);
      return;
    }
    if (action === 'show-relay') {
      setActiveSection('relay', true);
      return;
    }
    if (action === 'open-store-root') {
      openStoreRoot().catch(function (error) {
        console.error(error);
      });
      return;
    }
    if (action === 'toggle-relay') {
      runRelayToggle().catch(function (error) {
        console.error(error);
      });
    }
  }

  function handleGlobalKeydown(event) {
    if (!event || event.defaultPrevented) {
      return;
    }
    if (event.key !== 'Tab' || !event.ctrlKey || event.metaKey || event.altKey) {
      return;
    }
    event.preventDefault();
    cycleSection(event.shiftKey ? -1 : 1);
  }

  function cycleSection(direction) {
    var total = sections.length;
    if (!total) {
      return;
    }
    var currentIndex = sections.findIndex(function (section) {
      return section.id === state.activeSection;
    });
    if (currentIndex < 0) {
      currentIndex = 0;
    }
    var delta = direction < 0 ? -1 : 1;
    var nextIndex = (currentIndex + delta + total) % total;
    setActiveSection(sections[nextIndex].id, true);
  }

  function inferAppDir() {
    var path = decodeURIComponent(window.location.pathname || '');
    return path.replace(/\/index\.html$/, '');
  }

  function backendScript() {
    return state.appDir + '/scripts/stonr-control-backend.sh';
  }

  function bridgeAvailable() {
    return !!(window.wizardry && window.wizardry.exec);
  }

  function toast(message, kind) {
    els.toast.textContent = message;
    els.toast.className = 'toast show ' + (kind || '');
    if (state.toastTimer) {
      clearTimeout(state.toastTimer);
    }
    state.toastTimer = setTimeout(function () {
      els.toast.className = 'toast';
    }, 2600);
  }

  function shouldTrackSaveStatus(field) {
    return field.type === 'text' || field.type === 'number' || field.type === 'textarea';
  }

  function hideSaveStatus() {
    clearTimeout(state.saveStatusTimer);
    clearTimeout(state.saveStatusHideTimer);
    state.saveStatusTimer = null;
    state.saveStatusHideTimer = null;
    state.saveStatusShownAt = 0;
    els.saveStatus.className = 'save-status';
    els.saveStatus.textContent = '';
  }

  function showSaveStatusSaving(ticket) {
    if (!ticket || ticket !== state.saveStatusTicket) {
      return;
    }
    clearTimeout(state.saveStatusTimer);
    clearTimeout(state.saveStatusHideTimer);
    state.saveStatusTimer = null;
    state.saveStatusHideTimer = null;
    state.saveStatusShownAt = Date.now();
    els.saveStatus.innerHTML = '<span>Saving...</span><span class="action-spinner" aria-hidden="true"></span>';
    els.saveStatus.className = 'save-status show';
  }

  function showSaveStatusSaved(ticket) {
    if (!ticket || ticket !== state.saveStatusTicket) {
      return;
    }
    var elapsed = state.saveStatusShownAt ? Date.now() - state.saveStatusShownAt : 0;
    var wait = Math.max(0, 160 - elapsed);
    clearTimeout(state.saveStatusHideTimer);
    state.saveStatusHideTimer = setTimeout(function () {
      if (ticket !== state.saveStatusTicket) {
        return;
      }
      els.saveStatus.textContent = 'Saved.';
      els.saveStatus.className = 'save-status show saved';
      state.saveStatusHideTimer = setTimeout(function () {
        if (ticket !== state.saveStatusTicket) {
          return;
        }
        hideSaveStatus();
      }, 1200);
    }, wait);
  }

  function revealBootUi() {
    document.body.classList.remove('stonr-booting');
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

  async function execArgv(argv) {
    if (!state.bridge) {
      throw new Error('wizardry bridge unavailable; run this app in the native host');
    }
    var result = await window.wizardry.exec(argv);
    if (typeof result.exit_code !== 'undefined' && result.exit_code !== 0) {
      throw new Error(String(result.stderr || result.stdout || 'command failed').trim());
    }
    return result;
  }

  async function backend(command, extraArgs) {
    var argv = ['sh', backendScript(), command].concat(extraArgs || []);
    var result = await execArgv(argv);
    return String(result.stdout || '');
  }

  function summarizeBackendError(error, fallback) {
    var message = String((error && error.message) || fallback || 'Command failed').trim();
    if (!message) {
      return fallback || 'Command failed';
    }
    if (message.indexOf('stonr-control-backend:') === 0) {
      return message.replace(/^stonr-control-backend:\s*/, '').trim();
    }
    if (message.indexOf('Traceback ') >= 0) {
      message = message.split(/\nTraceback /, 1)[0].trim();
    }
    message = message.split('\n').find(function (line) {
      return String(line || '').trim();
    }) || message;
    if (/failed to run custom build command/.test(message)) {
      return 'Failed to build local stonr binary.';
    }
    return message;
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

  function createNipFragment(text) {
    var fragment = document.createDocumentFragment();
    var source = String(text || '');
    var pattern = /NIP-(?:\d+|B7)/g;
    var lastIndex = 0;
    var match;
    while ((match = pattern.exec(source))) {
      if (match.index > lastIndex) {
        fragment.appendChild(document.createTextNode(source.slice(lastIndex, match.index)));
      }
      var token = match[0];
      var node = document.createElement('span');
      node.className = 'nip-ref';
      node.textContent = token;
      if (nipSummaries[token]) {
        node.title = nipSummaries[token];
        node.setAttribute('aria-label', token + ': ' + nipSummaries[token]);
      }
      fragment.appendChild(node);
      lastIndex = match.index + token.length;
    }
    if (lastIndex < source.length) {
      fragment.appendChild(document.createTextNode(source.slice(lastIndex)));
    }
    return fragment;
  }

  function appendNipText(node, text) {
    node.textContent = '';
    node.appendChild(createNipFragment(text));
  }

  async function loadUiPrefs() {
    if (!state.bridge) {
      return {};
    }
    try {
      return parseKv(await backend('get-ui-prefs'));
    } catch (error) {
      console.error(error);
      return {};
    }
  }

  async function saveUiPref(key, value) {
    if (!state.bridge) {
      return;
    }
    try {
      await backend('set-ui-pref', [key, String(value || '')]);
    } catch (error) {
      console.error(error);
    }
  }

  async function saveDesktopPrefs() {
    if (!state.bridge) {
      return;
    }
    await saveUiPref('background_mode', state.backgroundMode ? '1' : '0');
    await saveUiPref('menu_bar_icon', state.menuBarIcon ? '1' : '0');
    await saveUiPref('auto_start_relay_on_open', state.autoStartRelayOnOpen ? '1' : '0');
    await syncDesktopHostSettings();
  }

  async function loadStartupServiceStatus() {
    if (!state.bridge) {
      state.startupServiceEnabled = false;
      state.startupServiceManager = 'none';
      return;
    }
    try {
      var kv = parseKv(await backend('service-autostart-status', [state.envPath]));
      state.startupServiceEnabled = matchesBool(kv.enabled || '');
      state.startupServiceManager = String(kv.manager || 'none');
    } catch (error) {
      console.error(error);
      state.startupServiceEnabled = false;
      state.startupServiceManager = 'none';
    }
  }

  async function syncDesktopHostSettings() {
    if (!state.bridge) {
      return;
    }
    await setHostBackgroundModeWithRelayState(relayRunningNow());
    if (!state.menuBarIcon) {
      state.hostStatusItemRelayRunning = null;
      return;
    }
    var hostState = await readHostStatusItemState();
    if (hostStatusItemHealthy(hostState)) {
      state.hostStatusItemRelayRunning = relayRunningNow();
      return;
    }
    await new Promise(function (resolve) {
      setTimeout(resolve, 120);
    });
    await setHostBackgroundModeWithRelayState(relayRunningNow());
    hostState = await readHostStatusItemState();
    if (!hostStatusItemHealthy(hostState)) {
      toast('Menu bar icon did not activate in host.', 'bad');
      return;
    }
    state.hostStatusItemRelayRunning = relayRunningNow();
  }

  function relayRunningNow() {
    return !!(state.status && state.status.status === 'running');
  }

  async function setHostBackgroundModeWithRelayState(relayRunning) {
    var argvWithRelay = [
      '__wizardry_host_set_background_mode',
      state.backgroundMode ? '1' : '0',
      state.menuBarIcon ? '1' : '0',
      relayRunning ? '1' : '0'
    ];
    try {
      await execArgv(argvWithRelay);
      return;
    } catch (_error) {
      var argvLegacy = [
        '__wizardry_host_set_background_mode',
        state.backgroundMode ? '1' : '0',
        state.menuBarIcon ? '1' : '0'
      ];
      await execArgv(argvLegacy);
    }
  }

  async function syncHostStatusItemRelayIcon() {
    if (!state.bridge || !state.menuBarIcon) {
      state.hostStatusItemRelayRunning = null;
      return;
    }
    var running = relayRunningNow();
    if (state.hostStatusItemRelayRunning === running) {
      return;
    }
    try {
      await setHostBackgroundModeWithRelayState(running);
      state.hostStatusItemRelayRunning = running;
    } catch (error) {
      console.error(error);
    }
  }

  function hostStatusItemHealthy(hostState) {
    if (!hostState || typeof hostState !== 'object') {
      return false;
    }
    if (Object.prototype.hasOwnProperty.call(hostState, 'status_item_rendered')) {
      return matchesBool(hostState.status_item_rendered || '');
    }
    return matchesBool(hostState.has_status_item || '');
  }

  async function readHostStatusItemState() {
    try {
      var result = await execArgv(['__wizardry_host_status_item_state']);
      return parseKv(String(result.stdout || ''));
    } catch (_error) {
      return {};
    }
  }

  async function loadAll() {
    var prefs = await loadUiPrefs();
    state.envPath = prefs.env_path || state.envPath || '';
    state.backgroundMode = matchesBool(prefs.background_mode || '');
    state.menuBarIcon = matchesBool(prefs.menu_bar_icon || '');
    state.autoStartRelayOnOpen = matchesBool(prefs.auto_start_relay_on_open || '');
    state.hostStatusItemRelayRunning = null;
    state.refreshInFlight = false;
    state.refreshQueued = false;
    state.startupServiceEnabled = false;
    state.startupServiceManager = 'none';
    state.startupServiceBusy = false;
    state.startupServicePendingAction = '';
    state.envValues = {};
    state.activeSection = 'relay';
    state.events = [];
    state.eventsTotal = 0;
    state.eventsBytes = 0;
    state.eventsError = '';
    state.eventsLoading = false;
    state.eventsTotalLoading = false;
    state.eventsLoadedOnce = false;
    state.eventsStatsLoadedAt = 0;
    state.eventsStatsPromise = null;
    state.diagnosticsLoading = false;
    state.diagnosticsLoadedOnce = false;
    state.diagnosticsMirror = [];
    state.diagnosticsRetention = null;
    state.diagnosticsError = '';
    state.railWidth = parseRailWidth(prefs.rail_width) || state.railWidth;
    els.envPath.value = state.envPath;
    applyRailWidth(state.railWidth);
    renderSectionList();
    if (!state.bridge) {
      renderRuntimeFallback();
      renderActiveSection();
      revealBootUi();
      return;
    }
    try {
      state.doctor = await backend('doctor', [state.envPath]);
      state.doctorRefreshedAt = Date.now();
      state.doctorKv = parseKv(state.doctor);
      if (state.doctorKv.env_path && state.doctorKv.env_path !== state.envPath) {
        state.envPath = state.doctorKv.env_path;
        saveUiPref('env_path', state.envPath);
      } else {
        state.envPath = state.doctorKv.env_path || state.envPath;
      }
      els.envPath.value = state.envPath || '';
      els.doctorOutput.textContent = state.doctor.trim() || 'No backend output.';
      state.envValues = parseKv(await backend('load-env', [state.envPath]));
      state.status = parseKv(await backend('relay-status', [state.envPath]));
      await loadStartupServiceStatus();
      await syncDesktopHostSettings();
      await loadConfigForBootFrame(state.configEditSeq);
      syncFieldDependencies();
      renderRuntime();
      renderActiveSection();
      revealBootUi();
      queuePostBootEventsLoad();
      hydrateAfterBoot(true);
      maybeAutoStartRelayOnOpen().catch(function (error) {
        console.error(error);
      });
    } catch (error) {
      console.error(error);
      toast(summarizeBackendError(error, 'Failed to load relay state'), 'bad');
      renderRuntimeFallback();
      renderActiveSection();
      revealBootUi();
    }
  }

  function setActiveSection(sectionId, resetScroll) {
    var section = sections.find(function (item) {
      return item.id === sectionId;
    });
    if (!section) {
      return;
    }
    state.activeSection = section.id;
    renderSectionList();
    renderActiveSection(resetScroll !== false);
    if (section.id === 'diagnostics') {
      queueDiagnosticsLoad();
    }
  }

  function renderSectionList() {
    els.sectionList.innerHTML = '';
    sections.forEach(function (section) {
      var button = document.createElement('button');
      button.type = 'button';
      button.className = 'section-tab' + (section.id === 'events' ? ' events-tab' : '') + (state.activeSection === section.id ? ' active' : '');
      button.setAttribute('role', 'option');
      button.setAttribute('aria-selected', state.activeSection === section.id ? 'true' : 'false');
      button.textContent = section.label;
      button.addEventListener('click', function () {
        setActiveSection(section.id, true);
      });
      els.sectionList.appendChild(button);
    });
  }

  function parseRailWidth(value) {
    var width = parseInt(String(value || '').trim(), 10);
    if (!isFinite(width)) {
      return 0;
    }
    return width;
  }

  function railWidthBounds() {
    var shellWidth = els.shell ? els.shell.clientWidth : window.innerWidth;
    var min = 184;
    var max = Math.max(min, Math.min(360, shellWidth - 360));
    return { min: min, max: max };
  }

  function applyRailWidth(next) {
    var bounds = railWidthBounds();
    state.railWidth = Math.max(bounds.min, Math.min(bounds.max, Math.round(next || bounds.min)));
    document.documentElement.style.setProperty('--rail-width', state.railWidth + 'px');
  }

  function syncRailWidth() {
    applyRailWidth(state.railWidth);
  }

  function startRailResize(event) {
    event.preventDefault();
    state.dragPointerId = event.pointerId;
    document.body.classList.add('stonr-resizing');
    if (els.shellDivider.setPointerCapture) {
      els.shellDivider.setPointerCapture(event.pointerId);
    }
  }

  function resizeRail(event) {
    if (state.dragPointerId === null || event.pointerId !== state.dragPointerId) {
      return;
    }
    var shellBounds = els.shell.getBoundingClientRect();
    applyRailWidth(event.clientX - shellBounds.left);
  }

  function stopRailResize(event) {
    if (state.dragPointerId === null || (event.pointerId !== state.dragPointerId && event.type !== 'resize')) {
      return;
    }
    if (els.shellDivider.releasePointerCapture && event.pointerId === state.dragPointerId) {
      try {
        els.shellDivider.releasePointerCapture(event.pointerId);
      } catch (error) {
        console.error(error);
      }
    }
    state.dragPointerId = null;
    document.body.classList.remove('stonr-resizing');
    saveUiPref('rail_width', state.railWidth);
  }

  function renderRuntime() {
    var status = state.status || {};
    var runtimeStatus = status.status || 'stopped';
    var pidValue = status.pid ? String(status.pid) : '';
    var pidOptions = null;
    if (runtimeStatus !== 'running' && !pidValue) {
      pidValue = 'not running';
      pidOptions = { empty: true };
    }
    els.relayPill.textContent = 'Relay: ' + (status.status || 'stopped');
    els.relayPill.className = 'status-pill ' + (
      status.status === 'running' ? 'good' : status.status === 'stopped' ? 'neutral' : 'bad'
    );
    syncRelayToggle(status);
    syncRuntimePanelVisibility();
    els.runtimeGrid.innerHTML = '';
    els.runtimeGrid.appendChild(renderKv('Status', runtimeStatus));
    els.runtimeGrid.appendChild(renderKv('PID', pidValue, pidOptions));
    els.runtimeGrid.appendChild(renderKv('Env', state.envPath || ''));
    els.runtimeGrid.appendChild(renderKv('Store root', status.store_root || ''));
    els.runtimeGrid.appendChild(renderKv('PID file', status.pid_path || ''));
    els.runtimeGrid.appendChild(renderKv('Log file', status.log_path || ''));
  }

  function renderRuntimeFallback() {
    els.doctorOutput.textContent = state.bridge
      ? 'Backend not loaded yet.'
      : 'Bridge unavailable in hosted web mode.';
    els.runtimeGrid.innerHTML = '';
    [
      ['Status', state.bridge ? 'loading' : 'desktop bridge required'],
      ['Env', state.envPath || '.env']
    ].forEach(function (pair) {
      els.runtimeGrid.appendChild(renderKv(pair[0], pair[1]));
    });
    els.relayPill.textContent = state.bridge ? 'Relay: loading' : 'Relay: bridge unavailable';
    els.relayPill.className = 'status-pill ' + (state.bridge ? 'neutral' : 'bad');
    syncRelayToggle({ status: 'stopped' });
    syncRuntimePanelVisibility();
  }

  function renderKv(label, value, options) {
    var dl = document.createElement('dl');
    dl.className = 'kv';
    var dt = document.createElement('dt');
    dt.textContent = label;
    var dd = document.createElement('dd');
    dd.textContent = String(value || '');
    if (options && options.empty) {
      dd.classList.add('kv-empty');
    }
    dl.appendChild(dt);
    dl.appendChild(dd);
    return dl;
  }

  function renderActiveSection(resetScroll) {
    var section = sections.find(function (item) {
      return item.id === state.activeSection;
    }) || sections[0];
    els.activeTitle.textContent = section.title;
    if (section.id === 'policies') {
      els.activeSubtitle.hidden = false;
      els.activeSubtitle.textContent = section.detail || '';
    } else {
      els.activeSubtitle.hidden = true;
      els.activeSubtitle.textContent = '';
    }
    syncRuntimePanelVisibility();
    els.sectionContent.innerHTML = '';
    state.fieldNodes = {};
    if (section.fields.length) {
      els.sectionContent.appendChild(renderFieldSection(section));
    }
    if (section.id === 'general') {
      els.sectionContent.appendChild(renderDesktopSection());
      els.sectionContent.appendChild(renderGeneralRuntimeSection());
    }
    if (section.custom === 'moderation') {
      els.sectionContent.appendChild(renderModerationSection());
    }
    if (section.custom === 'events') {
      els.sectionContent.appendChild(renderEventsSection());
      ensureEventsLoaded();
    }
    if (section.custom === 'diagnostics') {
      els.sectionContent.appendChild(renderDiagnosticsSection());
    }
    syncFieldDependencies();
    if (resetScroll) {
      els.stage.scrollTop = 0;
    }
  }

  function syncRuntimePanelVisibility() {
    els.runtimePanel.classList.add('hidden');
  }

  function renderFieldSection(section) {
    var card = document.createElement('section');
    card.className = 'section-panel autosave-panel';

    var grid = document.createElement('div');
    grid.className = 'field-grid';
    section.fields.forEach(function (field) {
      grid.appendChild(renderField(field, section.id));
    });
    card.appendChild(grid);
    return card;
  }

  function renderField(field, sectionId) {
    if (field.type === 'group') {
      return renderGroupField(field);
    }
    if (field.type === 'note') {
      return renderNoteField(field);
    }
    if (field.type === 'retention-apply') {
      return renderRetentionApplyField();
    }
    var wrap = document.createElement('div');
    wrap.className = 'field' + (field.type === 'bool' ? ' checkbox-field' : '');
    if (field.collapseWhenUnavailable) {
      wrap.classList.add('field-collapsible');
    }
    if (sectionId === 'policies' && field.nipMaster) {
      wrap.classList.add('nip-master-field');
    }
    if (field.nipChild) {
      wrap.classList.add('nip-child-field');
    }
    var showHint = false;
    if (field.type === 'text' || field.type === 'textarea') {
      showHint = !!field.hint;
    } else if (field.type === 'bool' && (sectionId === 'policies' || field.hint)) {
      showHint = !!(field.hint || field.tooltip);
    }
    if (field.type === 'bool' && showHint) {
      wrap.classList.add('has-hint');
    }
    var input;
    var button;
    var unit;
    var helpText = field.tooltip || field.hint || field.label;

    if (field.type === 'bool') {
      input = document.createElement('input');
      input.type = 'checkbox';
      input.checked = !!resolvedFieldValue(field);
    } else if (field.type === 'radio') {
      input = document.createElement('div');
      input.className = 'radio-group';
      var selectedValue = String(displayValue(field) || field.options[0].value);
      input.dataset.envKey = field.envKey;
      input.dataset.path = field.path || '';
      input.dataset.savedValue = selectedValue;
      input.dataset.baseDisabled = !state.bridge ? '1' : '0';
      input.title = helpText;
      input.setAttribute('aria-description', helpText);
      field.options.forEach(function (option) {
        var optionWrap = document.createElement('label');
        optionWrap.className = 'radio-option';
        var radio = document.createElement('input');
        radio.type = 'radio';
        radio.name = field.envKey;
        radio.value = option.value;
        radio.checked = selectedValue === String(option.value);
        radio.disabled = !state.bridge;
        radio.dataset.baseDisabled = !state.bridge ? '1' : '0';
        radio.dataset.envKey = field.envKey;
        radio.dataset.path = field.path || '';
        radio.dataset.savedValue = selectedValue;
        radio.title = helpText;
        radio.setAttribute('aria-description', helpText);
        bindFieldAutosave(field, radio);
        var optionText = document.createElement('span');
        optionText.textContent = option.label;
        optionWrap.appendChild(radio);
        optionWrap.appendChild(optionText);
        input.appendChild(optionWrap);
      });
    } else if (field.type === 'select') {
      input = document.createElement('select');
      field.options.forEach(function (option) {
        var node = document.createElement('option');
        node.value = option.value;
        node.textContent = option.label;
        input.appendChild(node);
      });
      input.value = displayValue(field);
    } else if (field.type === 'textarea') {
      input = document.createElement('textarea');
      input.value = displayValue(field);
    } else {
      input = document.createElement('input');
      input.type = field.type === 'number' ? 'number' : 'text';
      input.value = displayValue(field);
      input.spellcheck = false;
    }

    if (field.type !== 'radio') {
      input.disabled = !state.bridge;
    }
    if (field.type !== 'radio' && wideFieldEnvKeys[field.envKey]) {
      input.classList.add('field-input-wide');
    }
    if (field.type !== 'radio') {
      input.dataset.envKey = field.envKey;
      input.dataset.path = field.path || '';
      input.dataset.savedValue = serializeInput(field, input);
      input.dataset.baseDisabled = !state.bridge ? '1' : '0';
      input.title = helpText;
      input.setAttribute('aria-description', helpText);
      bindFieldAutosave(field, input);
    }

    if (field.browseDir) {
      button = document.createElement('button');
      button.type = 'button';
      button.className = 'action mini browse-btn';
      button.textContent = 'Browse...';
      button.disabled = !state.bridge;
      button.dataset.baseDisabled = !state.bridge ? '1' : '0';
      button.title = 'Choose a folder on disk';
      button.setAttribute('aria-label', 'Browse for ' + field.label.toLowerCase());
      button.addEventListener('click', function () {
        browseFieldDirectory(field, input, button).catch(function (error) {
          console.error(error);
          toast(error.message || 'Failed to choose folder', 'bad');
        });
      });
    }

    if (field.type === 'number' && field.unit) {
      unit = document.createElement('span');
      unit.className = 'field-unit';
      unit.textContent = field.unit;
      unit.setAttribute('aria-hidden', 'true');
    }

    if (field.type !== 'radio') {
      input.id = field.envKey;
    }
    var nipPill = createFieldNipPill(field);
    var label = createFieldLabel(field, sectionId, helpText);
    var nipSummary = createFieldNipSummary(field, sectionId);

    var hint = document.createElement('p');
    hint.className = 'hint';
    appendNipText(hint, field.hint || (sectionId === 'policies' ? (field.tooltip || '') : ''));
    hint.title = helpText;

    if (field.type === 'bool') {
      var labelRow = document.createElement('div');
      labelRow.className = 'checkbox-label-row';
      labelRow.appendChild(label);
      var viewLink = createFieldViewLink(field);
      if (viewLink) {
        labelRow.appendChild(viewLink);
      }
      wrap.appendChild(input);
      if (nipPill) {
        if (field.nipMaster) {
          nipPill.tabIndex = 0;
          nipPill.setAttribute('role', 'button');
          nipPill.setAttribute('aria-label', 'Toggle ' + field.label);
          nipPill.addEventListener('click', function () {
            if (!input.disabled) {
              input.click();
            }
          });
          nipPill.addEventListener('keydown', function (event) {
            if (event.key !== 'Enter' && event.key !== ' ') {
              return;
            }
            event.preventDefault();
            if (!input.disabled) {
              input.click();
            }
          });
        }
        wrap.appendChild(nipPill);
      }
      wrap.appendChild(labelRow);
      bindCheckboxLabel(label, input);
      if (nipSummary) {
        wrap.appendChild(nipSummary);
      }
    } else {
      var main = document.createElement('div');
      main.className = 'field-main';
      var controls = document.createElement('div');
      controls.className = 'field-controls';
      main.appendChild(label);
      controls.appendChild(input);
      if (unit) {
        controls.appendChild(unit);
      }
      if (button) {
        controls.appendChild(button);
      }
      main.appendChild(controls);
      if (nipPill) {
        main.appendChild(nipPill);
      }
      wrap.appendChild(main);
    }
    if (showHint) {
      wrap.appendChild(hint);
    }
    state.fieldNodes[field.envKey] = {
      field: field,
      wrap: wrap,
      input: input,
      button: button,
      unit: unit,
      label: label,
      hint: showHint ? hint : null,
      nipPill: nipPill,
      helpText: helpText
    };
    if (field.type === 'bool' || field.type === 'radio') {
      input.addEventListener('change', function () {
        syncFieldDependencies();
      });
    }
    return wrap;
  }

  function createFieldNipPill(field) {
    if (field.hideNipPill) {
      return null;
    }
    var nipToken = fieldNipByField[field.envKey];
    if (!nipToken) {
      return null;
    }
    var pill = document.createElement('span');
    pill.className = 'scope-pill nip-pill';
    pill.dataset.nipToken = nipToken;
    pill.textContent = nipToken;
    pill.title = nipSummaries[nipToken] || nipToken;
    return pill;
  }

  function createFieldLabel(field, sectionId, helpText) {
    var nipToken = fieldNipByField[field.envKey];
    if (sectionId === 'policies' && field.nipMaster && nipToken && nipUrls[nipToken]) {
      var link = document.createElement('a');
      link.className = 'field-link';
      link.href = nipUrls[nipToken];
      link.target = '_blank';
      link.rel = 'noreferrer noopener';
      link.textContent = field.label;
      link.title = helpText;
      link.setAttribute('aria-label', field.label + '. Open ' + nipToken + ' on GitHub');
      return link;
    }
    var label = document.createElement('label');
    appendNipText(label, field.label);
    label.htmlFor = field.envKey;
    label.title = helpText;
    return label;
  }

  function createFieldViewLink(field) {
    if (!field || !field.viewCommand) {
      return null;
    }
    var link = document.createElement('button');
    link.type = 'button';
    link.className = 'field-view-link';
    link.textContent = field.viewLabel || 'View';
    link.disabled = !state.bridge;
    link.title = field.viewHint || ('Open ' + field.label);
    link.addEventListener('click', function (event) {
      event.preventDefault();
      event.stopPropagation();
      if (!state.bridge) {
        return;
      }
      backend(field.viewCommand, [state.envPath]).catch(function (error) {
        console.error(error);
        toast(summarizeBackendError(error, 'Failed to open view'), 'bad');
      });
    });
    return link;
  }

  function bindCheckboxLabel(label, input) {
    if (!label || !input || label.tagName !== 'LABEL') {
      return;
    }
    label.addEventListener('click', function (event) {
      event.preventDefault();
      if (input.disabled) {
        return;
      }
      input.click();
    });
  }

  function createFieldNipSummary(field, sectionId) {
    return null;
  }

  function renderGroupField(field) {
    var group = document.createElement('div');
    group.className = 'field-group';
    appendNipText(group, field.label);
    return group;
  }

  function renderNoteField(field) {
    var note = document.createElement('p');
    note.className = 'section-note';
    note.textContent = field.text;
    return note;
  }

  function renderRetentionApplyField() {
    var wrap = document.createElement('div');
    wrap.className = 'field retention-apply-field';
    var button = document.createElement('button');
    button.type = 'button';
    button.className = 'action mini retention-apply-btn';
    button.textContent = state.retentionBusy ? 'Starting...' : 'Save And Apply';
    button.disabled = !state.bridge || state.retentionBusy;
    button.title = 'Save these retention limits and prune stored events immediately.';
    button.addEventListener('click', function () {
      applyRetentionSettings().catch(function (error) {
        console.error(error);
        toast(error.message || 'Failed to apply retention', 'bad');
      });
    });
    wrap.appendChild(button);
    return wrap;
  }

  function renderDesktopSection() {
    var card = document.createElement('section');
    card.className = 'section-panel autosave-panel';
    card.appendChild(renderCardHead('Desktop App', ''));

    var grid = document.createElement('div');
    grid.className = 'field-grid';

    grid.appendChild(renderDesktopToggleField(
      'Keep running when window closes',
      state.backgroundMode,
      function (checked) {
        state.backgroundMode = checked;
      },
      'Hide the window instead of quitting the app so the relay keeps running in the background.'
    ));

    grid.appendChild(renderDesktopToggleField(
      'Show menu bar icon',
      state.menuBarIcon,
      function (checked) {
        state.menuBarIcon = checked;
      },
      'Show a menu bar or tray icon so you can reopen the window or quit while the relay keeps running.',
      false
    ));

    grid.appendChild(renderDesktopToggleField(
      'Start relay when Stonr opens',
      state.autoStartRelayOnOpen,
      function (checked) {
        state.autoStartRelayOnOpen = checked;
      },
      'Automatically start the relay each time this app opens.'
    ));

    grid.appendChild(renderDesktopToggleField(
      'Auto-start relay when system starts',
      state.startupServiceEnabled,
      function () {
        return;
      },
      state.startupServiceManager === 'none'
        ? 'Startup service is unavailable on this host.'
        : 'Install a user startup service so the relay starts automatically when your system starts.',
      state.startupServiceBusy || state.startupServiceManager === 'none',
      function (checked) {
        return setStartupServiceEnabled(checked);
      },
      state.startupServiceBusy
        ? (state.startupServicePendingAction === 'disable' ? 'Disabling...' : 'Enabling...')
        : ''
    ));

    card.appendChild(grid);
    return card;
  }

  async function maybeAutoStartRelayOnOpen() {
    if (!state.bridge || state.autoStartRelayChecked) {
      return;
    }
    state.autoStartRelayChecked = true;
    if (!state.autoStartRelayOnOpen || relayRunningNow()) {
      return;
    }
    try {
      state.relayBusyAction = 'relay-start';
      syncRelayToggle(state.status || { status: 'stopped' });
      state.status = parseKv(await backend('relay-start', [state.envPath]));
      renderRuntime();
      if (state.activeSection === 'events') {
        await loadEvents();
      }
      if (state.activeSection === 'diagnostics') {
        await Promise.all([loadLog(), loadDiagnosticsStatus()]);
      }
      renderActiveSection();
    } catch (error) {
      console.error(error);
      toast(summarizeBackendError(error, 'Failed to auto-start relay'), 'bad');
    } finally {
      state.relayBusyAction = '';
      syncRelayToggle(state.status || { status: 'stopped' });
    }
  }

  function renderGeneralRuntimeSection() {
    var card = document.createElement('section');
    card.className = 'section-panel';
    card.appendChild(renderCardHead('Runtime', 'Live relay process state and active paths.'));

    var grid = document.createElement('div');
    grid.className = 'kv-grid';

    var status = state.status || {};
    var runtimeStatus = status.status || 'stopped';
    var pidValue = status.pid ? String(status.pid) : '';
    var pidOptions = null;
    if (runtimeStatus !== 'running' && !pidValue) {
      pidValue = 'not running';
      pidOptions = { empty: true };
    }

    grid.appendChild(renderKv('Status', runtimeStatus));
    grid.appendChild(renderKv('PID', pidValue, pidOptions));
    grid.appendChild(renderKv('Relay URL', relayPublicWsUrl()));
    grid.appendChild(renderKv('Profile URL', relayProfileUrl()));
    grid.appendChild(renderKv('Env', state.envPath || ''));
    grid.appendChild(renderKv('Store root', status.store_root || ''));
    grid.appendChild(renderKv('PID file', status.pid_path || ''));
    grid.appendChild(renderKv('Log file', status.log_path || ''));

    card.appendChild(grid);
    return card;
  }

  function renderDesktopToggleField(labelText, checked, onChange, helpText, forceDisabled) {
    var onPersist = arguments[5];
    var busyText = arguments[6];
    var wrap = document.createElement('div');
    wrap.className = 'field checkbox-field';
    if (forceDisabled) {
      wrap.classList.add('field-disabled');
    }
    var inputId = 'desktop-toggle-' + String(labelText || '')
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '');
    var input = document.createElement('input');
    input.type = 'checkbox';
    input.id = inputId;
    input.checked = !!checked;
    input.disabled = !state.bridge || !!forceDisabled;
    var label = document.createElement('label');
    label.htmlFor = inputId;
    label.textContent = labelText;
    label.title = helpText;
    input.title = helpText;
    bindCheckboxLabel(label, input);
    input.addEventListener('change', function () {
      var previous = !input.checked;
      onChange(input.checked);
      var persistPromise;
      if (typeof onPersist === 'function') {
        persistPromise = Promise.resolve().then(function () {
          return onPersist(input.checked);
        });
      } else {
        persistPromise = saveDesktopPrefs();
      }
      persistPromise.then(function () {
        renderActiveSection();
      }).catch(function (error) {
        console.error(error);
        input.checked = previous;
        onChange(previous);
        renderActiveSection();
        toast(summarizeBackendError(error, 'Failed to save desktop settings'), 'bad');
      });
    });
    wrap.appendChild(input);
    wrap.appendChild(label);
    if (busyText) {
      var busy = document.createElement('span');
      busy.className = 'desktop-toggle-busy';
      busy.innerHTML = '<span>' + escapeHtml(String(busyText)) + '</span><span class="action-spinner" aria-hidden="true"></span>';
      wrap.appendChild(busy);
    }
    return wrap;
  }

  function displayValue(field) {
    var value = resolvedFieldValue(field);
    if (field.envKey === 'MAX_STORED_EVENT_BYTES') {
      return formatStoredEventMegabytes(value);
    }
    if (field.envKey === 'MIRROR_MODE') {
      return String(value || '').toLowerCase();
    }
    if (field.format) {
      return field.format(value);
    }
    if (value === null || typeof value === 'undefined') {
      return '';
    }
    return String(value);
  }

  function rawEnvValueByKey(envKey) {
    if (!state.envValues || typeof state.envValues !== 'object') {
      return undefined;
    }
    if (!Object.prototype.hasOwnProperty.call(state.envValues, envKey)) {
      return undefined;
    }
    return state.envValues[envKey];
  }

  function rawEnvValue(field) {
    var value = rawEnvValueByKey(field.envKey);
    if (typeof value === 'undefined') {
      return undefined;
    }
    if (field.type === 'bool') {
      return matchesBool(value);
    }
    if (field.type === 'number') {
      if (String(value).trim() === '') {
        return undefined;
      }
      return Number(value);
    }
    return String(value);
  }

  function matchesBool(value) {
    return /^(1|true|yes|on)$/i.test(String(value || '').trim());
  }

  function configValue(field) {
    var rawValue = rawEnvValue(field);
    if (typeof rawValue !== 'undefined') {
      return rawValue;
    }
    return getPath(state.config || {}, field.path || '');
  }

  function resolvedFieldValue(field) {
    var rawValue = rawEnvValue(field);
    if (typeof rawValue !== 'undefined') {
      return rawValue;
    }
    var value = getPath(state.config || {}, field.path || '');
    if (
      value === null ||
      typeof value === 'undefined' ||
      (typeof value === 'string' && value.trim() === '')
    ) {
      return defaultFieldValue(field);
    }
    return value;
  }

  function defaultFieldValue(field) {
    switch (field.envKey) {
      case 'STORE_ROOT':
        return state.doctorKv.store_root || '';
      case 'RELAYS_UPSTREAM':
        return defaultUpstreamRelays.join(',');
      case 'MIRROR_MODE':
        return 'broad';
      case 'MIRROR_SITE_INCLUDE_COMMENTS':
        return true;
      case 'PUBLIC_RELAY_URL':
        return '';
      case 'RELAY_NAME':
        return 'stonr';
      case 'RELAY_DESCRIPTION':
        return 'File-backed Nostr relay';
      case 'SUPPORT_NIP09':
      case 'SUPPORT_NIP11':
      case 'SUPPORT_NIP12':
      case 'SUPPORT_NIP40':
      case 'SUPPORT_NIP42':
      case 'SUPPORT_NIP45':
      case 'SUPPORT_NIP50':
      case 'SUPPORT_NIP94':
      case 'SUPPORT_NIP96':
      case 'SUPPORT_NIP98':
      case 'SUPPORT_NIP_B7':
        return true;
      case 'VERIFY_SIG':
        return false;
      case 'ENABLE_NIP11':
      case 'ENABLE_QUERY':
      case 'ENABLE_PUBLISH':
      case 'ENABLE_LIVE_SUBSCRIPTIONS':
      case 'ENABLE_COUNT':
      case 'ENABLE_TAG_QUERIES':
      case 'ENABLE_SEARCH':
      case 'ENABLE_MIRRORING':
      case 'ENABLE_FILE_METADATA':
      case 'ENABLE_FILE_API':
      case 'ENABLE_BLOSSOM':
      case 'ENABLE_BLOSSOM_LIST':
      case 'FILTER_PRIVATE_MESSAGES':
      case 'PIN_PROTECT_FROM_DELETES':
        return true;
      case 'ENABLE_NIP42':
      case 'ENABLE_BLOSSOM_MIRROR':
      case 'REQUIRE_NIP98_AUTH':
      case 'REQUIRE_BLOSSOM_AUTH':
      case 'REQUIRE_BLOSSOM_GET_AUTH':
      case 'REQUIRE_AUTH_FOR_QUERY':
      case 'REQUIRE_AUTH_FOR_COUNT':
      case 'REQUIRE_AUTH_FOR_PUBLISH':
        return false;
      case 'AUTH_MUST_MATCH_EVENT_PUBKEY':
        return true;
      case 'FILTER_SINCE_MODE':
        return 'cursor';
      case 'FILE_API_URL':
        return bindHttpOrigin() + '/files';
      case 'BLOSSOM_PUBLIC_URL':
        return bindHttpOrigin();
      default:
        return field.defaultValue;
    }
  }

  function setConfigValue(field, value) {
    if (!field.path) {
      return;
    }
    if (!state.config || typeof state.config !== 'object') {
      state.config = {};
    }
    setPath(state.config, field.path, value);
  }

  function applyInputToState(field, input) {
    var serialized = serializeInput(field, input);
    if (field.type === 'bool') {
      setConfigValue(field, input.checked);
    } else {
      setConfigValue(field, serialized);
    }
    if (!state.envValues || typeof state.envValues !== 'object') {
      state.envValues = {};
    }
    state.envValues[field.envKey] = serialized;
    state.configEditSeq += 1;
  }

  function serializeInput(field, input) {
    if (field.type === 'bool') {
      return input.checked ? '1' : '0';
    }
    if (field.type === 'radio') {
      if (input && input.type === 'radio') {
        if (input.checked) {
          return String(input.value || '');
        }
        var checked = document.querySelector('input[type="radio"][name="' + field.envKey + '"]:checked');
        return checked ? String(checked.value || '') : '';
      }
      var selected = input.querySelector('input[type="radio"]:checked');
      return selected ? String(selected.value || '') : '';
    }
    if (field.envKey === 'MAX_STORED_EVENT_BYTES') {
      return serializeStoredEventMegabytes(input.value);
    }
    if (field.type === 'textarea' && field.lineDelimited) {
      return String(input.value || '')
        .split(/\r?\n/)
        .map(function (item) {
          return item.trim();
        })
        .filter(Boolean)
        .join(',');
    }
    return String(input.value || '').trim();
  }

  function renderEventsSection() {
    var wrap = document.createElement('div');
    wrap.className = 'section-stack';

    var browser = document.createElement('section');
    browser.className = 'section-panel';
    browser.appendChild(renderCardHead('Recent Events', '', false));

    var controls = document.createElement('div');
    controls.className = 'events-toolbar';

    var searchWrap = document.createElement('label');
    searchWrap.className = 'events-search';
    searchWrap.htmlFor = 'events-search';
    searchWrap.textContent = 'Keyword';

    var search = document.createElement('input');
    search.id = 'events-search';
    search.type = 'text';
    search.spellcheck = false;
    search.placeholder = 'Search stored event text';
    search.value = state.eventsSearch;
    search.disabled = !state.bridge;
    search.title = 'Filter stored events by words in their content.';
    search.addEventListener('input', function () {
      queueEventsSearch(search.value);
    });

    searchWrap.appendChild(search);
    controls.appendChild(searchWrap);

    var spacer = document.createElement('div');
    spacer.className = 'events-toolbar-spacer';
    controls.appendChild(spacer);

    var count = document.createElement('span');
    count.className = 'status-pill neutral';
    if (state.eventsLoading) {
      count.textContent = 'Loading events...';
      count.title = 'Loading up to 60 recent matching events.';
    } else {
      count.textContent = 'Showing ' + state.events.length + ' most recent';
      count.title = 'Showing up to 60 recent matching events. Refreshes automatically every 2 seconds while this tab is open.';
    }
    controls.appendChild(count);

    var total = document.createElement('span');
    total.className = 'status-pill neutral';
    if (state.eventsTotalLoading) {
      total.innerHTML = '<span>Loading...</span><span class="action-spinner" aria-hidden="true"></span>';
      total.title = 'Loading the total number of stored events.';
    } else {
      total.textContent = state.eventsTotal + ' stored';
      total.title = 'Total events currently stored on disk for this relay.';
    }
    controls.appendChild(total);

    var purge = document.createElement('button');
    purge.type = 'button';
    purge.className = 'action mini';
    purge.textContent = 'Purge...';
    purge.disabled = !state.bridge || state.eventsLoading;
    purge.title = 'Delete every stored event from this relay.';
    purge.addEventListener('click', function () {
      purgeEvents().catch(function (error) {
        console.error(error);
        toast(error.message || 'Failed to purge events', 'bad');
      });
    });
    controls.appendChild(purge);

    var size = document.createElement('span');
    size.className = 'status-pill neutral events-size-pill';
    if (state.eventsTotalLoading) {
      size.innerHTML = '<span>Loading...</span><span class="action-spinner" aria-hidden="true"></span>';
      size.title = 'Loading total stored event size.';
    } else {
      size.textContent = formatBytes(state.eventsBytes);
      size.title = 'Total disk space used by stored event files.';
    }
    controls.appendChild(size);
    browser.appendChild(controls);

    if (state.eventsError) {
      var error = document.createElement('p');
      error.className = 'events-empty events-error';
      error.textContent = state.eventsError;
      browser.appendChild(error);
      wrap.appendChild(browser);
      return wrap;
    }

    if (state.eventsLoading && !state.events.length) {
      var loading = document.createElement('p');
      loading.className = 'events-empty';
      loading.innerHTML = '<span>Loading events...</span><span class="action-spinner" aria-hidden="true"></span>';
      browser.appendChild(loading);
      wrap.appendChild(browser);
      return wrap;
    }

    if (!state.events.length) {
      var empty = document.createElement('p');
      empty.className = 'events-empty';
      empty.textContent = state.eventsSearch
        ? 'No stored events match that keyword yet.'
        : 'No stored events yet. To pull remote events in, turn on Import from relays and add Source relays in Network, or publish directly to this relay.';
      browser.appendChild(empty);
      wrap.appendChild(browser);
      return wrap;
    }

    var list = document.createElement('div');
    list.className = 'event-list';
    state.events.forEach(function (event) {
      list.appendChild(renderEventRow(event));
    });
    browser.appendChild(list);
    wrap.appendChild(browser);
    return wrap;
  }

  function renderEventRow(event) {
    var row = document.createElement('article');
    row.className = 'event-row';

    var meta = document.createElement('div');
    meta.className = 'event-meta';
    var createdAt = normalizeEventTs(event.created_at);
    var ingestedAt = normalizeEventTs(event.ingested_at);
    var primaryTs = ingestedAt > 0 ? ingestedAt : createdAt;
    meta.appendChild(eventMetaPill('kind ' + String(event.kind)));
    meta.appendChild(eventMetaPill('author ' + String(event.pubkey || '').slice(0, 12)));
    meta.appendChild(eventMetaPill((ingestedAt > 0 ? 'seen ' : '') + formatEventTime(primaryTs)));
    row.appendChild(meta);

    if (event.image_url) {
      var media = document.createElement('img');
      media.className = 'event-media';
      media.src = String(event.image_url);
      media.alt = 'Event image preview';
      media.loading = 'lazy';
      row.appendChild(media);
    }

    var content = document.createElement('p');
    content.className = 'event-content';
    content.textContent = String(event.content || '').trim() || '(empty content)';
    content.title = String(event.content || '').trim() || '(empty content)';
    row.appendChild(content);

    var footer = document.createElement('p');
    footer.className = 'event-id';
    footer.textContent = String(event.id || '').slice(0, 24);
    row.appendChild(footer);

    return row;
  }

  function eventMetaPill(text) {
    var node = document.createElement('span');
    node.className = 'scope-pill event-pill';
    node.textContent = text;
    return node;
  }

  function formatEventTime(value) {
    var stamp = Number(value || 0);
    if (!isFinite(stamp) || stamp <= 0) {
      return 'unknown time';
    }
    try {
      return new Date(stamp * 1000).toLocaleString([], {
        year: 'numeric',
        month: 'short',
        day: 'numeric',
        hour: 'numeric',
        minute: '2-digit'
      });
    } catch (error) {
      return String(stamp);
    }
  }

  function formatBytes(value) {
    var bytes = Number(value || 0);
    if (!isFinite(bytes) || bytes <= 0) {
      return '0 B';
    }
    var units = ['B', 'KB', 'MB', 'GB', 'TB'];
    var unitIndex = 0;
    while (bytes >= 1024 && unitIndex < units.length - 1) {
      bytes /= 1024;
      unitIndex += 1;
    }
    var digits = bytes >= 100 || unitIndex === 0 ? 0 : bytes >= 10 ? 1 : 2;
    return bytes.toFixed(digits).replace(/\.0+$|(\.\d*[1-9])0+$/, '$1') + ' ' + units[unitIndex];
  }

  function queueEventsSearch(nextValue) {
    state.eventsSearch = String(nextValue || '');
    clearTimeout(state.eventsSearchTimer);
    state.eventsSearchTimer = setTimeout(function () {
      if (state.activeSection === 'events') {
        state.eventsLoading = true;
        renderActiveSection();
      }
      loadEvents().then(function () {
        if (state.activeSection === 'events') {
          renderActiveSection();
        }
      }).catch(function (error) {
        console.error(error);
        toast(summarizeBackendError(error, 'Failed to load events'), 'bad');
      });
    }, 220);
  }

  async function loadEvents() {
    if (!state.bridge) {
      state.events = [];
      state.eventsTotal = 0;
      state.eventsBytes = 0;
      state.eventsError = '';
      state.eventsLoading = false;
      state.eventsTotalLoading = false;
      state.eventsLoadedOnce = true;
      state.eventsStatsLoadedAt = 0;
      state.eventsStatsPromise = null;
      return;
    }
    state.eventsLoading = true;
    var refreshStats = false;
    var hadSnapshot = state.eventsLoadedOnce || state.events.length > 0 || state.eventsTotal > 0 || state.eventsBytes > 0;
    try {
      var output = await backend('query-events', [state.envPath, state.eventsSearch.trim(), '60']);
      var events = JSON.parse(output || '[]');
      events.sort(compareEventsByRecency);
      state.events = events;
      refreshStats = true;
      state.eventsError = '';
      state.eventsLoadedOnce = true;
    } catch (error) {
      console.error(error);
      if (!hadSnapshot) {
        state.events = [];
        state.eventsTotal = 0;
        state.eventsBytes = 0;
        state.eventsStatsLoadedAt = 0;
        state.eventsError = summarizeBackendError(error, 'Failed to load events');
        state.eventsLoadedOnce = true;
      } else {
        state.eventsError = '';
      }
    } finally {
      state.eventsLoading = false;
    }
    if (refreshStats) {
      refreshEventsStats().catch(function (error) {
        console.error(error);
      });
    }
  }

  function normalizeEventTs(value) {
    var stamp = Number(value || 0);
    return isFinite(stamp) && stamp > 0 ? stamp : 0;
  }

  function eventPrimaryTs(event) {
    var ingestedAt = normalizeEventTs(event && event.ingested_at);
    if (ingestedAt > 0) {
      return ingestedAt;
    }
    return normalizeEventTs(event && event.created_at);
  }

  function compareEventsByRecency(a, b) {
    var aPrimary = eventPrimaryTs(a);
    var bPrimary = eventPrimaryTs(b);
    if (bPrimary !== aPrimary) {
      return bPrimary - aPrimary;
    }
    var aCreated = normalizeEventTs(a && a.created_at);
    var bCreated = normalizeEventTs(b && b.created_at);
    if (bCreated !== aCreated) {
      return bCreated - aCreated;
    }
    var aId = String((a && a.id) || '');
    var bId = String((b && b.id) || '');
    if (bId > aId) return 1;
    if (bId < aId) return -1;
    return 0;
  }

  function shouldRefreshEventsStats() {
    return !state.eventsStatsLoadedAt || Date.now() - state.eventsStatsLoadedAt >= 30000;
  }

  function refreshEventsStats() {
    if (!state.bridge) {
      state.eventsTotal = 0;
      state.eventsBytes = 0;
      state.eventsTotalLoading = false;
      state.eventsStatsLoadedAt = 0;
      state.eventsStatsPromise = null;
      return Promise.resolve();
    }
    if (state.eventsStatsPromise) {
      return state.eventsStatsPromise;
    }
    if (!shouldRefreshEventsStats()) {
      return Promise.resolve();
    }
    state.eventsTotalLoading = true;
    state.eventsStatsPromise = Promise.all([
      backend('count-events', [state.envPath]),
      backend('size-events', [state.envPath])
    ]).then(function (results) {
      var total = results[0];
      var size = results[1];
      state.eventsTotal = Number(String(total || '0').trim()) || 0;
      state.eventsBytes = Number(String(size || '0').trim()) || 0;
      state.eventsStatsLoadedAt = Date.now();
    }).catch(function (error) {
      console.error(error);
    }).finally(function () {
      state.eventsTotalLoading = false;
      state.eventsStatsPromise = null;
      if (state.activeSection === 'events') {
        renderActiveSection();
      }
    });
    return state.eventsStatsPromise;
  }

  async function purgeEvents() {
    if (!state.bridge) {
      return;
    }
    var warning = 'Purging events will delete ALL events in this Nostr relay. Anything not mirrored or sourced from elsewhere will be deleted. This action cannot be undone.';
    if (!window.confirm(warning)) {
      return;
    }
    await backend('purge-events', [state.envPath]);
    state.events = [];
    state.eventsTotal = 0;
    state.eventsBytes = 0;
    state.eventsTotalLoading = false;
    state.eventsLoadedOnce = false;
    state.eventsStatsLoadedAt = 0;
    state.eventsStatsPromise = null;
    state.eventsError = '';
    await loadEvents();
    renderActiveSection();
    toast('All relay events purged.');
  }

  function queuePostBootEventsLoad() {
    if (!state.bridge) {
      return;
    }
    setTimeout(function () {
      loadEvents().then(function () {
        if (state.activeSection === 'events') {
          renderActiveSection();
        }
      }).catch(function (error) {
        console.error(error);
      });
    }, 0);
  }

  function ensureEventsLoaded() {
    if (!state.bridge || state.eventsLoading || state.eventsLoadedOnce) {
      return;
    }
    state.eventsLoading = true;
    loadEvents().then(function () {
      if (state.activeSection === 'events') {
        renderActiveSection();
      }
    }).catch(function (error) {
      console.error(error);
      if (state.activeSection === 'events') {
        renderActiveSection();
      }
    });
  }

  async function openStoreRoot() {
    if (!state.bridge) {
      return;
    }
    try {
      await backend('open-store-root', [state.envPath]);
    } catch (error) {
      console.error(error);
      toast(error.message || 'Failed to open relay folder', 'bad');
    }
  }

  function renderModerationSection() {
    var card = document.createElement('section');
    card.className = 'section-panel autosave-panel';
    card.innerHTML = [
      renderCardHeadHtml(
        'Runtime Lists',
        'These files are read directly from the relay store and save automatically. Pubkey lists take effect immediately. File hash denies apply on next config load or restart.'
      ),
      '<div class="field-grid">',
      moderationTextarea('pubkeys-allow', 'Live allow pubkeys', 'Allow these pubkeys immediately.'),
      moderationTextarea('pubkeys-deny', 'Live deny pubkeys', 'Hide and reject these pubkeys immediately.'),
      moderationTextarea('file-hashes-deny', 'Denied file hashes', 'Reject exact blob hashes. Stored in the configured hash denylist file.')
    ].join('');
    bindModerationInputs(card);
    return card;
  }

  function moderationTextarea(name, label, hint) {
    return [
      '<div class="field">',
      '<div class="field-main">',
      '<label for="list-' + name + '">' + escapeHtml(label) + '</label>',
      '<div class="field-controls">',
      '<textarea id="list-' + name + '" class="field-input-wide" spellcheck="false">' + escapeHtml(state.moderationLists[name] || '') + '</textarea>',
      '</div>',
      '</div>',
      '<p class="hint">' + escapeHtml(hint) + '</p>',
      '</div>'
    ].join('');
  }

  function bindModerationInputs(scope) {
    Object.keys(state.moderationLists).forEach(function (name) {
      var input = scope.querySelector('#list-' + name);
      if (!input) {
        return;
      }
      input.disabled = !state.bridge;
      input.addEventListener('input', function () {
        queueModerationSave(name);
      });
      input.addEventListener('blur', function () {
        queueModerationSave(name, 0);
      });
    });
  }

  async function loadModerationLists() {
    if (!state.bridge) {
      return;
    }
    var names = Object.keys(state.moderationLists);
    for (var i = 0; i < names.length; i += 1) {
      state.moderationLists[names[i]] = await backend('load-list', [state.envPath, names[i]]);
    }
  }

  async function saveModerationList(name) {
    if (!state.bridge) {
      return;
    }
    try {
      var value = document.getElementById('list-' + name).value || '';
      await backend('save-list', [state.envPath, name, toBase64(value)]);
      state.moderationLists[name] = value;
    } catch (error) {
      console.error(error);
      toast(error.message || 'Failed to save moderation list', 'bad');
    }
  }

  function renderDiagnosticsSection() {
    var wrap = document.createElement('div');
    wrap.className = 'section-panel autosave-panel';
    var stack = document.createElement('div');
    stack.className = 'section-stack diagnostics-stack';

    if (state.diagnosticsError) {
      var alert = document.createElement('p');
      alert.className = 'diagnostics-alert';
      alert.textContent = state.diagnosticsError;
      stack.appendChild(alert);
    }

    stack.appendChild(renderMirrorHealthPanel());
    stack.appendChild(renderRetentionHealthPanel());

    var actions = document.createElement('section');
    actions.className = 'section-panel';
    var checksActions = [
      '<button id="verify-run" class="action mini" type="button">Verify sample</button>'
    ].join('');
    actions.innerHTML = [
      renderCardHeadHtml(
        'Checks',
        'Inspect backend state, tail the relay log, and verify stored events.',
        checksActions,
        true
      ),
      '<pre id="diagnostics-doctor" class="mono">' + escapeHtml(state.doctor.trim() || 'No doctor output.') + '</pre>'
    ].join('');
    stack.appendChild(actions);

    var log = document.createElement('section');
    log.className = 'section-panel log-panel';
    log.innerHTML = [
      renderCardHeadHtml('Relay log', 'Last 200 log lines from the runtime log file.'),
      '<textarea id="relay-log" spellcheck="false" readonly></textarea>'
    ].join('');
    log.querySelector('#relay-log').value = state.log || 'Log is empty.';
    stack.appendChild(log);

    actions.querySelector('#verify-run').disabled = !state.bridge;
    actions.querySelector('#verify-run').addEventListener('click', async function () {
      try {
        var output = await backend('verify', [state.envPath, '100']);
        toast(output.trim() || 'Verification finished');
      } catch (error) {
        console.error(error);
        toast(error.message || 'Verification failed', 'bad');
      }
    });
    wrap.appendChild(stack);
    return wrap;
  }

  function renderMirrorHealthPanel() {
    var card = document.createElement('section');
    card.className = 'section-panel';
    card.innerHTML = renderCardHeadHtml(
      'Mirror health',
      'Live upstream connection state, last successful import, and last mirror errors.'
    );

    if (!state.bridge) {
      card.appendChild(renderDiagnosticsEmpty('Run Stonr in the desktop host to load live mirror health.'));
      return card;
    }
    if (state.diagnosticsLoading && !state.diagnosticsLoadedOnce) {
      card.appendChild(renderDiagnosticsLoading('Loading mirror health...'));
      return card;
    }
    if (!state.diagnosticsMirror.length) {
      var mirroringEnabled = !!getPath(state.config || {}, 'policy.enable_mirroring');
      var upstreamRelays = getPath(state.config || {}, 'relays_upstream');
      var upstreamCount = 0;
      if (Array.isArray(upstreamRelays)) {
        upstreamCount = upstreamRelays.filter(function (relay) {
          return String(relay || '').trim().length > 0;
        }).length;
      } else if (typeof upstreamRelays === 'string') {
        upstreamCount = upstreamRelays.split(',').filter(function (relay) {
          return String(relay || '').trim().length > 0;
        }).length;
      }
      if (!mirroringEnabled) {
        card.appendChild(renderDiagnosticsEmpty('Mirror import is disabled. Turn on Import from relays in Relay settings.'));
      } else if (upstreamCount === 0) {
        card.appendChild(renderDiagnosticsEmpty('No Source relays configured. Add at least one relay in Network settings.'));
      } else {
        card.appendChild(renderDiagnosticsEmpty('Waiting for first upstream mirror status. Keep relay running until at least one source relay connection completes.'));
      }
      return card;
    }

    var list = document.createElement('div');
    list.className = 'diagnostic-list';
    state.diagnosticsMirror.forEach(function (status) {
      var row = document.createElement('article');
      row.className = 'diagnostic-item';

      var head = document.createElement('div');
      head.className = 'diagnostic-item-head';
      var title = document.createElement('h4');
      title.textContent = shortRelayLabel(status.relay) + ' · ' + String(status.scope || 'mirror');
      title.title = String(status.relay || '');
      head.appendChild(title);
      head.appendChild(renderDiagnosticsStatePill(status.state));
      row.appendChild(head);

      var meta = document.createElement('p');
      meta.className = 'diagnostic-meta';
      meta.textContent = [
        'Last success ' + formatDiagnosticTimestamp(status.last_success_at),
        'Last event ' + formatDiagnosticTimestamp(status.last_seen_event_created_at),
        'EOSE ' + formatDiagnosticTimestamp(status.last_eose_at)
      ].join(' · ');
      row.appendChild(meta);

      if (status.last_error) {
        var error = document.createElement('p');
        error.className = 'diagnostic-inline-error';
        error.textContent = status.last_error;
        row.appendChild(error);
      }

      list.appendChild(row);
    });
    card.appendChild(list);
    return card;
  }

  function renderRetentionHealthPanel() {
    var card = document.createElement('section');
    card.className = 'section-panel';
    card.innerHTML = renderCardHeadHtml(
      'Retention health',
      'Current stored volume, configured caps, and the last prune result.'
    );

    if (!state.bridge) {
      card.appendChild(renderDiagnosticsEmpty('Run Stonr in the desktop host to load live retention health.'));
      return card;
    }
    if (state.diagnosticsLoading && !state.diagnosticsLoadedOnce) {
      card.appendChild(renderDiagnosticsLoading('Loading retention health...'));
      return card;
    }
    if (!state.diagnosticsRetention) {
      card.appendChild(renderDiagnosticsEmpty('No retention status yet.'));
      return card;
    }

    var rows = document.createElement('div');
    rows.className = 'diagnostic-kv';
    rows.appendChild(renderDiagnosticKvRow('State', String(state.diagnosticsRetention.state || 'unknown')));
    rows.appendChild(renderDiagnosticKvRow('Stored events', Number(state.diagnosticsRetention.current_events || 0).toLocaleString()));
    rows.appendChild(renderDiagnosticKvRow('Stored size', formatBytes(state.diagnosticsRetention.current_bytes || 0)));
    rows.appendChild(renderDiagnosticKvRow('Event cap', formatDiagnosticLimit(state.diagnosticsRetention.max_events, 'events')));
    rows.appendChild(renderDiagnosticKvRow('Size cap', formatDiagnosticBytesLimit(state.diagnosticsRetention.max_bytes)));
    rows.appendChild(renderDiagnosticKvRow('Last prune', formatDiagnosticTimestamp(state.diagnosticsRetention.last_prune_at)));
    rows.appendChild(renderDiagnosticKvRow('Removed last prune', formatDiagnosticOptionalCount(state.diagnosticsRetention.last_prune_removed)));
    card.appendChild(rows);

    if (state.diagnosticsRetention.warning) {
      var warning = document.createElement('p');
      warning.className = 'diagnostic-inline-warning';
      warning.textContent = state.diagnosticsRetention.warning;
      card.appendChild(warning);
    }
    if (state.diagnosticsRetention.last_error) {
      var error = document.createElement('p');
      error.className = 'diagnostic-inline-error';
      error.textContent = state.diagnosticsRetention.last_error;
      card.appendChild(error);
    }
    return card;
  }

  function renderDiagnosticsLoading(text) {
    var node = document.createElement('p');
    node.className = 'diagnostics-empty';
    node.innerHTML = '<span>' + escapeHtml(text) + '</span><span class="action-spinner" aria-hidden="true"></span>';
    return node;
  }

  function renderDiagnosticsEmpty(text) {
    var node = document.createElement('p');
    node.className = 'diagnostics-empty';
    node.textContent = text;
    return node;
  }

  function renderDiagnosticKvRow(labelText, valueText) {
    var row = document.createElement('div');
    row.className = 'diagnostic-kv-row';
    var label = document.createElement('span');
    label.className = 'diagnostic-kv-label';
    label.textContent = labelText;
    var value = document.createElement('span');
    value.className = 'diagnostic-kv-value';
    value.textContent = valueText;
    row.appendChild(label);
    row.appendChild(value);
    return row;
  }

  function renderDiagnosticsStatePill(value) {
    var pill = document.createElement('span');
    var stateText = String(value || 'unknown');
    var kind = 'neutral';
    if (stateText === 'running' || stateText === 'idle') {
      kind = 'good';
    } else if (stateText === 'error') {
      kind = 'bad';
    }
    pill.className = 'status-pill ' + kind;
    pill.textContent = stateText;
    return pill;
  }

  function shortRelayLabel(relay) {
    try {
      return new URL(String(relay || '')).host || String(relay || '');
    } catch (error) {
      return String(relay || '');
    }
  }

  function formatDiagnosticTimestamp(value) {
    var stamp = Number(value || 0);
    if (!isFinite(stamp) || stamp <= 0) {
      return 'never';
    }
    return formatEventTime(stamp);
  }

  function formatDiagnosticLimit(value, suffix) {
    if (value === null || typeof value === 'undefined' || value === '') {
      return 'unlimited';
    }
    return Number(value || 0).toLocaleString() + (suffix ? ' ' + suffix : '');
  }

  function formatDiagnosticBytesLimit(value) {
    if (value === null || typeof value === 'undefined' || value === '') {
      return 'unlimited';
    }
    return formatBytes(value);
  }

  function formatDiagnosticOptionalCount(value) {
    if (value === null || typeof value === 'undefined' || value === '') {
      return 'n/a';
    }
    return Number(value || 0).toLocaleString();
  }

  async function refreshStatus() {
    if (!state.bridge) {
      return;
    }
    try {
      state.status = parseKv(await backend('relay-status', [state.envPath]));
      syncHostStatusItemRelayIcon();
      renderRuntime();
      if (state.activeSection === 'general') {
        renderActiveSection(false);
      }
    } catch (error) {
      console.error(error);
      els.relayPill.textContent = 'Relay: status unavailable';
      els.relayPill.className = 'status-pill bad';
    }
  }

  async function refreshDoctor() {
    if (!state.bridge) {
      return;
    }
    state.doctor = await backend('doctor', [state.envPath]);
    state.doctorRefreshedAt = Date.now();
    els.doctorOutput.textContent = state.doctor.trim() || 'No backend output.';
  }

  function shouldRefreshDoctor() {
    return !state.doctorRefreshedAt || Date.now() - state.doctorRefreshedAt >= DOCTOR_REFRESH_INTERVAL_MS;
  }

  async function loadLog() {
    if (!state.bridge) {
      state.log = '';
      return;
    }
    state.log = await backend('tail-log', [state.envPath, '200']);
  }

  function queueDiagnosticsLoad() {
    if (!state.bridge || state.diagnosticsLoading) {
      return;
    }
    loadDiagnosticsStatus().then(function () {
      if (state.activeSection === 'diagnostics') {
        renderActiveSection();
      }
    }).catch(function (error) {
      console.error(error);
    });
  }

  async function loadDiagnosticsStatus() {
    if (!state.bridge) {
      state.diagnosticsMirror = [];
      state.diagnosticsRetention = null;
      state.diagnosticsError = '';
      state.diagnosticsLoading = false;
      state.diagnosticsLoadedOnce = true;
      return;
    }
    var hadSnapshot = state.diagnosticsLoadedOnce || state.diagnosticsMirror.length > 0 || !!state.diagnosticsRetention;
    state.diagnosticsLoading = true;
    try {
      var results = await Promise.all([
        backend('mirror-status', [state.envPath]),
        backend('retention-status', [state.envPath])
      ]);
      state.diagnosticsMirror = JSON.parse(results[0] || '[]');
      state.diagnosticsRetention = JSON.parse(results[1] || 'null');
      state.diagnosticsError = '';
      state.diagnosticsLoadedOnce = true;
    } catch (error) {
      console.error(error);
      if (!hadSnapshot) {
        state.diagnosticsMirror = [];
        state.diagnosticsRetention = null;
      }
      state.diagnosticsError = summarizeBackendError(error, 'Failed to load relay health');
      state.diagnosticsLoadedOnce = true;
    } finally {
      state.diagnosticsLoading = false;
    }
  }

  async function runRelayAction(command) {
    if (!state.bridge) {
      return;
    }
    try {
      state.relayBusyAction = command;
      syncRelayToggle(state.status || { status: 'stopped' });
      state.status = parseKv(await backend(command, [state.envPath]));
      if (command === 'relay-start' && (!state.status || state.status.status !== 'running')) {
        for (var attempt = 0; attempt < 8; attempt += 1) {
          await new Promise(function (resolve) { setTimeout(resolve, 250); });
          state.status = parseKv(await backend('relay-status', [state.envPath]));
          if (state.status && state.status.status === 'running') {
            break;
          }
        }
      }
      if (command === 'relay-start' && (!state.status || state.status.status !== 'running')) {
        throw new Error('Relay failed to start');
      }
      if (command === 'relay-stop' && state.status && state.status.status === 'running') {
        throw new Error('Relay failed to stop');
      }
      if (
        (command === 'relay-start' && state.status && state.status.status === 'running') ||
        (command === 'relay-stop' && state.status && state.status.status !== 'running')
      ) {
        state.relayBusyAction = '';
        syncRelayToggle(state.status);
        syncHostStatusItemRelayIcon();
      }
      await refreshDoctor();
      await loadEvents();
      if (state.activeSection === 'diagnostics') {
        await Promise.all([loadLog(), loadDiagnosticsStatus()]);
      }
      renderRuntime();
      renderActiveSection();
      toast(command.replace('relay-', '').replace(/-/g, ' ') + ' complete');
    } catch (error) {
      console.error(error);
      toast(error.message || 'Relay action failed', 'bad');
    } finally {
      state.relayBusyAction = '';
      syncRelayToggle(state.status || { status: 'stopped' });
      refreshLiveState().catch(function (error) {
        console.error(error);
      });
    }
  }

  async function applyEnvPath() {
    var next = els.envPath.value.trim();
    if (!next || next === state.envPath) {
      return;
    }
    state.envPath = next;
    await saveUiPref('env_path', state.envPath);
    await loadAll();
  }

  function textField(envKey, path, label, hint, format, dependsOn) {
    var tooltip = arguments[6];
    return { envKey: envKey, path: path, label: label, hint: hint, type: 'text', format: format, dependsOn: dependsOn || [], tooltip: tooltip || '' };
  }

  function browseTextField(envKey, path, label, hint, format, dependsOn) {
    var tooltip = arguments[6];
    return { envKey: envKey, path: path, label: label, hint: hint, type: 'text', format: format, browseDir: true, dependsOn: dependsOn || [], tooltip: tooltip || '' };
  }

  function radioField(envKey, path, label, options, dependsOn, tooltip) {
    return {
      envKey: envKey,
      path: path,
      label: label,
      type: 'radio',
      options: options,
      dependsOn: dependsOn || [],
      tooltip: tooltip || ''
    };
  }

  function numberField(envKey, path, label, hint, format, dependsOn) {
    var tooltip = arguments[6];
    return {
      envKey: envKey,
      path: path,
      label: label,
      hint: hint,
      type: 'number',
      format: format,
      dependsOn: dependsOn || [],
      defaultValue: defaultNumberValue(envKey),
      unit: defaultNumberUnit(envKey),
      tooltip: tooltip || ''
    };
  }

  function defaultNumberUnit(envKey) {
    switch (envKey) {
      case 'AUTH_MAX_AGE_SECS':
      case 'MAX_EVENT_AGE_SECS':
      case 'MAX_EVENT_FUTURE_SECS':
      case 'RATE_LIMIT_WINDOW_SECS':
        return 'sec';
      case 'FILE_MAX_BYTES':
      case 'MAX_EVENT_BYTES':
      case 'MAX_UPLOAD_BYTES_PER_WINDOW':
      case 'MAX_BLOB_BYTES_PER_PUBKEY':
        return 'bytes';
      case 'MAX_STORED_EVENT_BYTES':
        return 'MB';
      case 'MAX_LIMIT':
      case 'MAX_STORED_EVENTS':
        return 'events';
      case 'MAX_QUERIES_PER_WINDOW':
        return 'reads';
      case 'MAX_COUNTS_PER_WINDOW':
        return 'counts';
      case 'MAX_PUBLISHES_PER_WINDOW':
        return 'events';
      case 'MAX_UPLOADS_PER_WINDOW':
        return 'uploads';
      default:
        return '';
    }
  }

  function boolField(envKey, path, label, hint, dependsOn) {
    var extra = arguments[5];
    var opts;
    if (extra && typeof extra === 'object' && !Array.isArray(extra)) {
      opts = extra;
    } else {
      opts = { tooltip: extra || '' };
    }
    return {
      envKey: envKey,
      path: path,
      label: label,
      hint: hint,
      type: 'bool',
      dependsOn: dependsOn || [],
      tooltip: opts.tooltip || '',
      viewCommand: opts.viewCommand || '',
      viewLabel: opts.viewLabel || '',
      viewHint: opts.viewHint || ''
    };
  }

  function textareaField(envKey, path, label, hint, format, dependsOn) {
    var tooltip = arguments[6];
    return {
      envKey: envKey,
      path: path,
      label: label,
      hint: hint,
      type: 'textarea',
      format: format,
      dependsOn: dependsOn || [],
      lineDelimited: true,
      tooltip: tooltip || ''
    };
  }

  function selectField(envKey, path, label, hint, options, format, dependsOn) {
    var tooltip = arguments[7];
    return { envKey: envKey, path: path, label: label, hint: hint, type: 'select', options: options, format: format, dependsOn: dependsOn || [], tooltip: tooltip || '' };
  }

  function withExplicitSave(field, explicitSaveGroup) {
    field.explicitSaveGroup = explicitSaveGroup;
    return field;
  }

  function groupField(label) {
    return { type: 'group', label: label };
  }

  function noteField(text) {
    return { type: 'note', text: text };
  }

  function retentionApplyField() {
    return { type: 'retention-apply' };
  }

  function withFieldUi(field, options) {
    var ui = options || {};
    field.nipMaster = !!ui.nipMaster;
    field.nipChild = !!ui.nipChild;
    field.hideNipPill = !!ui.hideNipPill;
    field.collapseWhenUnavailable = !!ui.collapseWhenUnavailable;
    return field;
  }

  function getPath(source, path) {
    if (!path) {
      return '';
    }
    return path.split('.').reduce(function (acc, key) {
      return acc && typeof acc === 'object' ? acc[key] : undefined;
    }, source);
  }

  function setPath(target, path, value) {
    var keys = String(path || '').split('.');
    var cursor = target;
    keys.forEach(function (key, index) {
      if (!key) {
        return;
      }
      if (index === keys.length - 1) {
        cursor[key] = value;
        return;
      }
      if (!cursor[key] || typeof cursor[key] !== 'object') {
        cursor[key] = {};
      }
      cursor = cursor[key];
    });
  }

  function fieldByEnvKey(envKey) {
    var match = null;
    sections.some(function (section) {
      return section.fields.some(function (field) {
        if (field.envKey === envKey) {
          match = field;
          return true;
        }
        return false;
      });
    });
    return match;
  }

  function dependencySpecEnvKey(spec) {
    if (typeof spec === 'string') {
      return spec;
    }
    if (spec && typeof spec === 'object') {
      return spec.envKey || '';
    }
    return '';
  }

  function dependencySpecMet(spec) {
    var envKey = dependencySpecEnvKey(spec);
    if (!envKey) {
      return true;
    }
    var dependency = fieldByEnvKey(envKey);
    if (!dependency) {
      return true;
    }
    var value = resolvedFieldValue(dependency);
    if (typeof spec === 'string') {
      return !!value;
    }
    if (Object.prototype.hasOwnProperty.call(spec, 'equals')) {
      return String(value || '') === String(spec.equals);
    }
    return !!value;
  }

  function isFieldDependencyEnabled(field) {
    if (!field.dependsOn || !field.dependsOn.length) {
      return true;
    }
    return field.dependsOn.every(function (spec) {
      return dependencySpecMet(spec);
    });
  }

  function unmetFieldDependency(field) {
    if (!field.dependsOn || !field.dependsOn.length) {
      return '';
    }
    for (var index = 0; index < field.dependsOn.length; index += 1) {
      var spec = field.dependsOn[index];
      var envKey = dependencySpecEnvKey(spec);
      var dependency = fieldByEnvKey(envKey);
      if (!dependency) {
        continue;
      }
      if (!dependencySpecMet(spec)) {
        return spec;
      }
    }
    return '';
  }

  function dependencyReasonForKey(spec) {
    var envKey = typeof spec === 'string' ? spec : dependencySpecEnvKey(spec);
    switch (envKey) {
      case 'ENABLE_NIP42':
        return 'Requires relay login.';
      case 'MIRROR_MODE':
        if (spec && typeof spec === 'object' && spec.equals === 'site') {
          return 'Requires strict owner-author scope.';
        }
        return 'Requires normal relay scope.';
      default:
        return 'Required feature is off.';
    }
  }

  function disabledNipSupportKey(field) {
    var supportKeys = nipMasterByField[field.envKey] || [];
    for (var index = 0; index < supportKeys.length; index += 1) {
      var supportField = fieldByEnvKey(supportKeys[index]);
      if (supportField && !resolvedFieldValue(supportField)) {
        return supportKeys[index];
      }
    }
    return '';
  }

  function nipTokenForSupportKey(envKey) {
    switch (envKey) {
      case 'SUPPORT_NIP09':
        return 'NIP-09';
      case 'SUPPORT_NIP11':
        return 'NIP-11';
      case 'SUPPORT_NIP12':
        return 'NIP-12';
      case 'SUPPORT_NIP40':
        return 'NIP-40';
      case 'SUPPORT_NIP42':
        return 'NIP-42';
      case 'SUPPORT_NIP45':
        return 'NIP-45';
      case 'SUPPORT_NIP50':
        return 'NIP-50';
      case 'SUPPORT_NIP94':
        return 'NIP-94';
      case 'SUPPORT_NIP96':
        return 'NIP-96';
      case 'SUPPORT_NIP98':
        return 'NIP-98';
      case 'SUPPORT_NIP_B7':
        return 'NIP-B7';
      default:
        return 'This NIP';
    }
  }

  function syncFieldDependencies() {
    var suppressCollapseAnimation = !state.initialDependencySyncDone;
    Object.keys(state.fieldNodes).forEach(function (envKey) {
      var node = state.fieldNodes[envKey];
      var unmetDependency = unmetFieldDependency(node.field);
      var disabledNip = disabledNipSupportKey(node.field);
      var enabled = state.bridge && !unmetDependency && !disabledNip;
      var collapsedByDependency = !!(node.field.collapseWhenUnavailable && unmetDependency);
      if (node.field.collapseWhenUnavailable) {
        node.wrap.classList.toggle('field-no-transition', suppressCollapseAnimation);
        var expandedHeight = node.wrap.scrollHeight;
        if (expandedHeight > 0) {
          node.wrap.style.setProperty('--field-expanded-height', expandedHeight + 'px');
        }
      }
      if (node.field.type === 'radio') {
        Array.prototype.slice.call(node.input.querySelectorAll('input[type="radio"]')).forEach(function (radio) {
          radio.disabled = !enabled || radio.dataset.baseDisabled === '1';
        });
      } else {
        node.input.disabled = !enabled || node.input.dataset.baseDisabled === '1';
      }
      if (node.button) {
        node.button.disabled = !enabled || node.button.dataset.baseDisabled === '1';
      }
      node.wrap.classList.toggle('field-disabled', !enabled);
      node.wrap.classList.toggle('field-dependency-disabled', !!unmetDependency);
      node.wrap.classList.toggle('field-collapsed', collapsedByDependency);
      node.wrap.setAttribute('aria-disabled', enabled ? 'false' : 'true');
      node.wrap.setAttribute('aria-hidden', collapsedByDependency ? 'true' : 'false');
      if (node.nipPill) {
        var pillTitle = '';
        if (disabledNip) {
          var nipToken = nipTokenForSupportKey(disabledNip);
          var reason = nipToken + ' is disabled in NIP policies.';
          pillTitle = (nipSummaries[nipToken] ? nipSummaries[nipToken] + ' ' : '') + reason;
          node.nipPill.textContent = nipToken + ' disabled';
          node.nipPill.classList.add('nip-disabled');
          node.nipPill.title = pillTitle;
          node.label.title = (node.helpText ? node.helpText + ' ' : '') + reason;
          node.input.title = (node.helpText ? node.helpText + ' ' : '') + reason;
          if (node.hint) {
            node.hint.title = (node.helpText ? node.helpText + ' ' : '') + reason;
          }
        } else if (unmetDependency) {
          var dependencyReason = dependencyReasonForKey(unmetDependency);
          pillTitle = (nipSummaries[node.nipPill.dataset.nipToken] ? nipSummaries[node.nipPill.dataset.nipToken] + ' ' : '') + dependencyReason;
          node.nipPill.textContent = node.nipPill.dataset.nipToken;
          node.nipPill.classList.remove('nip-disabled');
          node.nipPill.title = pillTitle;
          node.label.title = (node.helpText ? node.helpText + ' ' : '') + dependencyReason;
          node.input.title = (node.helpText ? node.helpText + ' ' : '') + dependencyReason;
          if (node.hint) {
            node.hint.title = (node.helpText ? node.helpText + ' ' : '') + dependencyReason;
          }
        } else {
          pillTitle = nipSummaries[node.nipPill.dataset.nipToken] || node.nipPill.dataset.nipToken;
          node.nipPill.textContent = node.nipPill.dataset.nipToken;
          node.nipPill.classList.remove('nip-disabled');
          node.nipPill.title = pillTitle;
          node.label.title = node.helpText;
          node.input.title = node.helpText;
          if (node.hint) {
            node.hint.title = node.helpText;
          }
        }
      }
    });
    if (suppressCollapseAnimation) {
      state.initialDependencySyncDone = true;
      requestAnimationFrame(function () {
        Object.keys(state.fieldNodes).forEach(function (envKey) {
          var node = state.fieldNodes[envKey];
          if (node && node.field && node.field.collapseWhenUnavailable) {
            node.wrap.classList.remove('field-no-transition');
          }
        });
      });
    }
  }

  function formatList(value) {
    if (!value) {
      return '';
    }
    if (Array.isArray(value)) {
      return value.slice().sort().join(',');
    }
    return String(value);
  }

  function formatLineList(value) {
    if (!value) {
      return '';
    }
    if (Array.isArray(value)) {
      return value.slice().sort().join('\n');
    }
    return String(value)
      .split(',')
      .map(function (item) {
        return item.trim();
      })
      .filter(Boolean)
      .join('\n');
  }

  function formatNumberList(value) {
    if (!value) {
      return '';
    }
    if (Array.isArray(value)) {
      return value.slice().sort(function (left, right) {
        return Number(left) - Number(right);
      }).join(',');
    }
    return String(value);
  }

  function formatKeepMode(value) {
    if (!value) {
      return 'referenced';
    }
    return String(value).toLowerCase();
  }

  function formatSinceMode(value) {
    if (!value) {
      return 'cursor';
    }
    if (typeof value === 'string') {
      return value.toLowerCase();
    }
    if (typeof value === 'object' && typeof value.Fixed !== 'undefined') {
      return 'fixed:' + value.Fixed;
    }
    return 'cursor';
  }

  function defaultNumberValue(envKey) {
    switch (envKey) {
      case 'AUTH_MAX_AGE_SECS':
        return 600;
      case 'FILE_MAX_BYTES':
        return 32 * 1024 * 1024;
      case 'MAX_LIMIT':
        return 1000;
      case 'MAX_EVENT_BYTES':
        return 256 * 1024;
      case 'MAX_EVENT_AGE_SECS':
        return 31536000;
      case 'MAX_EVENT_FUTURE_SECS':
        return 900;
      case 'RATE_LIMIT_WINDOW_SECS':
        return 60;
      case 'MAX_QUERIES_PER_WINDOW':
        return 120;
      case 'MAX_COUNTS_PER_WINDOW':
        return 120;
      case 'MAX_PUBLISHES_PER_WINDOW':
        return 60;
      case 'MAX_UPLOADS_PER_WINDOW':
        return 16;
      case 'MAX_UPLOAD_BYTES_PER_WINDOW':
        return 128 * 1024 * 1024;
      case 'MAX_BLOB_BYTES_PER_PUBKEY':
        return 512 * 1024 * 1024;
      default:
        return null;
    }
  }

  function formatStoredEventMegabytes(value) {
    if (value === null || typeof value === 'undefined' || String(value).trim() === '') {
      return '';
    }
    var bytes = Number(value);
    if (!isFinite(bytes) || bytes <= 0) {
      return '';
    }
    return String(Math.round(bytes / (1024 * 1024)));
  }

  function serializeStoredEventMegabytes(value) {
    var trimmed = String(value || '').trim();
    if (!trimmed) {
      return '';
    }
    var megabytes = Number(trimmed);
    if (!isFinite(megabytes) || megabytes <= 0) {
      return '';
    }
    return String(Math.round(megabytes * 1024 * 1024));
  }

  function bindHttpOrigin() {
    var bind = rawEnvValueByKey('BIND_HTTP') || getPath(state.config || {}, 'bind_http') || '127.0.0.1:7777';
    var host = String(bind).trim() || '127.0.0.1:7777';
    if (host.indexOf('://') !== -1) {
      return host.replace(/\/+$/, '');
    }
    if (host === '0.0.0.0:7777' || host === '[::]:7777') {
      host = '127.0.0.1:7777';
    }
    return 'http://' + host;
  }

  function bindWsUrl() {
    var bind = rawEnvValueByKey('BIND_WS') || getPath(state.config || {}, 'bind_ws') || '127.0.0.1:7778';
    var host = String(bind).trim() || '127.0.0.1:7778';
    if (host === '0.0.0.0:7778' || host === '[::]:7778') {
      host = '127.0.0.1:7778';
    }
    if (/^wss?:\/\//i.test(host)) {
      return host.replace(/\/+$/, '');
    }
    if (/^https?:\/\//i.test(host)) {
      return host.replace(/^http/i, 'ws').replace(/\/+$/, '');
    }
    return 'ws://' + host.replace(/\/+$/, '');
  }

  function normalizeRelayWsUrl(value) {
    var text = String(value || '').trim();
    if (!text) {
      return '';
    }
    if (/^wss?:\/\//i.test(text)) {
      return text.replace(/\/+$/, '');
    }
    if (/^https?:\/\//i.test(text)) {
      return text.replace(/^http/i, 'ws').replace(/\/+$/, '');
    }
    return 'wss://' + text.replace(/\/+$/, '');
  }

  function relayPublicWsUrl() {
    var configured =
      rawEnvValueByKey('PUBLIC_RELAY_URL') ||
      getPath(state.config || {}, 'policy.public_relay_url') ||
      '';
    var normalized = normalizeRelayWsUrl(configured);
    return normalized || bindWsUrl();
  }

  function relayProfileUrl() {
    var relay = relayPublicWsUrl();
    if (/^wss:\/\//i.test(relay)) {
      return relay.replace(/^wss:/i, 'https:') + '/';
    }
    if (/^ws:\/\//i.test(relay)) {
      return relay.replace(/^ws:/i, 'http:') + '/';
    }
    return bindHttpOrigin().replace(/\/+$/, '') + '/';
  }

  function basename(path) {
    return String(path || '').split('/').filter(Boolean).pop() || '.env';
  }

  function renderCardHead(title, detail, hasActions) {
    var head = document.createElement('header');
    head.className = 'card-head' + (hasActions ? ' has-actions' : '');
    var heading = document.createElement('h3');
    appendNipText(heading, title);
    head.appendChild(heading);
    if (hasActions) {
      var actions = document.createElement('div');
      actions.className = 'card-head-actions';
      head.appendChild(actions);
    }
    if (detail) {
      var copy = document.createElement('p');
      copy.className = 'sub';
      appendNipText(copy, detail);
      head.appendChild(copy);
    }
    return head;
  }

  function renderCardHeadHtml(title, detail, actionsHtml, hasActions) {
    return [
      '<header class="card-head' + (hasActions ? ' has-actions' : '') + '">',
      '<h3>' + escapeHtml(title) + '</h3>',
      hasActions ? '<div class="card-head-actions">' + (actionsHtml || '') + '</div>' : '',
      detail ? '<p class="sub">' + escapeHtml(detail) + '</p>' : '',
      '</header>'
    ].join('');
  }

  function relayToggleSvg(iconName) {
    if (iconName === 'stop') {
      return '<svg class="relay-toggle-icon stop-icon" viewBox="0 0 16 16" role="presentation" focusable="false" aria-hidden="true"><rect x="3" y="3" width="10" height="10"></rect></svg>';
    }
    return '<svg class="relay-toggle-icon play-icon" viewBox="0 0 16 16" role="presentation" focusable="false" aria-hidden="true"><path d="M4 2.75v10.5c0 .53.57.87 1.03.61l8-5.25a.7.7 0 0 0 0-1.22l-8-5.25A.7.7 0 0 0 4 2.75z"></path></svg>';
  }

  function syncRelayToggle(status) {
    var running = status && status.status === 'running';
    if (state.relayBusyAction === 'relay-start') {
      els.relayToggle.innerHTML = '<span class="action-spinner" aria-hidden="true"></span>';
      els.relayToggle.className = 'action primary pending';
      els.relayToggle.title = 'Starting relay';
      els.relayToggle.setAttribute('aria-label', 'Starting relay');
      els.relayToggle.disabled = true;
      return;
    }
    if (state.relayBusyAction === 'relay-stop') {
      els.relayToggle.innerHTML = '<span class="action-spinner" aria-hidden="true"></span>';
      els.relayToggle.className = 'action primary running pending';
      els.relayToggle.title = 'Stopping relay';
      els.relayToggle.setAttribute('aria-label', 'Stopping relay');
      els.relayToggle.disabled = true;
      return;
    }
    els.relayToggle.innerHTML = relayToggleSvg(running ? 'stop' : 'play');
    els.relayToggle.className = 'action primary' + (running ? ' running' : '');
    els.relayToggle.title = running ? 'Stop relay' : 'Start relay';
    els.relayToggle.setAttribute('aria-label', running ? 'Stop relay' : 'Start relay');
    els.relayToggle.disabled = !state.bridge;
  }

  function runRelayToggle() {
    var status = state.status || {};
    return runRelayAction(status.status === 'running' ? 'relay-stop' : 'relay-start');
  }

  async function setStartupServiceEnabled(nextEnabled) {
    if (!state.bridge) {
      return;
    }
    state.startupServiceBusy = true;
    state.startupServicePendingAction = nextEnabled ? 'enable' : 'disable';
    renderActiveSection();
    try {
      var command = nextEnabled ? 'service-autostart-enable' : 'service-autostart-disable';
      var kv = parseKv(await backend(command, [state.envPath]));
      state.startupServiceEnabled = matchesBool(kv.enabled || '');
      state.startupServiceManager = String(kv.manager || state.startupServiceManager || 'none');
      toast(nextEnabled ? 'Startup service enabled.' : 'Startup service disabled.', 'good');
    } finally {
      state.startupServiceBusy = false;
      state.startupServicePendingAction = '';
    }
  }

  function queueEnvPathSave() {
    clearTimeout(state.envPathTimer);
    state.envPathTimer = setTimeout(function () {
      applyEnvPath().catch(function (error) {
        console.error(error);
        toast(error.message || 'Failed to update env path', 'bad');
      });
    }, 250);
  }

  function bindFieldAutosave(field, input) {
    var eventName = field.type === 'bool' || field.type === 'select' || field.type === 'radio' ? 'change' : 'input';
    input.addEventListener(eventName, function () {
      applyInputToState(field, input);
      if (field.explicitSaveGroup) {
        return;
      }
      queueFieldSave(field, input, eventName === 'change' ? 0 : 500);
    });
    if (eventName !== 'change') {
      input.addEventListener('blur', function () {
        applyInputToState(field, input);
        if (field.explicitSaveGroup) {
          return;
        }
        queueFieldSave(field, input, 0);
      });
    }
  }

  function queueFieldSave(field, input, delay) {
    var saveStatusTicket = 0;
    if (shouldTrackSaveStatus(field)) {
      state.saveStatusTicket += 1;
      saveStatusTicket = state.saveStatusTicket;
      hideSaveStatus();
      if (delay > 0) {
        state.saveStatusTimer = setTimeout(function () {
          showSaveStatusSaving(saveStatusTicket);
        }, delay);
      }
    }
    clearTimeout(state.fieldSaveTimers[field.envKey]);
    state.fieldSaveTargets[field.envKey] = { field: field, input: input };
    if (!delay || delay <= 0) {
      if (saveStatusTicket) {
        showSaveStatusSaving(saveStatusTicket);
      }
      state.pendingFieldSavePromises[field.envKey] = saveField(field, input, saveStatusTicket).catch(function (error) {
        console.error(error);
        if (saveStatusTicket === state.saveStatusTicket) {
          hideSaveStatus();
        }
        toast(error.message || 'Failed to save field', 'bad');
      }).finally(function () {
        delete state.pendingFieldSavePromises[field.envKey];
        delete state.fieldSaveTargets[field.envKey];
      });
      state.fieldSaveTimers[field.envKey] = null;
      return;
    }
    state.fieldSaveTimers[field.envKey] = setTimeout(function () {
      if (saveStatusTicket) {
        showSaveStatusSaving(saveStatusTicket);
      }
      state.pendingFieldSavePromises[field.envKey] = saveField(field, input, saveStatusTicket).catch(function (error) {
        console.error(error);
        if (saveStatusTicket === state.saveStatusTicket) {
          hideSaveStatus();
        }
        toast(error.message || 'Failed to save field', 'bad');
      }).finally(function () {
        delete state.pendingFieldSavePromises[field.envKey];
        delete state.fieldSaveTargets[field.envKey];
      });
    }, delay);
  }

  async function saveField(field, input, saveStatusTicket) {
    if (!state.bridge) {
      return;
    }
    var nextValue = serializeInput(field, input);
    var appliedValue = nextValue;
    if (field.envKey === 'STORE_ROOT' && !appliedValue) {
      appliedValue = String(defaultFieldValue(field) || '');
      input.value = appliedValue;
      nextValue = appliedValue;
      setConfigValue(field, appliedValue);
    }
    if (input.dataset.savedValue === nextValue) {
      return;
    }
    var ticket = state.nextSaveTicket + 1;
    state.nextSaveTicket = ticket;
    state.saveQueue = state.saveQueue.catch(function () {
      return null;
    }).then(async function () {
      var strictOwnerValue = '';
      await backend('save-env', [state.envPath, field.envKey, nextValue]);
      if (field.envKey === 'OWNER_PUBKEYS') {
        // Keep strict one-site scope aligned with owner authors now that
        // owners are the single source of truth in the Pinned section.
        strictOwnerValue = nextValue;
        await backend('save-env', [state.envPath, 'MIRROR_SITE_AUTHOR', nextValue]);
      } else if (field.envKey === 'MIRROR_MODE' && String(nextValue) === 'site') {
        var ownerNode = state.fieldNodes.OWNER_PUBKEYS;
        strictOwnerValue = ownerNode ? serializeInput(ownerNode.field, ownerNode.input) : String((state.envValues && state.envValues.OWNER_PUBKEYS) || '');
        await backend('save-env', [state.envPath, 'MIRROR_SITE_AUTHOR', strictOwnerValue]);
      }
      if (!state.envValues || typeof state.envValues !== 'object') {
        state.envValues = {};
      }
      state.envValues[field.envKey] = nextValue;
      if (field.envKey === 'OWNER_PUBKEYS') {
        state.envValues.MIRROR_SITE_AUTHOR = nextValue;
      } else if (field.envKey === 'MIRROR_MODE' && String(nextValue) === 'site') {
        state.envValues.MIRROR_SITE_AUTHOR = strictOwnerValue;
      }
      if (ticket < state.appliedSaveTicket) {
        return;
      }
      state.appliedSaveTicket = ticket;
      input.dataset.savedValue = nextValue;
      syncFieldDependencies();
      if (saveStatusTicket) {
        showSaveStatusSaved(saveStatusTicket);
      }
    });
    await state.saveQueue;
  }

  async function flushPendingFieldSaves() {
    if (!state.bridge) {
      return;
    }
    hideSaveStatus();
    Object.keys(state.fieldSaveTimers).forEach(function (envKey) {
      var timer = state.fieldSaveTimers[envKey];
      if (timer) {
        clearTimeout(timer);
        state.fieldSaveTimers[envKey] = null;
      }
      var target = state.fieldSaveTargets[envKey] || state.fieldNodes[envKey];
      if (!target) {
        return;
      }
      if (target.field.explicitSaveGroup) {
        delete state.fieldSaveTargets[envKey];
        return;
      }
      var nextValue = serializeInput(target.field, target.input);
      if (target.input.dataset.savedValue === nextValue && !state.pendingFieldSavePromises[envKey]) {
        return;
      }
      if (!state.pendingFieldSavePromises[envKey]) {
        state.pendingFieldSavePromises[envKey] = saveField(target.field, target.input).catch(function (error) {
          console.error(error);
        }).finally(function () {
          delete state.pendingFieldSavePromises[envKey];
          delete state.fieldSaveTargets[envKey];
        });
      }
    });
    await Promise.all(Object.keys(state.pendingFieldSavePromises).map(function (envKey) {
      return state.pendingFieldSavePromises[envKey];
    }));
  }

  async function browseFieldDirectory(field, input, button) {
    if (!state.bridge) {
      return;
    }
    var currentValue = String(input.value || '').trim();
    button.disabled = true;
    try {
      var chosen = String(await backend('choose-dir', [currentValue || input.dataset.savedValue || ''])).trim();
      if (!chosen) {
        return;
      }
      input.value = chosen;
      await saveField(field, input);
    } finally {
      button.disabled = !state.bridge;
    }
  }

  async function applyRetentionSettings() {
    if (!state.bridge || state.retentionBusy) {
      return;
    }
    var keys = ['MAX_STORED_EVENT_BYTES', 'MAX_STORED_EVENTS'];
    state.retentionBusy = true;
    renderActiveSection();
    try {
      for (var index = 0; index < keys.length; index += 1) {
        var key = keys[index];
        var target = state.fieldNodes[key];
        if (!target) {
          continue;
        }
        clearTimeout(state.fieldSaveTimers[key]);
        state.fieldSaveTimers[key] = null;
        delete state.fieldSaveTargets[key];
        await saveField(target.field, target.input);
      }
      await backend('apply-retention', [state.envPath]);
      state.eventsLoadedOnce = false;
      toast('Retention apply started', 'good');
    } finally {
      state.retentionBusy = false;
      renderActiveSection();
    }
  }

  async function hydrateAfterBoot() {
    var skipConfigReload = !!arguments[0];
    var editSeq = state.configEditSeq;
    try {
      if (!skipConfigReload) {
        await loadConfigForBootFrame(editSeq);
      }
      if (editSeq !== state.configEditSeq) {
        return;
      }
      syncFieldDependencies();
      if (!skipConfigReload) {
        renderActiveSection();
      }
      await loadModerationLists();
      if (state.activeSection === 'moderation') {
        renderActiveSection();
        return;
      }
      if (state.activeSection === 'diagnostics') {
        await Promise.all([loadLog(), loadDiagnosticsStatus()]);
        renderActiveSection();
      }
    } catch (error) {
      console.error(error);
      if (!state.config) {
        state.config = {};
      }
      syncFieldDependencies();
      renderActiveSection();
    }
  }

  async function loadConfigForBootFrame(editSeq) {
    var loadedConfig = JSON.parse(await backend('load-config', [state.envPath]));
    if (editSeq !== state.configEditSeq) {
      return false;
    }
    state.config = loadedConfig;
    return true;
  }

  function queueModerationSave(name, delay) {
    clearTimeout(state.listSaveTimers[name]);
    state.listSaveTimers[name] = setTimeout(function () {
      saveModerationList(name);
    }, typeof delay === 'number' ? delay : 350);
  }

  async function refreshLiveState() {
    if (!state.bridge) {
      return;
    }
    if (state.refreshInFlight) {
      state.refreshQueued = true;
      return;
    }
    state.refreshInFlight = true;
    try {
      await refreshStatus();
      if (shouldRefreshDoctor()) {
        await refreshDoctor();
      }
      if (state.activeSection === 'events') {
        await loadEvents();
        renderActiveSection();
        return;
      }
      if (state.activeSection === 'diagnostics') {
        await Promise.all([loadLog(), loadDiagnosticsStatus()]);
        renderActiveSection();
      }
    } finally {
      state.refreshInFlight = false;
      if (state.refreshQueued) {
        state.refreshQueued = false;
        refreshLiveState().catch(function (error) {
          console.error(error);
        });
      }
    }
  }

  function startRefreshLoop() {
    if (!state.bridge) {
      return;
    }
    if (state.refreshTimer) {
      clearInterval(state.refreshTimer);
    }
    state.refreshTimer = setInterval(function () {
      if (document.visibilityState === 'visible') {
        refreshLiveState().catch(function (error) {
          console.error(error);
        });
      }
    }, LIVE_REFRESH_INTERVAL_MS);
  }

  function toBase64(value) {
    var bytes = new TextEncoder().encode(String(value || ''));
    var binary = '';
    bytes.forEach(function (byte) {
      binary += String.fromCharCode(byte);
    });
    return btoa(binary);
  }

  function escapeHtml(value) {
    return String(value || '')
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');
  }
})();
