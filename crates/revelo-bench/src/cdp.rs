use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine;
use serde_json::{Value, json};
use tempfile::TempDir;
use tungstenite::{Message, connect};

use crate::util::{Result, err, which};

pub(crate) fn capture_html(
    html: &Path,
    output: &Path,
    selector: &str,
    chrome_path: Option<&Path>,
) -> Result<()> {
    let browser = resolve_browser(chrome_path)?;
    let port = free_port()?;
    let user_data = tempfile::tempdir()?;
    let mut child = launch_browser(&browser, port, &user_data)?;
    let result = capture_with_child(html, output, selector, port);
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn resolve_browser(chrome_path: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = chrome_path {
        if path.is_file() {
            return Ok(path.to_path_buf());
        }
        return Err(err(format!("configured chrome path does not exist: {}", path.display())));
    }
    for candidate in browser_candidates() {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    for binary in ["google-chrome", "chromium", "chromium-browser", "chrome", "msedge"] {
        if let Some(path) = which(binary) {
            return Ok(path);
        }
    }
    Err(err("Chrome/Chromium/Edge not found; pass --chrome-path to render PNG"))
}

fn browser_candidates() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
        PathBuf::from("/Applications/Chromium.app/Contents/MacOS/Chromium"),
        PathBuf::from("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"),
    ]
}

fn free_port() -> Result<u16> {
    Ok(TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port())
}

fn launch_browser(browser: &Path, port: u16, user_data: &TempDir) -> Result<Child> {
    Ok(Command::new(browser)
        .arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--hide-scrollbars")
        .arg(format!("--remote-debugging-port={port}"))
        .arg(format!("--user-data-dir={}", user_data.path().display()))
        .arg("about:blank")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?)
}

fn capture_with_child(html: &Path, output: &Path, selector: &str, port: u16) -> Result<()> {
    let version = wait_for_json(&format!("http://127.0.0.1:{port}/json/version"))?;
    let browser_ws = version
        .get("webSocketDebuggerUrl")
        .and_then(Value::as_str)
        .ok_or_else(|| err("Chrome did not expose webSocketDebuggerUrl"))?;
    let target = new_page(port, html)?;
    let page_ws = target
        .get("webSocketDebuggerUrl")
        .and_then(Value::as_str)
        .ok_or_else(|| err("Chrome target did not expose webSocketDebuggerUrl"))?;
    let _browser_ws = browser_ws;
    capture_page(page_ws, output, selector)
}

fn wait_for_json(url: &str) -> Result<Value> {
    let started = Instant::now();
    loop {
        match ureq::get(url).call() {
            Ok(response) => return Ok(response.into_json()?),
            Err(_) if started.elapsed() < Duration::from_secs(5) => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(err(format!("Chrome CDP endpoint unavailable: {error}"))),
        }
    }
}

fn new_page(port: u16, html: &Path) -> Result<Value> {
    let url = file_url(&html.canonicalize()?);
    let endpoint = format!("http://127.0.0.1:{port}/json/new?{url}");
    match ureq::put(&endpoint).call() {
        Ok(response) => Ok(response.into_json()?),
        Err(error) => Err(err(format!("failed to create Chrome target: {error}"))),
    }
}

fn file_url(path: &Path) -> String {
    let raw = path.to_string_lossy();
    format!("file://{}", percent_encode_path(&raw))
}

fn percent_encode_path(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn capture_page(ws_url: &str, output: &Path, selector: &str) -> Result<()> {
    let (mut socket, _) = connect(ws_url)
        .map_err(|error| err(format!("failed to connect to Chrome CDP websocket: {error}")))?;
    let mut cdp = CdpSession { next_id: 1 };
    cdp.call(&mut socket, "Page.enable", json!({}))?;
    cdp.call(&mut socket, "Runtime.enable", json!({}))?;
    cdp.call(&mut socket, "Runtime.evaluate", json!({"expression": "document.fonts ? document.fonts.ready : Promise.resolve()", "awaitPromise": true}))?;
    let metrics = cdp.call(
        &mut socket,
        "Runtime.evaluate",
        json!({
            "expression": format!(
                r#"(function() {{
                    const el = document.querySelector({selector:?}) || document.documentElement;
                    const rect = el.getBoundingClientRect();
                    return {{x: Math.floor(rect.x), y: Math.floor(rect.y), width: Math.ceil(rect.width), height: Math.ceil(rect.height)}};
                }})()"#
            ),
            "returnByValue": true
        }),
    )?;
    let value = metrics
        .pointer("/result/result/value")
        .ok_or_else(|| err("Chrome did not return capture metrics"))?;
    let width = value.get("width").and_then(Value::as_u64).unwrap_or(1200).max(1);
    let height = value.get("height").and_then(Value::as_u64).unwrap_or(800).max(1);
    cdp.call(
        &mut socket,
        "Emulation.setDeviceMetricsOverride",
        json!({"width": width, "height": height, "deviceScaleFactor": 1, "mobile": false}),
    )?;
    let screenshot = cdp.call(
        &mut socket,
        "Page.captureScreenshot",
        json!({"format": "png", "fromSurface": true, "captureBeyondViewport": true}),
    )?;
    let data = screenshot
        .pointer("/result/data")
        .and_then(Value::as_str)
        .ok_or_else(|| err("Chrome did not return screenshot data"))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|error| err(format!("invalid Chrome screenshot base64: {error}")))?;
    fs::write(output, bytes)?;
    Ok(())
}

struct CdpSession {
    next_id: u64,
}

impl CdpSession {
    fn call(
        &mut self,
        socket: &mut tungstenite::WebSocket<
            tungstenite::stream::MaybeTlsStream<std::net::TcpStream>,
        >,
        method: &str,
        params: Value,
    ) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        socket
            .send(Message::Text(json!({"id": id, "method": method, "params": params}).to_string()))
            .map_err(|error| err(format!("Chrome CDP send failed: {error}")))?;
        loop {
            let message =
                socket.read().map_err(|error| err(format!("Chrome CDP read failed: {error}")))?;
            if let Message::Text(text) = message {
                let value: Value = serde_json::from_str(&text)?;
                if value.get("id").and_then(Value::as_u64) == Some(id) {
                    if let Some(error) = value.get("error") {
                        return Err(err(format!("Chrome CDP method {method} failed: {error}")));
                    }
                    return Ok(value);
                }
            }
        }
    }
}

pub(crate) fn self_test() -> Result<()> {
    assert!(resolve_browser(Some(Path::new("/definitely/missing/chrome"))).is_err());
    assert_eq!(
        file_url(Path::new("/tmp/Revelo Bench/#table?.html")),
        "file:///tmp/Revelo%20Bench/%23table%3F.html"
    );
    Ok(())
}
