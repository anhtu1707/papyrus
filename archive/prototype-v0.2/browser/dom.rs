#[derive(Clone, Debug)]
pub enum DomNode {
    Heading { level: u8, text: String },
    Paragraph(String),
    Link { label: String, url: String },
    CodeBlock(String),
    BlockQuote(String),
    AdBlockedPlaceholder { domain: String, reason: String },
    Divider,
    MetricRow { label: String, val_rust: String, val_chrome: String, val_tor: String },
}

#[derive(Clone, Debug)]
pub struct SimpleDom {
    pub title: String,
    pub nodes: Vec<DomNode>,
}

impl SimpleDom {
    pub fn parse_html_or_text(raw: &str) -> Self {
        let mut title = "Trang Web".to_string();
        let mut nodes = Vec::new();

        // 1. Strip raw script tags and style tags so their contents don't pollute the view
        let sanitized = strip_scripts_and_styles(raw);

        // 2. Format single-line HTML by splitting key elements
        let formatted = sanitized
            .replace("<h1>", "\n<h1>")
            .replace("<h2>", "\n<h2>")
            .replace("<h3>", "\n<h3>")
            .replace("<p>", "\n<p>")
            .replace("<pre>", "\n<pre>")
            .replace("<hr>", "\n<hr>")
            .replace("<li", "\n• <li")
            .replace("<a ", "\n<a ");

        let lines: Vec<&str> = formatted.lines().collect();
        for line in lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if trimmed.contains("<title>") {
                if let Some(start) = trimmed.find("<title>") {
                    if let Some(end) = trimmed.find("</title>") {
                        title = trimmed[start + 7..end].trim().to_string();
                        continue;
                    }
                }
            }

            if trimmed.starts_with("# ") || trimmed.starts_with("<h1>") {
                let text = clean_tags(trimmed);
                if !text.is_empty() {
                    nodes.push(DomNode::Heading { level: 1, text });
                }
            } else if trimmed.starts_with("## ") || trimmed.starts_with("<h2>") {
                let text = clean_tags(trimmed);
                if !text.is_empty() {
                    nodes.push(DomNode::Heading { level: 2, text });
                }
            } else if trimmed.starts_with("### ") || trimmed.starts_with("<h3>") {
                let text = clean_tags(trimmed);
                if !text.is_empty() {
                    nodes.push(DomNode::Heading { level: 3, text });
                }
            } else if trimmed.starts_with("```") || trimmed.starts_with("<pre>") {
                let code = clean_tags(trimmed);
                if !code.is_empty() {
                    nodes.push(DomNode::CodeBlock(code));
                }
            } else if trimmed.starts_with("> ") {
                nodes.push(DomNode::BlockQuote(trimmed.trim_start_matches("> ").to_string()));
            } else if trimmed.starts_with("---") || trimmed.starts_with("<hr>") {
                nodes.push(DomNode::Divider);
            } else if trimmed.contains("href=") {
                let label = clean_tags(trimmed);
                let url = extract_href(trimmed).unwrap_or_else(|| "https://rust-lang.org".to_string());
                if !label.is_empty() && label.len() > 1 {
                    nodes.push(DomNode::Link { label, url });
                }
            } else {
                let cleaned = clean_tags(trimmed);
                // Filter out CSS braces or minified artifacts
                if !cleaned.is_empty() && !cleaned.starts_with('{') && !cleaned.ends_with('}') && cleaned.len() > 2 {
                    nodes.push(DomNode::Paragraph(cleaned));
                }
            }
        }

        if nodes.is_empty() {
            nodes.push(DomNode::Heading {
                level: 1,
                text: "Trang Trống".to_string(),
            });
            nodes.push(DomNode::Paragraph("Không có nội dung hiển thị.".to_string()));
        }

        Self { title, nodes }
    }
}

fn strip_scripts_and_styles(html: &str) -> String {
    let mut s = html.to_string();
    // Strip scripts
    while let Some(start) = s.to_lowercase().find("<script") {
        if let Some(end) = s.to_lowercase()[start..].find("</script>") {
            s.replace_range(start..start + end + 9, " ");
        } else {
            break;
        }
    }
    // Strip styles
    while let Some(start) = s.to_lowercase().find("<style") {
        if let Some(end) = s.to_lowercase()[start..].find("</style>") {
            s.replace_range(start..start + end + 8, " ");
        } else {
            break;
        }
    }
    s
}

fn clean_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;

    for c in input.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(c);
        }
    }

    out.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

fn extract_href(input: &str) -> Option<String> {
    if let Some(idx) = input.find("href=\"") {
        let rest = &input[idx + 6..];
        if let Some(end) = rest.find('\"') {
            return Some(rest[..end].to_string());
        }
    } else if let Some(idx) = input.find("href='") {
        let rest = &input[idx + 6..];
        if let Some(end) = rest.find('\'') {
            return Some(rest[..end].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dom_parsing() {
        let sample = "<html><head><title>Servo Test</title></head><body><h1>Hello Rust</h1><p>Test paragraph</p><a href=\"https://servo.org\">Servo Link</a></body></html>";
        let dom = SimpleDom::parse_html_or_text(sample);

        assert_eq!(dom.title, "Servo Test");
        assert_eq!(dom.nodes.len(), 3);
        match &dom.nodes[0] {
            DomNode::Heading { level, text } => {
                assert_eq!(*level, 1);
                assert_eq!(text, "Hello Rust");
            }
            _ => panic!("Expected Heading"),
        }
    }

    #[test]
    fn test_script_stripping() {
        let html_with_script = "<div><h1>Article</h1><script>console.log('malicious tracker');</script><p>Clean content</p></div>";
        let dom = SimpleDom::parse_html_or_text(html_with_script);
        for node in dom.nodes {
            if let DomNode::Paragraph(p) = node {
                assert!(!p.contains("console.log"));
            }
        }
    }
}
