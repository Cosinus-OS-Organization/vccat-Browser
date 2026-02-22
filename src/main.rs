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

#[derive(Debug)]
enum UserEvent {
    Navigate(String),
    GoBack,
    GoForward,
    Reload,
    UpdateUrl(String),
    UpdateTitle(String),
}

fn normalize_url(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() { return "about:blank".into(); }
    if raw.starts_with("about:") || raw.starts_with("data:") { return raw.into(); }
    if !raw.starts_with("http://") && !raw.starts_with("https://") {
        // looks like a domain
        if raw.contains('.') && !raw.contains(' ') {
            return format!("https://{}", raw);
        }
        // search
        return format!(
            "https://search.brave.com/search?q={}",
            urlencoding::encode(raw)
        );
    }
    raw.into()
}

fn main() -> wry::Result<()> {
    let event_loop: EventLoop<UserEvent> = EventLoopBuilder::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title("vccat browser")
        .with_inner_size(tao::dpi::LogicalSize::new(1280, 820))
        .with_decorations(true)
        .build(&event_loop)
        .unwrap();

    // ── Toolbar HTML (tiny overlay, NOT the page itself) ──────────────────────
    let toolbar_html = r#"<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<style>
  * { margin:0; padding:0; box-sizing:border-box; }
  html, body { height:100%; overflow:hidden; background:#141414; }

  body {
    display:flex;
    align-items:center;
    gap:8px;
    padding:0 12px;
    height:48px;
    border-bottom:1px solid #222;
    font-family:'JetBrains Mono','Fira Code',monospace;
  }

  button {
    background:none;
    border:1px solid #2a2a2a;
    color:#888;
    width:30px; height:30px;
    border-radius:6px;
    cursor:pointer;
    font-size:15px;
    display:flex; align-items:center; justify-content:center;
    transition:all 0.12s;
    flex-shrink:0;
  }
  button:hover  { background:#1e1e1e; color:#ddd; border-color:#3a3a3a; }
  button:active { transform:scale(0.93); }

  #url {
    flex:1;
    background:#1a1a1a;
    border:1px solid #2a2a2a;
    color:#ddd;
    padding:6px 14px;
    border-radius:8px;
    font-size:13px;
    font-family:inherit;
    outline:none;
    letter-spacing:0.02em;
    transition:border-color 0.15s, background 0.15s;
  }
  #url:focus { border-color:#444; background:#1e1e1e; color:#f0f0f0; }
  #url::placeholder { color:#3a3a3a; }
</style>
</head>
<body>
  <button title="Wstecz"   onclick="send('back')">&#8592;</button>
  <button title="Dalej"    onclick="send('fwd')">&#8594;</button>
  <button title="Odśwież"  onclick="send('reload')">&#8635;</button>
  <input id="url" type="text" placeholder="Wpisz adres lub wyszukaj..."
    spellcheck="false"
    onkeydown="if(event.key==='Enter'){ send('nav:'+this.value); this.blur(); }"
    onfocus="this.select()" />

  <script>
    function send(msg) {
      window.ipc.postMessage(msg);
    }

    // called from Rust to update URL bar
    function setUrl(url) {
      const el = document.getElementById('url');
      if (document.activeElement !== el) el.value = url;
    }
  </script>
</body>
</html>"#;

    // ── IPC from toolbar ──────────────────────────────────────────────────────
    let p_nav    = proxy.clone();
    let p_back   = proxy.clone();
    let p_fwd    = proxy.clone();
    let p_reload = proxy.clone();

    let toolbar_ipc = move |msg: wry::http::Request<String>| {
        let body = msg.body().as_str();
        if let Some(url) = body.strip_prefix("nav:") {
            let normalized = normalize_url(url);
            let _ = p_nav.send_event(UserEvent::Navigate(normalized));
        } else if body == "back"   { let _ = p_back.send_event(UserEvent::GoBack);    }
        else if body == "fwd"    { let _ = p_fwd.send_event(UserEvent::GoForward);  }
        else if body == "reload" { let _ = p_reload.send_event(UserEvent::Reload);  }
    };

    // ── Build toolbar WebView (GTK fixed height box) ──────────────────────────
    #[cfg(target_os = "linux")]
    let (toolbar_wv, page_wv) = {
        use gtk::prelude::*;

        let vbox = window.default_vbox().unwrap();

        // toolbar fixed-height
        let toolbar_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        toolbar_box.set_size_request(-1, 48);

        // page fills the rest
        let page_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        page_box.set_vexpand(true);

        // status bar
        let status_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        status_box.set_size_request(-1, 22);

        vbox.pack_start(&toolbar_box, false, false, 0);
        vbox.pack_start(&page_box, true, true, 0);
        vbox.pack_start(&status_box, false, false, 0);
        vbox.show_all();

        let twv = WebViewBuilder::new_gtk(&toolbar_box)
            .with_html(toolbar_html)
            .with_ipc_handler(toolbar_ipc)
            .with_background_color((20, 20, 20, 255))
            .build()?;

        let p_url   = proxy.clone();
        let p_title = proxy.clone();

        let pwv = WebViewBuilder::new_gtk(&page_box)
            .with_url("about:blank")
            .with_navigation_handler(move |url| {
                let _ = p_url.send_event(UserEvent::UpdateUrl(url));
                true
            })
            .with_document_title_changed_handler(move |title| {
                let _ = p_title.send_event(UserEvent::UpdateTitle(title));
            })
            .build()?;

        (twv, pwv)
    };

    #[cfg(not(target_os = "linux"))]
    compile_error!("Only Linux supported in this build. Remove this for macOS/Windows support.");

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested, ..
            } => *control_flow = ControlFlow::Exit,

            Event::UserEvent(e) => match e {
                UserEvent::Navigate(url) => {
                    let _ = page_wv.load_url(&url);
                    let js = format!("setUrl('{}');", url.replace('\'', "\\'"));
                    let _ = toolbar_wv.evaluate_script(&js);
                }
                UserEvent::GoBack    => { let _ = page_wv.evaluate_script("history.back()");    }
                UserEvent::GoForward => { let _ = page_wv.evaluate_script("history.forward()"); }
                UserEvent::Reload    => { let _ = page_wv.evaluate_script("location.reload()"); }
                UserEvent::UpdateUrl(url) => {
                    let js = format!("setUrl('{}');", url.replace('\'', "\\'"));
                    let _ = toolbar_wv.evaluate_script(&js);
                }
                UserEvent::UpdateTitle(title) => {
                    // optionally update window title
                    let _ = title; // suppress unused warning
                }
            },
            _ => {}
        }
    });
}