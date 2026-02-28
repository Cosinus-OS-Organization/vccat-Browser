use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder},
    window::WindowBuilder,
};
use wry::WebViewBuilder;

#[cfg(target_os = "linux")]
use tao::platform::unix::WindowExtUnix;
#[cfg(target_os = "linux")]
use wry::WebViewBuilderExtUnix;

#[derive(Debug, Clone)]
enum UserEvent {
    // toolbar
    Navigate(String),
    GoBack,
    GoForward,
    Reload,
    // tabs
    NewTab,
    CloseTab(usize),
    SwitchTab(usize),
    // from page webview
    PageUrlChanged(usize, String),
    PageFaviconChanged(usize, String),
    PageTitleChanged(usize, String),
}

fn normalize_url(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() { return "about:blank".into(); }
    if raw.starts_with("about:") || raw.starts_with("data:") { return raw.into(); }
    if !raw.starts_with("http://") && !raw.starts_with("https://") {
        if raw.contains('.') && !raw.contains(' ') {
            return format!("https://{}", raw);
        }
        return format!(
            "https://search.brave.com/search?q={}",
            urlencoding::encode(raw)
        );
    }
    raw.into()
}

// JS injected into every page to grab favicon and URL changes
fn page_init_js(tab_idx: usize) -> String {
    format!(r#"
(function() {{
    const TAB = {tab_idx};

    function sendFavicon() {{
        let favicon = '';
        // try link[rel~=icon]
        const links = document.querySelectorAll('link[rel~="icon"], link[rel~="shortcut"]');
        for (const l of links) {{
            if (l.href) {{ favicon = l.href; break; }}
        }}
        // fallback: /favicon.ico
        if (!favicon) favicon = location.origin + '/favicon.ico';
        if (favicon) window.ipc.postMessage('favicon:' + favicon);
    }}

    // send url on navigation
    window.ipc.postMessage('url:' + location.href);

    // send title
    if (document.title) window.ipc.postMessage('title:' + document.title);

    // send favicon when dom ready
    if (document.readyState === 'loading') {{
        document.addEventListener('DOMContentLoaded', sendFavicon);
    }} else {{
        sendFavicon();
    }}

    // observe title changes
    const obs = new MutationObserver(() => {{
        window.ipc.postMessage('title:' + document.title);
    }});
    const titleEl = document.querySelector('title');
    if (titleEl) obs.observe(titleEl, {{ childList: true }});
}})();
"#, tab_idx = tab_idx)
}

fn sidebar_html() -> &'static str {
    r#"<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<style>
  * { margin:0; padding:0; box-sizing:border-box; }
  html, body {
    width:52px; height:100%;
    background:#0e0e0e;
    overflow:hidden;
    display:flex; flex-direction:column;
    align-items:center;
    border-right:1px solid #1c1c1c;
  }
  #tabs {
    flex:1; width:100%;
    overflow-y:auto; overflow-x:hidden;
    display:flex; flex-direction:column;
    align-items:center;
    padding:8px 0; gap:4px;
  }
  #tabs::-webkit-scrollbar { width:2px; }
  #tabs::-webkit-scrollbar-thumb { background:#252525; border-radius:2px; }

  .tab {
    position:relative;
    width:36px; height:36px;
    border-radius:8px;
    cursor:pointer;
    display:flex; align-items:center; justify-content:center;
    flex-shrink:0;
    border:1px solid transparent;
    transition:background 0.1s, border-color 0.1s;
  }
  .tab:hover { background:#181818; border-color:#262626; }
  .tab.active {
    background:#1e1e1e;
    border-color:#2e2e2e;
    box-shadow: inset 0 0 0 1px #333;
  }
  .tab img {
    width:18px; height:18px;
    border-radius:3px;
    object-fit:contain;
  }
  .tab .fb {
    width:20px; height:20px;
    border-radius:5px;
    background:#1e1e1e;
    border:1px solid #2a2a2a;
    display:flex; align-items:center; justify-content:center;
    font-size:10px; font-weight:600;
    color:#666;
    font-family: 'JetBrains Mono', monospace;
    text-transform:uppercase;
  }
  .tab .x {
    display:none;
    position:absolute;
    top:-5px; right:-5px;
    width:14px; height:14px;
    background:#111;
    border:1px solid #2a2a2a;
    border-radius:50%;
    color:#777;
    font-size:9px;
    align-items:center; justify-content:center;
    cursor:pointer;
    z-index:10;
    line-height:1;
  }
  .tab:hover .x { display:flex; }
  .x:hover { background:#1e1e1e; color:#ccc !important; }

  #add {
    width:36px; height:36px;
    border-radius:8px;
    border:1px dashed #222;
    background:none; color:#3a3a3a;
    font-size:20px; cursor:pointer;
    display:flex; align-items:center; justify-content:center;
    transition:all 0.12s;
    flex-shrink:0;
    margin:4px 0 8px;
  }
  #add:hover { border-color:#3a3a3a; color:#777; background:#141414; }
</style>
</head>
<body>
<div id="tabs"></div>
<button id="add" title="Nowa karta" onclick="send('new')">+</button>
<script>
  let state = { tabs: [], active: 0 };

  function send(m) { window.ipc.postMessage(m); }

  function render() {
    const c = document.getElementById('tabs');
    c.innerHTML = '';
    state.tabs.forEach((t, i) => {
      const el = document.createElement('div');
      el.className = 'tab' + (i === state.active ? ' active' : '');
      el.title = t.title || t.url || 'Nowa karta';

      if (t.favicon) {
        const img = document.createElement('img');
        img.src = t.favicon;
        img.onerror = () => img.replaceWith(makeFb(t));
        el.appendChild(img);
      } else {
        el.appendChild(makeFb(t));
      }

      const x = document.createElement('div');
      x.className = 'x';
      x.textContent = '×';
      x.onclick = e => { e.stopPropagation(); send('close:' + i); };
      el.appendChild(x);

      el.onclick = () => send('switch:' + i);
      c.appendChild(el);
    });
  }

  function makeFb(t) {
    const d = document.createElement('div');
    d.className = 'fb';
    try {
      const h = new URL(t.url || 'about:blank').hostname;
      d.textContent = h ? h[0] : '?';
    } catch { d.textContent = '?'; }
    return d;
  }

  function update(newState) {
    state = newState;
    render();
  }
</script>
</body>
</html>"#
}

fn toolbar_html() -> &'static str {
    r#"<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<style>
  * { margin:0; padding:0; box-sizing:border-box; }
  html, body {
    height:100%; background:#111;
    overflow:hidden;
    display:flex; align-items:center;
    gap:7px; padding:0 10px;
    border-bottom:1px solid #1c1c1c;
    font-family:'JetBrains Mono','Fira Code',monospace;
  }
  button {
    background:none; border:1px solid #222; color:#666;
    width:28px; height:28px; border-radius:6px; cursor:pointer;
    font-size:13px; display:flex; align-items:center; justify-content:center;
    transition:all 0.1s; flex-shrink:0;
  }
  button:hover  { background:#1a1a1a; color:#ccc; border-color:#333; }
  button:active { transform:scale(0.92); }
  #url {
    flex:1; background:#161616; border:1px solid #222; color:#ccc;
    padding:5px 12px; border-radius:7px; font-size:12px;
    font-family:inherit; outline:none; letter-spacing:0.02em;
    transition:border-color 0.15s;
  }
  #url:focus { border-color:#3a3a3a; color:#eee; }
  #url::placeholder { color:#2e2e2e; }
</style>
</head>
<body>
  <button title="Wstecz"  onclick="s('back')">&#8592;</button>
  <button title="Dalej"   onclick="s('fwd')">&#8594;</button>
  <button title="Odśwież" onclick="s('reload')">&#8635;</button>
  <input id="url" type="text" placeholder="Adres lub wyszukaj..."
    spellcheck="false"
    onkeydown="if(event.key==='Enter'){s('nav:'+this.value);this.blur();}"
    onfocus="this.select()" />
<script>
  function s(m) { window.ipc.postMessage(m); }
  function setUrl(u) {
    const el = document.getElementById('url');
    if (document.activeElement !== el) el.value = u;
  }
</script>
</body>
</html>"#
}

struct Tab {
    url: String,
    title: String,
    favicon: Option<String>,
}

fn main() -> wry::Result<()> {
    let event_loop: EventLoop<UserEvent> = EventLoopBuilder::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title("vccat browser")
        .with_inner_size(tao::dpi::LogicalSize::new(1360, 860))
        .with_decorations(true)
        .build(&event_loop)
        .unwrap();

    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::*;

        let root = window.default_vbox().unwrap();
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        root.pack_start(&hbox, true, true, 0);

        // sidebar
        let sidebar_gtk = gtk::Box::new(gtk::Orientation::Vertical, 0);
        sidebar_gtk.set_size_request(52, -1);

        // right: toolbar + pages
        let right = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let toolbar_gtk = gtk::Box::new(gtk::Orientation::Vertical, 0);
        toolbar_gtk.set_size_request(-1, 44);
        let pages_gtk = gtk::Box::new(gtk::Orientation::Vertical, 0);
        pages_gtk.set_vexpand(true);

        right.pack_start(&toolbar_gtk, false, false, 0);
        right.pack_start(&pages_gtk, true, true, 0);
        hbox.pack_start(&sidebar_gtk, false, false, 0);
        hbox.pack_start(&right, true, true, 0);
        root.show_all();

        // ── Sidebar WebView ──
        let ps = proxy.clone();
        let sidebar_wv = WebViewBuilder::new_gtk(&sidebar_gtk)
            .with_html(sidebar_html())
            .with_ipc_handler(move |msg: wry::http::Request<String>| {
                let b = msg.body().as_str();
                if b == "new" {
                    let _ = ps.send_event(UserEvent::NewTab);
                } else if let Some(i) = b.strip_prefix("close:").and_then(|s| s.parse::<usize>().ok()) {
                    let _ = ps.send_event(UserEvent::CloseTab(i));
                } else if let Some(i) = b.strip_prefix("switch:").and_then(|s| s.parse::<usize>().ok()) {
                    let _ = ps.send_event(UserEvent::SwitchTab(i));
                }
            })
            .with_background_color((14, 14, 14, 255))
            .build()?;

        // ── Toolbar WebView ──
        let pt = proxy.clone();
        let toolbar_wv = WebViewBuilder::new_gtk(&toolbar_gtk)
            .with_html(toolbar_html())
            .with_ipc_handler(move |msg: wry::http::Request<String>| {
                let b = msg.body().as_str();
                if let Some(url) = b.strip_prefix("nav:") {
                    let _ = pt.send_event(UserEvent::Navigate(normalize_url(url)));
                } else if b == "back"   { let _ = pt.send_event(UserEvent::GoBack);    }
                else if b == "fwd"    { let _ = pt.send_event(UserEvent::GoForward);  }
                else if b == "reload" { let _ = pt.send_event(UserEvent::Reload);     }
            })
            .with_background_color((17, 17, 17, 255))
            .build()?;

        // ── helper: build a page WebView for given tab index ──
        let make_page_wv = |idx: usize, url: &str, pages_box: &gtk::Box| -> wry::Result<wry::WebView> {
            let pu_nav = proxy.clone();
            let pu_ipc = proxy.clone();
            let pf     = proxy.clone();
            let pt2    = proxy.clone();
            let init_js = page_init_js(idx);

            let wv = WebViewBuilder::new_gtk(pages_box)
                .with_url(url)
                .with_initialization_script(&init_js)
                .with_navigation_handler(move |url| {
                    let _ = pu_nav.send_event(UserEvent::PageUrlChanged(idx, url));
                    true
                })
                .with_ipc_handler(move |msg: wry::http::Request<String>| {
                    let b = msg.body().to_string();
                    if let Some(u) = b.strip_prefix("url:") {
                        let _ = pu_ipc.send_event(UserEvent::PageUrlChanged(idx, u.to_string()));
                    } else if let Some(f) = b.strip_prefix("favicon:") {
                        let _ = pf.send_event(UserEvent::PageFaviconChanged(idx, f.to_string()));
                    } else if let Some(t) = b.strip_prefix("title:") {
                        let _ = pt2.send_event(UserEvent::PageTitleChanged(idx, t.to_string()));
                    }
                })
                .build()?;
            Ok(wv)
        };

        // ── Initial tab ──
        let mut tabs: Vec<Tab> = vec![Tab {
            url: "about:blank".into(),
            title: String::new(),
            favicon: None,
        }];
        let mut active: usize = 0;

        // Vec of (gtk_box, webview) per tab
        let first_page_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        first_page_box.set_vexpand(true);
        pages_gtk.pack_start(&first_page_box, true, true, 0);
        first_page_box.show_all();

        let first_wv = make_page_wv(0, "about:blank", &first_page_box)?;
        let mut page_entries: Vec<(gtk::Box, wry::WebView)> = vec![(first_page_box, first_wv)];

        // helper closures captured below in event loop need owned data
        // we'll move everything into the event loop closure

        fn sync_sidebar(sidebar_wv: &wry::WebView, tabs: &[Tab], active: usize) {
            let items: Vec<String> = tabs.iter().map(|t| {
                let fav = t.favicon.as_deref().unwrap_or("");
                format!(
                    r#"{{"url":"{}","favicon":"{}","title":"{}"}}"#,
                    t.url.replace('"', "\\\""),
                    fav.replace('"', "\\\""),
                    t.title.replace('"', "\\\""),
                )
            }).collect();
            let js = format!("update({{tabs:[{}],active:{}}});", items.join(","), active);
            let _ = sidebar_wv.evaluate_script(&js);
        }

        sync_sidebar(&sidebar_wv, &tabs, active);

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;

            match event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested, ..
                } => *control_flow = ControlFlow::Exit,

                Event::UserEvent(e) => {
                    use gtk::prelude::*;
                    match e {
                        // ── Navigation ──
                        UserEvent::Navigate(url) => {
                            tabs[active].url = url.clone();
                            tabs[active].favicon = None;
                            let _ = page_entries[active].1.load_url(&url);
                            let js = format!("setUrl('{}');", url.replace('\'', "\\'"));
                            let _ = toolbar_wv.evaluate_script(&js);
                            sync_sidebar(&sidebar_wv, &tabs, active);
                        }
                        UserEvent::GoBack => {
                            let _ = page_entries[active].1.evaluate_script("history.back()");
                        }
                        UserEvent::GoForward => {
                            let _ = page_entries[active].1.evaluate_script("history.forward()");
                        }
                        UserEvent::Reload => {
                            let _ = page_entries[active].1.evaluate_script("location.reload()");
                        }

                        // ── Tabs ──
                        UserEvent::NewTab => {
                            let idx = tabs.len();
                            tabs.push(Tab { url: "about:blank".into(), title: String::new(), favicon: None });

                            // new gtk box
                            let new_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
                            new_box.set_vexpand(true);
                            pages_gtk.pack_start(&new_box, true, true, 0);

                            // hide all others
                            for (b, _) in &page_entries {
                                b.hide();
                            }
                            new_box.show_all();

                            // create webview — need proxy clone
                            let pu2_nav = proxy.clone();
                            let pu2_ipc = proxy.clone();
                            let pf2     = proxy.clone();
                            let pt3     = proxy.clone();
                            let init_js = page_init_js(idx);
                            let wv = WebViewBuilder::new_gtk(&new_box)
                                .with_url("about:blank")
                                .with_initialization_script(&init_js)
                                .with_navigation_handler(move |url| {
                                    let _ = pu2_nav.send_event(UserEvent::PageUrlChanged(idx, url));
                                    true
                                })
                                .with_ipc_handler(move |msg: wry::http::Request<String>| {
                                    let b = msg.body().to_string();
                                    if let Some(u) = b.strip_prefix("url:") {
                                        let _ = pu2_ipc.send_event(UserEvent::PageUrlChanged(idx, u.to_string()));
                                    } else if let Some(f) = b.strip_prefix("favicon:") {
                                        let _ = pf2.send_event(UserEvent::PageFaviconChanged(idx, f.to_string()));
                                    } else if let Some(t) = b.strip_prefix("title:") {
                                        let _ = pt3.send_event(UserEvent::PageTitleChanged(idx, t.to_string()));
                                    }
                                })
                                .build()
                                .unwrap();

                            page_entries.push((new_box, wv));
                            active = idx;

                            let _ = toolbar_wv.evaluate_script("setUrl('');");
                            sync_sidebar(&sidebar_wv, &tabs, active);
                        }

                        UserEvent::CloseTab(i) => {
                            if tabs.len() == 1 {
                                // clear last tab
                                tabs[0] = Tab { url: "about:blank".into(), title: String::new(), favicon: None };
                                let _ = page_entries[0].1.load_url("about:blank");
                                active = 0;
                            } else {
                                // destroy gtk widget
                                let (b, _wv) = page_entries.remove(i);
                                pages_gtk.remove(&b);
                                tabs.remove(i);

                                if active >= tabs.len() { active = tabs.len() - 1; }

                                // show active
                                for (j, (b2, _)) in page_entries.iter().enumerate() {
                                    if j == active { b2.show_all(); } else { b2.hide(); }
                                }
                                let url = tabs[active].url.clone();
                                let js = format!("setUrl('{}');", url.replace('\'', "\\'"));
                                let _ = toolbar_wv.evaluate_script(&js);
                            }
                            sync_sidebar(&sidebar_wv, &tabs, active);
                        }

                        UserEvent::SwitchTab(i) => {
                            if i < tabs.len() {
                                // hide old, show new
                                page_entries[active].0.hide();
                                active = i;
                                page_entries[active].0.show_all();

                                let url = tabs[active].url.clone();
                                let js = format!("setUrl('{}');", url.replace('\'', "\\'"));
                                let _ = toolbar_wv.evaluate_script(&js);
                                sync_sidebar(&sidebar_wv, &tabs, active);
                            }
                        }

                        // ── Page events ──
                        UserEvent::PageUrlChanged(idx, url) => {
                            if idx < tabs.len() {
                                tabs[idx].url = url.clone();
                            }
                            if idx == active {
                                let js = format!("setUrl('{}');", url.replace('\'', "\\'"));
                                let _ = toolbar_wv.evaluate_script(&js);
                            }
                            sync_sidebar(&sidebar_wv, &tabs, active);
                        }

                        UserEvent::PageFaviconChanged(idx, fav) => {
                            if idx < tabs.len() {
                                tabs[idx].favicon = Some(fav);
                            }
                            sync_sidebar(&sidebar_wv, &tabs, active);
                        }

                        UserEvent::PageTitleChanged(idx, title) => {
                            if idx < tabs.len() {
                                tabs[idx].title = title;
                            }
                            sync_sidebar(&sidebar_wv, &tabs, active);
                        }
                    }
                }
                _ => {}
            }
        });
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = window;
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;
            if let Event::WindowEvent { event: WindowEvent::CloseRequested, .. } = event {
                *control_flow = ControlFlow::Exit;
            }
        });
    }
}