mod storage;
mod updater;
mod adblock;

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

// ── Tab state ─────────────────────────────────────────────────────────────────

struct Tab {
    url:       String,
    title:     String,
    favicon:   Option<String>,
    suspended: bool,
}

impl Tab {
    fn new(url: &str) -> Self {
        Tab { url: url.into(), title: String::new(), favicon: None, suspended: false }
    }
}

// ── Events ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum UserEvent {
    Navigate(String),
    GoBack, GoForward, Reload,
    NewTab,
    CloseTab(usize),
    SwitchTab(usize),
    ShowHistory,
    PageUrlChanged(usize, String),
    PageFaviconChanged(usize, String),
    PageTitleChanged(usize, String),
    UpdateAvailable(String, String),
}

// ── URL normalizer ────────────────────────────────────────────────────────────

fn normalize_url(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() { return "vccat:home".into(); }
    if raw.starts_with("about:") || raw.starts_with("vccat:") || raw.starts_with("data:") {
        return raw.into();
    }
    if !raw.starts_with("http://") && !raw.starts_with("https://") {
        if raw.contains('.') && !raw.contains(' ') {
            return format!("https://{}", raw);
        }
        return format!("https://search.brave.com/search?q={}", urlencoding::encode(raw));
    }
    raw.into()
}

// ── Page init JS ──────────────────────────────────────────────────────────────

fn page_init_js(tab_idx: usize) -> String {
    let adblock = adblock::youtube_dom_cleaner_js();
    format!(r#"(function() {{
    function ipc(m) {{ window.ipc.postMessage(m); }}
    ipc('url:' + location.href);
    function sendTitle() {{ if (document.title) ipc('title:' + document.title); }}
    sendTitle();
    const titleEl = document.querySelector('title');
    if (titleEl) new MutationObserver(sendTitle).observe(titleEl, {{childList:true}});
    function sendFavicon() {{
        let fav = '';
        for (const l of document.querySelectorAll('link[rel~="icon"],link[rel~="shortcut"]')) {{
            if (l.href) {{ fav = l.href; break; }}
        }}
        if (!fav && location.origin !== 'null') fav = location.origin + '/favicon.ico';
        if (fav) ipc('favicon:' + fav);
    }}
    if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', sendFavicon);
    else sendFavicon();
    {adblock}
}})();"#, adblock = adblock)
}

// ── Home page ─────────────────────────────────────────────────────────────────

fn home_page_html(history: &[storage::HistoryEntry]) -> String {
    let recent: String = history.iter().rev().take(8).map(|h| {
        let title = if h.title.is_empty() { &h.url } else { &h.title };
        let t = if title.len() > 45 { &title[..45] } else { title };
        let u = if h.url.len() > 55 { &h.url[..55] } else { &h.url };
        format!(r#"<a href="{url}" class="hi"><span class="ht">{t}</span><span class="hu">{u}</span></a>"#,
                url = h.url, t = t, u = u)
    }).collect();

    format!(r#"<!DOCTYPE html><html><head><meta charset="UTF-8"><title>vccat</title>
<style>
*{{margin:0;padding:0;box-sizing:border-box;}}
html,body{{height:100%;background:#08080f;color:#555;
  font-family:'JetBrains Mono','Fira Code',monospace;
  display:flex;flex-direction:column;align-items:center;justify-content:center;gap:28px;}}
.logo{{font-size:32px;font-weight:700;letter-spacing:0.2em;color:#1e1630;
  text-shadow:0 0 60px #2a1a4e;}}
.logo span{{color:#5a3a8a;}}
.wrap{{display:flex;flex-direction:column;gap:5px;width:460px;max-width:90vw;}}
.lbl{{font-size:10px;letter-spacing:0.2em;color:#1a1a28;text-transform:uppercase;margin-bottom:4px;}}
.hi{{display:flex;flex-direction:column;gap:2px;padding:8px 12px;border-radius:8px;
  border:1px solid #100f18;background:#0c0b14;text-decoration:none;transition:all 0.1s;}}
.hi:hover{{background:#0f0e1c;border-color:#1e1630;}}
.ht{{font-size:12px;color:#5a3a7a;}}
.hu{{font-size:10px;color:#1c1c28;}}
.ver{{font-size:10px;color:#141420;position:fixed;bottom:12px;right:16px;}}
</style></head><body>
<div class="logo">vc<span>cat</span></div>
<div class="wrap"><div class="lbl">ostatnio odwiedzone</div>{}</div>
<div class="ver">v{}</div>
</body></html>"#, recent, env!("CARGO_PKG_VERSION"))
}

// ── History page ──────────────────────────────────────────────────────────────

fn history_page_html(history: &[storage::HistoryEntry]) -> String {
    let rows: String = history.iter().rev().take(300).map(|h| {
        let title = if h.title.is_empty() { h.url.clone() } else { h.title.clone() };
        format!(r#"<tr><td><a href="{}">{}</a></td><td class="u">{}</td></tr>"#,
                h.url, title, h.url)
    }).collect();
    format!(r#"<!DOCTYPE html><html><head><meta charset="UTF-8"><title>Historia</title>
<style>
*{{margin:0;padding:0;box-sizing:border-box;}}
body{{background:#08080f;color:#555;font-family:'JetBrains Mono','Fira Code',monospace;padding:32px;}}
h1{{font-size:16px;color:#3a2a5e;margin-bottom:20px;letter-spacing:0.15em;}}
table{{width:100%;border-collapse:collapse;}}
tr{{border-bottom:1px solid #0f0e18;}}
tr:hover{{background:#0c0b14;}}
td{{padding:7px 10px;font-size:12px;}}
a{{color:#6a4a9a;text-decoration:none;}}
a:hover{{color:#8a6abb;}}
.u{{color:#1e1e2e;font-size:10px;}}
</style></head><body>
<h1>// historia</h1><table>{}</table>
</body></html>"#, rows)
}

// ── Sidebar HTML ──────────────────────────────────────────────────────────────

fn sidebar_html() -> &'static str {
    r#"<!DOCTYPE html><html><head><meta charset="UTF-8">
<style>
*{margin:0;padding:0;box-sizing:border-box;}
html,body{width:52px;height:100%;background:#080810;overflow:hidden;
  display:flex;flex-direction:column;align-items:center;
  border-right:1px solid #111118;}
#tabs{flex:1;width:100%;overflow-y:auto;overflow-x:hidden;
  display:flex;flex-direction:column;align-items:center;padding:8px 0;gap:4px;}
#tabs::-webkit-scrollbar{width:2px;}
#tabs::-webkit-scrollbar-thumb{background:#1e1e2e;border-radius:2px;}
.tab{position:relative;width:36px;height:36px;border-radius:8px;cursor:pointer;
  display:flex;align-items:center;justify-content:center;flex-shrink:0;
  border:1px solid transparent;transition:all 0.1s;}
.tab:hover{background:#111120;border-color:#1e1e2e;}
.tab.active{background:#14102a;border-color:#2a1a4e;box-shadow:inset 0 0 0 1px #2a1a4e;}
.tab.suspended{opacity:0.35;}
.tab img{width:18px;height:18px;border-radius:3px;object-fit:contain;}
.fb{width:20px;height:20px;border-radius:5px;background:#111120;border:1px solid #1e1e2e;
  display:flex;align-items:center;justify-content:center;font-size:10px;font-weight:600;
  color:#3a2a5e;font-family:monospace;text-transform:uppercase;}
.x{display:none;position:absolute;top:-5px;right:-5px;width:14px;height:14px;
  background:#0c0c18;border:1px solid #1e1e2e;border-radius:50%;color:#555;font-size:9px;
  align-items:center;justify-content:center;cursor:pointer;z-index:10;}
.tab:hover .x{display:flex;}
.x:hover{background:#141428;color:#ccc;}
#bottom{width:100%;display:flex;flex-direction:column;align-items:center;gap:4px;padding:6px 0;}
.ib{width:36px;height:36px;border-radius:8px;border:1px solid #111120;background:none;
  color:#252535;font-size:14px;cursor:pointer;display:flex;align-items:center;
  justify-content:center;transition:all 0.1s;}
.ib:hover{border-color:#2a1a4e;color:#7a5aaa;background:#0f0f1e;}
</style></head><body>
<div id="tabs"></div>
<div id="bottom">
  <button class="ib" title="Historia" onclick="send('history')">&#9776;</button>
  <button class="ib" title="Nowa karta" onclick="send('new')">+</button>
</div>
<script>
let state={tabs:[],active:0};
function send(m){window.ipc.postMessage(m);}
function render(){
  const c=document.getElementById('tabs');c.innerHTML='';
  state.tabs.forEach((t,i)=>{
    const el=document.createElement('div');
    el.className='tab'+(i===state.active?' active':'')+(t.suspended?' suspended':'');
    el.title=t.title||t.url||'Nowa karta';
    if(t.favicon){const img=document.createElement('img');img.src=t.favicon;
      img.onerror=()=>img.replaceWith(makeFb(t));el.appendChild(img);}
    else el.appendChild(makeFb(t));
    const x=document.createElement('div');x.className='x';x.textContent='×';
    x.onclick=e=>{e.stopPropagation();send('close:'+i);};
    el.appendChild(x);
    el.onclick=()=>send('switch:'+i);
    c.appendChild(el);
  });
}
function makeFb(t){
  const d=document.createElement('div');d.className='fb';
  try{const h=new URL(t.url||'about:blank').hostname;d.textContent=h?h[0]:'?';}
  catch{d.textContent='?';}
  return d;
}
function update(s){state=s;render();}
</script></body></html>"#
}

// ── Toolbar HTML ──────────────────────────────────────────────────────────────

fn toolbar_html() -> &'static str {
    r#"<!DOCTYPE html><html><head><meta charset="UTF-8">
<style>
*{margin:0;padding:0;box-sizing:border-box;}
html,body{height:100%;background:#0a0a12;overflow:hidden;display:flex;align-items:center;
  gap:7px;padding:0 10px;border-bottom:1px solid #111120;
  font-family:'JetBrains Mono','Fira Code',monospace;}
button{background:none;border:1px solid #161625;color:#444;width:28px;height:28px;
  border-radius:6px;cursor:pointer;font-size:13px;display:flex;align-items:center;
  justify-content:center;transition:all 0.1s;flex-shrink:0;}
button:hover{background:#0f0f1e;color:#bbb;border-color:#2a1a4e;}
button:active{transform:scale(0.92);}
#url{flex:1;background:#0d0d18;border:1px solid #161625;color:#aaa;padding:5px 12px;
  border-radius:7px;font-size:12px;font-family:inherit;outline:none;letter-spacing:0.02em;
  transition:border-color 0.15s;}
#url:focus{border-color:#3a2a5e;color:#eee;}
#url::placeholder{color:#1e1e2e;}
#upd{display:none;padding:3px 10px;background:#100e1e;border:1px solid #2a1a4e;
  border-radius:6px;font-size:10px;color:#7a5aaa;cursor:pointer;white-space:nowrap;}
#upd:hover{background:#14102a;}
</style></head><body>
<button title="Wstecz"  onclick="s('back')">&#8592;</button>
<button title="Dalej"   onclick="s('fwd')">&#8594;</button>
<button title="Odśwież" onclick="s('reload')">&#8635;</button>
<input id="url" type="text" placeholder="Adres lub wyszukaj..."
  spellcheck="false"
  onkeydown="if(event.key==='Enter'){s('nav:'+this.value);this.blur();}"
  onfocus="this.select()"/>
<div id="upd" onclick="s('apply-update')"></div>
<script>
function s(m){window.ipc.postMessage(m);}
function setUrl(u){const el=document.getElementById('url');if(document.activeElement!==el)el.value=u;}
function showUpdate(v){const b=document.getElementById('upd');b.textContent='↑ '+v;b.style.display='block';}
let _upd=null;
function setPendingUpdate(v,u){_upd={v,u};showUpdate(v);}
</script></body></html>"#
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn sync_sidebar(sidebar_wv: &wry::WebView, tabs: &[Tab], active: usize) {
    let items: String = tabs.iter().map(|t| {
        format!(r#"{{"url":"{}","favicon":"{}","title":"{}","suspended":{}}}"#,
                t.url.replace('"',"\\\""),
                t.favicon.as_deref().unwrap_or("").replace('"',"\\\""),
                t.title.replace('"',"\\\""),
                t.suspended)
    }).collect::<Vec<_>>().join(",");
    let _ = sidebar_wv.evaluate_script(
        &format!("update({{tabs:[{}],active:{}}});", items, active)
    );
}

fn toolbar_set_url(toolbar_wv: &wry::WebView, url: &str) {
    let safe = url.replace('\'', "\\'").replace('\n', "");
    let _ = toolbar_wv.evaluate_script(&format!("setUrl('{}');", safe));
}

fn save_session(tabs: &[Tab], active: usize) {
    storage::save_session(&storage::Session {
        tabs: tabs.iter().map(|t| t.url.clone()).collect(),
        active,
    });
}


fn load_html_into(wv: &wry::WebView, html: &str) {
    let escaped = html.replace('\\', "\\\\").replace('`', "\\`").replace("${", "\\${");
    let js = format!("document.open();document.write(\\`{}\\`);document.close();", escaped);
    let _ = wv.evaluate_script(&js);
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> wry::Result<()> {
    let session = storage::load_session();
    let mut history = storage::load_history();

    let event_loop: EventLoop<UserEvent> = EventLoopBuilder::with_user_event().build();
    let proxy = event_loop.create_proxy();

    // background update check
    let proxy_upd = proxy.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(4));
        if let Some(info) = updater::check_update() {
            let _ = proxy_upd.send_event(
                UserEvent::UpdateAvailable(info.version, info.download_url)
            );
        }
    });

    let window = WindowBuilder::new()
        .with_title("vccat browser")
        .with_inner_size(tao::dpi::LogicalSize::new(1360, 860))
        .with_decorations(true)
        .build(&event_loop)
        .unwrap();

    const SUSPEND_THRESHOLD: usize = 4;

    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::*;

        let root = window.default_vbox().unwrap();
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        root.pack_start(&hbox, true, true, 0);

        let sidebar_gtk = gtk::Box::new(gtk::Orientation::Vertical, 0);
        sidebar_gtk.set_size_request(52, -1);

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

        // ── Load adblock content rules ──
        let _filter_path = adblock::ensure_filters_downloaded();

        // ── Sidebar ──
        let ps = proxy.clone();
        let sidebar_wv = WebViewBuilder::new_gtk(&sidebar_gtk)
            .with_html(sidebar_html())
            .with_ipc_handler(move |msg: wry::http::Request<String>| {
                let b = msg.body().as_str();
                if b == "new" { let _ = ps.send_event(UserEvent::NewTab); }
                else if b == "history" { let _ = ps.send_event(UserEvent::ShowHistory); }
                else if let Some(i) = b.strip_prefix("close:")
                    .and_then(|s| s.parse::<usize>().ok()) {
                    let _ = ps.send_event(UserEvent::CloseTab(i));
                } else if let Some(i) = b.strip_prefix("switch:")
                    .and_then(|s| s.parse::<usize>().ok()) {
                    let _ = ps.send_event(UserEvent::SwitchTab(i));
                }
            })
            .with_background_color((8, 8, 16, 255))
            .build()?;

        // ── Toolbar ──
        let pt = proxy.clone();
        let mut pending_update: Option<updater::UpdateInfo> = None;
        let toolbar_wv = WebViewBuilder::new_gtk(&toolbar_gtk)
            .with_html(toolbar_html())
            .with_ipc_handler(move |msg: wry::http::Request<String>| {
                let b = msg.body().as_str();
                if let Some(url) = b.strip_prefix("nav:") {
                    let _ = pt.send_event(UserEvent::Navigate(normalize_url(url)));
                } else if b == "back"    { let _ = pt.send_event(UserEvent::GoBack);    }
                else if b == "fwd"     { let _ = pt.send_event(UserEvent::GoForward);  }
                else if b == "reload"  { let _ = pt.send_event(UserEvent::Reload);     }
            })
            .with_background_color((10, 10, 18, 255))
            .build()?;

        // ── helper: build a page WebView ──
        macro_rules! make_page_wv {
            ($idx:expr, $url:expr, $box:expr) => {{
                let idx = $idx;
                let pu_nav = proxy.clone();
                let pu_ipc = proxy.clone();
                let pf     = proxy.clone();
                let pt2    = proxy.clone();
                let init_js = page_init_js(idx);
                WebViewBuilder::new_gtk($box)
                    .with_url($url)
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
                    .build()?
            }};
        }

        // ── init tabs from session ──
        let mut tabs: Vec<Tab> = session.tabs.iter().map(|u| Tab::new(u)).collect();
        if tabs.is_empty() { tabs.push(Tab::new("vccat:home")); }
        let mut active = session.active.min(tabs.len() - 1);

        // page_entries: Option<(gtk::Box, WebView)> — None = suspended
        let mut page_entries: Vec<Option<(gtk::Box, wry::WebView)>> = Vec::new();

        for (i, tab) in tabs.iter_mut().enumerate() {
            let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
            b.set_vexpand(true);
            pages_gtk.pack_start(&b, true, true, 0);

            if i == active {
                b.show_all();
                let wv = if tab.url.starts_with("vccat:") {
                    let html = home_page_html(&history);
                    let idx = i;
                    let pu_nav = proxy.clone();
                    let pu_ipc = proxy.clone();
                    let pf     = proxy.clone();
                    let pt2    = proxy.clone();
                    let init_js = page_init_js(idx);
                    WebViewBuilder::new_gtk(&b)
                        .with_html(&html)
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
                        .build()?
                } else {
                    make_page_wv!(i, &tab.url.clone(), &b)
                };
                page_entries.push(Some((b, wv)));
            } else if i < SUSPEND_THRESHOLD {
                b.hide();
                let wv = make_page_wv!(i, &tab.url, &b);
                page_entries.push(Some((b, wv)));
            } else {
                b.hide();
                page_entries.push(None);
                tab.suspended = true;
            }
        }

        toolbar_set_url(&toolbar_wv, &tabs[active].url);
        sync_sidebar(&sidebar_wv, &tabs, active);

        // ── Event loop ────────────────────────────────────────────────────────
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;
            match event {
                Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                    save_session(&tabs, active);
                    *control_flow = ControlFlow::Exit;
                }

                Event::UserEvent(e) => {
                    use gtk::prelude::*;

                    // helper: wake a suspended tab
                    macro_rules! wake_tab {
                        ($i:expr) => {{
                            let i = $i;
                            if tabs[i].suspended {
                                // find the existing (hidden empty) box
                                if let Some(slot) = page_entries.get_mut(i) {
                                    if slot.is_none() {
                                        let nb = gtk::Box::new(gtk::Orientation::Vertical, 0);
                                        nb.set_vexpand(true);
                                        pages_gtk.pack_start(&nb, true, true, 0);
                                        let url = tabs[i].url.clone();
                                        let wv = {
                                            let idx = i;
                                            let pu_nav = proxy.clone();
                                            let pu_ipc = proxy.clone();
                                            let pf     = proxy.clone();
                                            let pt2    = proxy.clone();
                                            let init_js = page_init_js(idx);
                                            WebViewBuilder::new_gtk(&nb)
                                                .with_url(&url)
                                                .with_initialization_script(&init_js)
                                                .with_navigation_handler(move |u| {
                                                    let _ = pu_nav.send_event(UserEvent::PageUrlChanged(idx, u));
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
                                                .build().unwrap()
                                        };
                                        *slot = Some((nb, wv));
                                        tabs[i].suspended = false;
                                    }
                                }
                            }
                        }};
                    }

                    match e {
                        UserEvent::Navigate(url) => {
                            tabs[active].url = url.clone();
                            tabs[active].favicon = None;
                            if let Some(Some((_, ref wv))) = page_entries.get(active) {
                                if url.starts_with("vccat:") {
                                    let html = if url == "vccat:home" {
                                        home_page_html(&history)
                                    } else {
                                        history_page_html(&history)
                                    };
                                    load_html_into(wv, &html);
                                } else {
                                    let _ = wv.load_url(&url);
                                }
                            }
                            toolbar_set_url(&toolbar_wv, &url);
                            save_session(&tabs, active);
                            sync_sidebar(&sidebar_wv, &tabs, active);
                        }

                        UserEvent::GoBack => {
                            if let Some(Some((_, ref wv))) = page_entries.get(active) {
                                let _ = wv.evaluate_script("history.back()");
                            }
                        }
                        UserEvent::GoForward => {
                            if let Some(Some((_, ref wv))) = page_entries.get(active) {
                                let _ = wv.evaluate_script("history.forward()");
                            }
                        }
                        UserEvent::Reload => {
                            if let Some(Some((_, ref wv))) = page_entries.get(active) {
                                let _ = wv.evaluate_script("location.reload()");
                            }
                        }

                        UserEvent::NewTab => {
                            let idx = tabs.len();
                            tabs.push(Tab::new("vccat:home"));

                            // suspend oldest background tabs if over threshold
                            if idx >= SUSPEND_THRESHOLD {
                                for i in 0..idx {
                                    if i != active && !tabs[i].suspended {
                                        if let Some(ref mut slot) = page_entries.get_mut(i) {
                                            if let Some((ref b, _)) = slot {
                                                b.hide();
                                            }
                                            **slot = None;
                                            tabs[i].suspended = true;
                                        }
                                    }
                                }
                            }

                            // hide current active
                            if let Some(Some((ref b, _))) = page_entries.get(active) {
                                b.hide();
                            }

                            let nb = gtk::Box::new(gtk::Orientation::Vertical, 0);
                            nb.set_vexpand(true);
                            pages_gtk.pack_start(&nb, true, true, 0);
                            nb.show_all();

                            let home_html = home_page_html(&history);
                            let wv = {
                                let pu_nav = proxy.clone();
                                let pu_ipc = proxy.clone();
                                let pf     = proxy.clone();
                                let pt2    = proxy.clone();
                                let init_js = page_init_js(idx);
                                WebViewBuilder::new_gtk(&nb)
                                    .with_html(&home_html)
                                    .with_initialization_script(&init_js)
                                    .with_navigation_handler(move |u| {
                                        let _ = pu_nav.send_event(UserEvent::PageUrlChanged(idx, u));
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
                                    .build().unwrap()
                            };
                            page_entries.push(Some((nb, wv)));
                            active = idx;
                            toolbar_set_url(&toolbar_wv, "vccat:home");
                            save_session(&tabs, active);
                            sync_sidebar(&sidebar_wv, &tabs, active);
                        }

                        UserEvent::CloseTab(i) => {
                            if tabs.len() == 1 {
                                tabs[0] = Tab::new("vccat:home");
                                if let Some(Some((_, ref wv))) = page_entries.get(0) {
                                    load_html_into(wv, &home_page_html(&history));
                                }
                                active = 0;
                            } else {
                                if let Some(Some((ref b, _))) = page_entries.get(i) {
                                    pages_gtk.remove(b);
                                }
                                page_entries.remove(i);
                                tabs.remove(i);
                                if active >= tabs.len() { active = tabs.len() - 1; }
                                else if active > i { active -= 1; }

                                // show new active, hide others
                                for (j, entry) in page_entries.iter().enumerate() {
                                    if let Some((ref b, _)) = entry {
                                        if j == active { b.show_all(); } else { b.hide(); }
                                    }
                                }
                                // wake if suspended
                                wake_tab!(active);
                            }
                            toolbar_set_url(&toolbar_wv, &tabs[active].url);
                            save_session(&tabs, active);
                            sync_sidebar(&sidebar_wv, &tabs, active);
                        }

                        UserEvent::SwitchTab(i) => {
                            if i >= tabs.len() { return; }
                            if let Some(Some((ref b, _))) = page_entries.get(active) {
                                b.hide();
                            }
                            active = i;
                            wake_tab!(active);
                            if let Some(Some((ref b, _))) = page_entries.get(active) {
                                b.show_all();
                            }
                            toolbar_set_url(&toolbar_wv, &tabs[active].url);
                            save_session(&tabs, active);
                            sync_sidebar(&sidebar_wv, &tabs, active);
                        }

                        UserEvent::ShowHistory => {
                            let idx = tabs.len();
                            tabs.push(Tab { url: "vccat:history".into(), title: "Historia".into(),
                                favicon: None, suspended: false });
                            if let Some(Some((ref b, _))) = page_entries.get(active) { b.hide(); }
                            let nb = gtk::Box::new(gtk::Orientation::Vertical, 0);
                            nb.set_vexpand(true);
                            pages_gtk.pack_start(&nb, true, true, 0);
                            nb.show_all();
                            let hist_html = history_page_html(&history);
                            let ph = proxy.clone();
                            let wv = WebViewBuilder::new_gtk(&nb)
                                .with_html(&hist_html)
                                .with_navigation_handler(move |url| {
                                    if url.starts_with("http") {
                                        let _ = ph.send_event(UserEvent::Navigate(url));
                                        return false;
                                    }
                                    true
                                })
                                .build().unwrap();
                            page_entries.push(Some((nb, wv)));
                            active = idx;
                            toolbar_set_url(&toolbar_wv, "vccat:history");
                            sync_sidebar(&sidebar_wv, &tabs, active);
                        }

                        UserEvent::PageUrlChanged(idx, url) => {
                            // filter out data: URLs (home page internal)
                            if url.starts_with("data:") { return; }
                            if idx < tabs.len() {
                                tabs[idx].url = url.clone();
                                storage::append_history(&mut history, &url, &tabs[idx].title);
                            }
                            if idx == active { toolbar_set_url(&toolbar_wv, &url); }
                            save_session(&tabs, active);
                            sync_sidebar(&sidebar_wv, &tabs, active);
                        }

                        UserEvent::PageFaviconChanged(idx, fav) => {
                            if idx < tabs.len() { tabs[idx].favicon = Some(fav); }
                            sync_sidebar(&sidebar_wv, &tabs, active);
                        }

                        UserEvent::PageTitleChanged(idx, title) => {
                            if idx < tabs.len() {
                                if idx == active {
                                    storage::append_history(&mut history, &tabs[idx].url, &title);
                                }
                                tabs[idx].title = title;
                            }
                            sync_sidebar(&sidebar_wv, &tabs, active);
                        }

                        UserEvent::UpdateAvailable(version, url) => {
                            pending_update = Some(updater::UpdateInfo {
                                version: version.clone(),
                                download_url: url.clone(),
                            });
                            let js = format!("setPendingUpdate('{}','{}');",
                                             version.replace('\'',"\\'"), url.replace('\'',"\\'"));
                            let _ = toolbar_wv.evaluate_script(&js);
                        }
                    }
                }
                _ => {}
            }
        });
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (window, history, session);
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;
            if let Event::WindowEvent { event: WindowEvent::CloseRequested, .. } = event {
                *control_flow = ControlFlow::Exit;
            }
        });
    }
}