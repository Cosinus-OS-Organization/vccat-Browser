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
}

fn main() -> wry::Result<()> {
    let event_loop: EventLoop<UserEvent> = EventLoopBuilder::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title("vccat browser")
        .with_inner_size(tao::dpi::LogicalSize::new(1280, 800))
        .with_decorations(true)
        .build(&event_loop)
        .unwrap();

    let ui_html = r#"
<style>
  * { margin: 0; padding: 0; box-sizing: border-box; }

  body {
    font-family: 'JetBrains Mono', 'Fira Code', monospace;
    background: #0e0e0e;
    color: #c9c9c9;
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }

  #toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: #141414;
    border-bottom: 1px solid #222;
    min-height: 48px;
    flex-shrink: 0;
  }

  .nav-btn {
    background: none;
    border: 1px solid #2a2a2a;
    color: #888;
    width: 32px;
    height: 32px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 14px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.15s;
    flex-shrink: 0;
  }

  .nav-btn:hover {
    background: #1e1e1e;
    color: #ddd;
    border-color: #3a3a3a;
  }

  .nav-btn:active {
    background: #252525;
    transform: scale(0.95);
  }

  #url-bar {
    flex: 1;
    background: #1a1a1a;
    border: 1px solid #2a2a2a;
    color: #ddd;
    padding: 7px 14px;
    border-radius: 8px;
    font-size: 13px;
    font-family: inherit;
    outline: none;
    transition: border-color 0.15s, background 0.15s;
    letter-spacing: 0.02em;
  }

  #url-bar:focus {
    border-color: #444;
    background: #1e1e1e;
    color: #f0f0f0;
  }

  #url-bar::placeholder { color: #444; }

  #webview {
    flex: 1;
    width: 100%;
    border: none;
  }

  #status-bar {
    height: 22px;
    background: #0e0e0e;
    border-top: 1px solid #1a1a1a;
    display: flex;
    align-items: center;
    padding: 0 12px;
    font-size: 11px;
    color: #444;
    flex-shrink: 0;
    letter-spacing: 0.03em;
  }

  #status-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>

<div id="toolbar">
  <button class="nav-btn" title="Wstecz" onclick="goBack()">&#8592;</button>
  <button class="nav-btn" title="Dalej" onclick="goForward()">&#8594;</button>
  <button class="nav-btn" title="Odśwież" onclick="reload()">&#8635;</button>
  <input id="url-bar" type="text" placeholder="Wpisz adres lub wyszukaj..." spellcheck="false"
    onkeydown="if(event.key==='Enter') navigate(this.value)" />
</div>
<iframe id="webview" src="about:blank"
  sandbox="allow-same-origin allow-scripts allow-forms allow-popups allow-top-navigation">
</iframe>
<div id="status-bar"><span id="status-text">Gotowy</span></div>

<script>
  const frame = document.getElementById('webview');
  const urlBar = document.getElementById('url-bar');
  const status = document.getElementById('status-text');

  function normalizeUrl(raw) {
    raw = raw.trim();
    if (!raw) return 'about:blank';
    if (raw.startsWith('about:') || raw.startsWith('data:')) return raw;
    if (!/^https?:\/\//i.test(raw)) {
      if (/^[a-z0-9-]+\.[a-z]{2,}/i.test(raw)) return 'https://' + raw;
      return 'https://search.brave.com/search?q=' + encodeURIComponent(raw);
    }
    return raw;
  }

  function navigate(url) {
    const normalized = normalizeUrl(url);
    urlBar.value = normalized;
    frame.src = normalized;
    status.textContent = 'Ładowanie: ' + normalized;
  }

  function goBack()    { try { frame.contentWindow.history.back();    } catch(e) {} }
  function goForward() { try { frame.contentWindow.history.forward(); } catch(e) {} }
  function reload()    {
    try { frame.contentWindow.location.reload(); } catch(e) { frame.src = frame.src; }
  }

  frame.addEventListener('load', () => {
    try {
      const loc = frame.contentWindow.location.href;
      if (loc && loc !== 'about:blank') urlBar.value = loc;
      status.textContent = frame.contentDocument.title || loc || 'Załadowano';
    } catch(e) { status.textContent = 'Załadowano'; }
  });

  frame.addEventListener('mouseover', (e) => {
    try { const a = e.target.closest('a'); if (a?.href) status.textContent = a.href; } catch(e) {}
  });
  frame.addEventListener('mouseout', () => { status.textContent = ''; });

  urlBar.select();
  urlBar.focus();
</script>
"#;

    let proxy_nav    = proxy.clone();
    let proxy_back   = proxy.clone();
    let proxy_fwd    = proxy.clone();
    let proxy_reload = proxy.clone();

    let html = format!(
        r#"<!DOCTYPE html><html><head><meta charset="UTF-8">
        <meta name="color-scheme" content="dark"></head>
        <body>{}</body></html>"#,
        ui_html
    );

    let ipc = move |msg: wry::http::Request<String>| {
        let body = msg.body().to_string();
        if let Some(url) = body.strip_prefix("navigate:") {
            let _ = proxy_nav.send_event(UserEvent::Navigate(url.to_string()));
        } else if body == "back"    { let _ = proxy_back.send_event(UserEvent::GoBack);    }
        else if body == "forward" { let _ = proxy_fwd.send_event(UserEvent::GoForward);  }
        else if body == "reload"  { let _ = proxy_reload.send_event(UserEvent::Reload);  }
    };

    #[cfg(target_os = "linux")]
    let webview = {
        let vbox = window.default_vbox().unwrap();
        WebViewBuilder::new_gtk(vbox)
            .with_html(html)
            .with_ipc_handler(ipc)
            .build()?
    };

    #[cfg(not(target_os = "linux"))]
    let webview = WebViewBuilder::new(&window)
        .with_html(html)
        .with_ipc_handler(ipc)
        .build()?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested, ..
            } => *control_flow = ControlFlow::Exit,

            Event::UserEvent(e) => {
                let js = match e {
                    UserEvent::Navigate(url) =>
                        format!("navigate('{}');", url.replace('\'', "\\'")),
                    UserEvent::GoBack    => "goBack();".into(),
                    UserEvent::GoForward => "goForward();".into(),
                    UserEvent::Reload    => "reload();".into(),
                };
                let _ = webview.evaluate_script(&js);
            }
            _ => {}
        }
    });
}