use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

pub enum FetchResult {
    Success { url: String, content: String, bytes: usize },
    Error { url: String, message: String },
}

pub struct AsyncFetcher {
    tx: Sender<(String, bool)>, // (url, use_shredder)
    rx: Receiver<FetchResult>,
}

impl AsyncFetcher {
    pub fn new() -> Self {
        let (request_tx, request_rx) = channel::<(String, bool)>();
        let (response_tx, response_rx) = channel::<FetchResult>();

        thread::spawn(move || {
            while let Ok((url, _use_shredder)) = request_rx.recv() {
                if url.starts_with("servo://") {
                    let content = match url.as_str() {
                        "servo://newtab" => "SERVO_NEWTAB".to_string(),
                        "servo://shield" => "SERVO_SHIELD".to_string(),
                        "servo://about" => "SERVO_ABOUT".to_string(),
                        _ => format!("<h1>Trang Nội Bộ Servo</h1><p>URL: {}</p>", url),
                    };
                    let bytes = content.len();
                    let _ = response_tx.send(FetchResult::Success { url, content, bytes });
                    continue;
                }

                // External HTTP/HTTPS request via ureq native Rust TLS
                let target_url = if !url.starts_with("http://") && !url.starts_with("https://") {
                    format!("https://{}", url)
                } else {
                    url.clone()
                };

                // Add standard browser headers so web servers don't reject with 403
                let response = ureq::get(&target_url)
                    .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Papyrus/1.0 Safari/537.36")
                    .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
                    .header("Accept-Language", "vi,en-US;q=0.9,en;q=0.8")
                    .call();

                match response {
                    Ok(mut resp) => match resp.body_mut().read_to_string() {
                        Ok(body) => {
                            let bytes = body.len();
                            let _ = response_tx.send(FetchResult::Success {
                                url: target_url,
                                content: body,
                                bytes,
                            });
                        }
                        Err(e) => {
                            let _ = response_tx.send(FetchResult::Error {
                                url: target_url,
                                message: format!("Lỗi đọc nội dung: {}", e),
                            });
                        }
                    },
                    Err(e) => {
                        let _ = response_tx.send(FetchResult::Error {
                            url: target_url,
                            message: format!("Không thể kết nối đến máy chủ: {}", e),
                        });
                    }
                }
            }
        });

        Self {
            tx: request_tx,
            rx: response_rx,
        }
    }

    pub fn fetch(&self, url: &str, use_shredder: bool) {
        let _ = self.tx.send((url.to_string(), use_shredder));
    }

    pub fn try_recv(&self) -> Option<FetchResult> {
        self.rx.try_recv().ok()
    }
}
