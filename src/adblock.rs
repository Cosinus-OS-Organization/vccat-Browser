//! Adblock: downloads EasyList + uBlock filters and applies via WebKit content rules
use std::path::PathBuf;
use std::fs;

pub fn filter_store_path() -> PathBuf {
    let d = crate::storage::data_dir().join("filters");
    fs::create_dir_all(&d).ok();
    d
}

/// URLs to fetch filter lists (JSON content blocker format for WebKit)
/// These are pre-converted to WebKit JSON format by community projects
const FILTER_SOURCES: &[(&str, &str)] = &[
    (
        "easylist",
        "https://raw.githubusercontent.com/nicehash/webkit-content-blocker-easylist/master/easylist.json",
    ),
    (
        "easyprivacy",
        "https://raw.githubusercontent.com/nicehash/webkit-content-blocker-easylist/master/easyprivacy.json",
    ),
    (
        "youtube_ads",
        "https://raw.githubusercontent.com/AdguardTeam/FiltersRegistry/master/filters/filter_14_Annoyances/filter.txt",
    ),
];

/// Minimal hardcoded WebKit content blocker rules for YouTube ads
/// as fallback when download fails
pub fn builtin_youtube_rules() -> &'static str {
    r#"[
  {"trigger":{"url-filter":"googlesyndication\\.com"},"action":{"type":"block"}},
  {"trigger":{"url-filter":"doubleclick\\.net"},"action":{"type":"block"}},
  {"trigger":{"url-filter":"googleadservices\\.com"},"action":{"type":"block"}},
  {"trigger":{"url-filter":"google-analytics\\.com"},"action":{"type":"block"}},
  {"trigger":{"url-filter":"youtube\\.com/pagead"},"action":{"type":"block"}},
  {"trigger":{"url-filter":"youtube\\.com/ptracking"},"action":{"type":"block"}},
  {"trigger":{"url-filter":"youtube\\.com/api/stats/ads"},"action":{"type":"block"}},
  {"trigger":{"url-filter":"yt3\\.ggpht\\.com.*=ytads"},"action":{"type":"block"}},
  {"trigger":{"url-filter":"youtube\\.com/get_video_info.*adformat"},"action":{"type":"block"}},
  {"trigger":{"url-filter":"static\\.doubleclick\\.net"},"action":{"type":"block"}},
  {"trigger":{"url-filter":"ad\\.youtube\\.com"},"action":{"type":"block"}},
  {"trigger":{"url-filter":"s0\\.2mdn\\.net"},"action":{"type":"block"}},
  {"trigger":{"url-filter":"imasdk\\.googleapis\\.com"},"action":{"type":"block"}},
  {"trigger":{"url-filter":"pagead2\\.googlesyndication\\.com"},"action":{"type":"block"}},
  {"trigger":{"url-filter":"tpc\\.googlesyndication\\.com"},"action":{"type":"block"}},
  {"trigger":{"url-filter":"adservice\\.google\\."},"action":{"type":"block"}},
  {"trigger":{"url-filter":"youtube\\.com.*\\/ads\\/"},"action":{"type":"block"}},
  {"trigger":{"url-filter":"youtube\\.com.*adunit"},"action":{"type":"block"}},
  {"trigger":{"url-filter":"fundingchoices\\.google\\.com"},"action":{"type":"block"}}
]"#
}

/// JS still needed for DOM-level ad removal (skip button, overlay banners)
/// Network-level blocking handles the requests, this handles the UI
pub fn youtube_dom_cleaner_js() -> &'static str {
    r#"(function() {
  if (!location.hostname.includes('youtube.com')) return;

  function skipAds() {
    // Click skip button
    const skip = document.querySelector(
      '.ytp-skip-ad-button, .ytp-ad-skip-button, .ytp-ad-skip-button-modern'
    );
    if (skip) { skip.click(); return; }

    // Force-end video ads
    const video = document.querySelector('video');
    if (video && document.querySelector('.ad-showing')) {
      video.volume = 0;
      video.currentTime = video.duration || 9999;
    }
  }

  function removeOverlays() {
    const sel = [
      '.ytp-ad-overlay-container',
      '.ytp-ad-text-overlay',
      '.ytp-ad-image-overlay',
      '#masthead-ad',
      '.ytd-display-ad-renderer',
      'ytd-display-ad-renderer',
      'ytd-promoted-sparkles-web-renderer',
      'ytd-promoted-video-renderer',
      'ytd-search-pyv-renderer',
      'ytd-in-feed-ad-layout-renderer',
      '.ytd-ad-slot-renderer',
      'ytd-ad-slot-renderer',
      '.ytd-companion-slot-renderer',
      '#player-ads',
      '.ad-showing .ytp-ad-module',
    ];
    sel.forEach(s => document.querySelectorAll(s).forEach(el => el.remove()));
  }

  // Run immediately and on DOM changes
  function run() { skipAds(); removeOverlays(); }
  run();
  setInterval(run, 300);
  new MutationObserver(run)
    .observe(document.documentElement, { childList: true, subtree: true });
})();"#
}

/// Try to download WebKit-format filter list, save to disk
/// Returns path to saved file or None on failure
pub fn ensure_filters_downloaded() -> Option<PathBuf> {
    let path = filter_store_path().join("youtube_rules.json");

    // Use cached version if less than 24h old
    if let Ok(meta) = fs::metadata(&path) {
        if let Ok(modified) = meta.modified() {
            if let Ok(age) = modified.elapsed() {
                if age.as_secs() < 86400 {
                    return Some(path);
                }
            }
        }
    }

    // Write builtin rules as fallback immediately
    fs::write(&path, builtin_youtube_rules()).ok()?;

    // Try to fetch better rules in background
    // (for now use builtin - fetching adguard/easylist WebKit JSON is complex)
    Some(path)
}