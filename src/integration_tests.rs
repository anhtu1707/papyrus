//! Native integration test. Uses only disposable profiles under target/ and a
//! loopback HTTP server; never reads or clears the user's normal browser data.
use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tao::{
    event_loop::EventLoop,
    platform::{run_return::EventLoopExtRunReturn, windows::EventLoopBuilderExtWindows},
};

struct Harness {
    event_loop: EventLoop<EventMessage>,
    app: Rc<RefCell<Browser>>,
}
impl Harness {
    fn wait(&mut self, predicate: impl Fn(&Browser) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !predicate(&self.app.borrow()) && Instant::now() < deadline {
            let tick = Instant::now() + Duration::from_millis(20);
            let app = self.app.clone();
            let hwnd = app.borrow().window.hwnd();
            self.event_loop.run_return(|event, _, flow| {
                *flow = ControlFlow::WaitUntil(tick);
                if let Event::UserEvent(event) = event {
                    let app = app.clone();
                    native::dispatch(hwnd, move || {
                        let mut app = app.borrow_mut();
                        app.event(event).unwrap();
                        app.publish().unwrap();
                    })
                    .unwrap();
                }
                if Instant::now() >= tick {
                    *flow = ControlFlow::Exit;
                }
            });
        }
        assert!(
            predicate(&self.app.borrow()),
            "WebView condition timed out: {}",
            self.app.borrow().notice
        );
    }
    fn act(&mut self, operation: impl FnOnce(&mut Browser) + 'static) {
        let done = Rc::new(std::cell::Cell::new(false));
        let completed = done.clone();
        let app = self.app.clone();
        let hwnd = app.borrow().window.hwnd();
        native::dispatch(hwnd, move || {
            operation(&mut app.borrow_mut());
            completed.set(true);
        })
        .unwrap();
        self.wait(|_| done.get());
    }
    fn command(&mut self, command: Command) {
        self.act(move |app| {
            app.command(command).unwrap();
            app.publish().unwrap();
        });
    }
    fn eval(&mut self, header: bool, script: &str) -> serde_json::Value {
        let result = Arc::new(std::sync::Mutex::new(None));
        let output = result.clone();
        let script = script.to_owned();
        self.act(move |app| {
            let view = if header {
                &app.header
            } else {
                app.active_view().unwrap()
            };
            view.evaluate_script_with_callback(&script, move |value| {
                *output.lock().unwrap() = Some(value);
            })
            .unwrap();
        });
        self.wait(|_| result.lock().unwrap().is_some());
        serde_json::from_str(result.lock().unwrap().as_ref().unwrap()).unwrap()
    }
}

struct Server {
    port: u16,
    stop: Arc<AtomicBool>,
    hits: Arc<AtomicUsize>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let signal = stop.clone();
        let hits = Arc::new(AtomicUsize::new(0));
        let count = hits.clone();
        let thread = std::thread::spawn(move || {
            while !signal.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut buffer = [0u8; 8192];
                        let Ok(len) = stream.read(&mut buffer) else {
                            continue;
                        };
                        let request = String::from_utf8_lossy(&buffer[..len]);
                        if request.starts_with("GET /download") {
                            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=report.txt\r\nContent-Length: 12\r\nConnection: close\r\n\r\naudit-file\r\n");
                            continue;
                        }
                        let ad = request.starts_with("GET /ad");
                        if ad {
                            count.fetch_add(1, Ordering::Relaxed);
                        }
                        let body = if ad {
                            "window.adLoaded=true".into()
                        } else {
                            format!(
                                r#"<!doctype html><meta charset='utf-8'><title>Local test</title><input id='draft'><a href='/second'>Second</a><script src='http://localhost:{port}/ad.js'></script><img src='http://localhost:{port}/ad.png'><script>window.ready=true;</script>"#
                            )
                        };
                        let content_type = if ad {
                            "application/javascript"
                        } else {
                            "text/html; charset=utf-8"
                        };
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = stream.write_all(response.as_bytes());
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            port,
            stop,
            hits,
            thread: Some(thread),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}

#[test]
#[ignore = "requires Windows WebView2 and a desktop session; run explicitly"]
fn native_browser_regressions() {
    eprintln!("Native test: creating isolated WebView2 environment");
    let server = Server::start();
    let event_loop = EventLoopBuilder::<EventMessage>::with_user_event()
        .with_any_thread(true)
        .build();
    let window = WindowBuilder::new()
        .with_title("Papyrus isolated integration test")
        .with_visible(false)
        .build(&event_loop)
        .unwrap();
    let profile = std::env::current_dir()
        .unwrap()
        .join("target")
        .join(format!(
            "webview-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    let app = Browser::create_at(window, event_loop.create_proxy(), profile).unwrap();
    let mut h = Harness {
        event_loop,
        app: Rc::new(RefCell::new(app)),
    };
    h.wait(|a| a.header_ready && a.active_tab().is_some_and(|t| !t.info.loading));
    eprintln!("Native test: request blocking and IPC isolation");
    let url = format!("http://127.0.0.1:{}/state", server.port);
    h.act(|a| {
        let mut prefs = a.saved.preferences.clone();
        prefs.blocked_hosts.push("localhost".into());
        a.preferences(prefs).unwrap();
    });
    h.command(Command::Navigate { input: url.clone() });
    h.wait(|a| {
        a.active_tab()
            .is_some_and(|t| !t.info.loading && t.info.url.contains("/state"))
            && a.blocked >= 2
    });
    assert_eq!(
        server.hits.load(Ordering::Relaxed),
        0,
        "native filter must stop requests before server sees them"
    );
    assert_eq!(h.eval(false, "typeof window.ipc"), "undefined");
    h.eval(false,"document.getElementById('draft').value='retained'; document.cookie='audit=1'; localStorage.setItem('audit','1'); true");
    let original = h.app.borrow().active;
    h.command(Command::NewTab);
    h.wait(|a| a.active_tab().is_some_and(|t| !t.info.loading));
    h.command(Command::SwitchTab { id: original });
    assert_eq!(
        h.eval(false, "document.getElementById('draft').value"),
        "retained"
    );
    h.eval(false, "history.pushState({}, '', '/settings.html'); true");
    h.wait(|a| a.active_tab().unwrap().info.url.ends_with("/settings.html"));
    assert!(
        h.app
            .borrow()
            .active_tab()
            .unwrap()
            .info
            .url
            .starts_with("http://127.0.0.1:")
    );
    h.command(Command::Back);
    h.wait(|a| a.active_tab().unwrap().info.url.ends_with("/state"));
    h.eval(
        false,
        r#"document.title='<img src=x onerror="window.titleInjected=true">';true"#,
    );
    h.wait(|a| a.active_tab().unwrap().info.title.starts_with("<img"));
    h.act(|a| a.publish().unwrap());
    assert_eq!(
        h.eval(
            true,
            "document.querySelector('#tabs-container img:not(.tab-fav)') === null && !window.titleInjected"
        ),
        true
    );
    h.eval(false,"try {window.chrome.webview.postMessage(JSON.stringify({action:'clear_data'}))}catch(e){}; true");
    assert!(!h.app.borrow().clearing);
    eprintln!("Native test: popup with opener and tab identity");
    h.act(|a| {
        let mut prefs = a.saved.preferences.clone();
        prefs.zoom = 125;
        prefs.block_popups = false;
        a.preferences(prefs).unwrap();
    });
    let before = h.app.borrow().tabs.len();
    h.eval(false, "window.popupChild=window.open('/popup'); true");
    h.wait(|a| {
        a.tabs.len() > before
            && a.active_tab()
                .is_some_and(|t| !t.info.loading && t.info.url.ends_with("/popup"))
    });
    assert_eq!(h.eval(false, "window.opener !== null"), true);
    assert_eq!(
        server.hits.load(Ordering::Relaxed),
        0,
        "popup must retain the network filter"
    );
    h.act(|a| {
        use wry::WebViewExtWindows;
        let mut factor = 0.0;
        unsafe {
            a.active_view()
                .unwrap()
                .controller()
                .ZoomFactor(&mut factor)
                .unwrap();
        }
        assert!(
            (factor - 1.25).abs() < 0.001,
            "new tabs must inherit engine zoom"
        );
    });
    let active = h.app.borrow().active;
    h.command(Command::CloseTab { id: original });
    assert_eq!(h.app.borrow().active, active);
    eprintln!("Native test: asynchronous profile clear");
    h.command(Command::ClearData);
    h.wait(|a| !a.clearing && a.notice.starts_with("Đã xóa"));
    assert_eq!(h.app.borrow().tabs.len(), 1);
    h.command(Command::Navigate { input: url.clone() });
    h.wait(|a| {
        a.active_tab()
            .is_some_and(|t| !t.info.loading && t.info.url.ends_with("/state"))
    });
    assert_eq!(
        h.eval(false, "localStorage.getItem('audit')"),
        serde_json::Value::Null
    );
    assert_eq!(h.eval(false, "document.cookie.includes('audit=1')"), false);
    assert_eq!(h.app.borrow().saved.preferences.zoom, 125);
    assert!(!h.app.borrow().saved.bookmarks.is_empty());
    eprintln!("Native test: allowlist, download collision, and saved preferences");
    h.act(|a| {
        let mut prefs = a.saved.preferences.clone();
        prefs.allowed_hosts.push("127.0.0.1".into());
        prefs.startup = "restore".into();
        prefs.engine = "brave".into();
        let root = a.data_dir.join("downloads");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("report.txt"), "keep-existing-file").unwrap();
        prefs.download_dir = root.display().to_string();
        a.preferences(prefs).unwrap();
    });
    h.command(Command::Reload);
    h.wait(|a| {
        a.active_tab().is_some_and(|t| !t.info.loading) && server.hits.load(Ordering::Relaxed) >= 2
    });
    let download = format!("http://127.0.0.1:{}/download", server.port);
    h.command(Command::Navigate { input: download });
    h.wait(|a| a.downloads.iter().any(|line| line.starts_with("Đã tải:")));
    {
        let a = h.app.borrow();
        assert_eq!(
            fs::read_to_string(a.data_dir.join("downloads/report.txt")).unwrap(),
            "keep-existing-file"
        );
        assert_eq!(
            fs::read(a.data_dir.join("downloads/1-report.txt")).unwrap(),
            b"audit-file\r\n"
        );
    }
    h.command(Command::Navigate { input: url.clone() });
    h.wait(|a| {
        a.active_tab()
            .is_some_and(|t| !t.info.loading && t.info.url == url)
    });
    h.command(Command::Bookmark);
    {
        let a = h.app.borrow();
        let disk = SavedState::read(&a.state_path).unwrap();
        assert_eq!(disk.preferences.engine, "brave");
        assert_eq!(disk.preferences.zoom, 125);
        assert!(disk.restored_urls.contains(&url));
        assert!(disk.bookmarks.iter().any(|b| b.url == url));
    }
    println!(
        "PASS: native request blocking, IPC isolation, retained tab document, SPA URL, history, safe title, popup opener, async cookie/storage clear."
    );
}
