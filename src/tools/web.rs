use anyhow::{Context, Result};
use std::time::Duration;
use crate::tools::result_store::ResultStore;

/// Fetches a URL and converts HTML content into clean readable Markdown/text.
pub fn web_fetch(url: &str) -> Result<String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        anyhow::bail!("Invalid URL '{}'. Only 'http://' and 'https://' URLs are supported.", url);
    }

    let response = ureq::get(url)
        .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .set("Accept", "text/html,application/xhtml+xml,application/json,text/plain;q=0.9,*/*;q=0.8")
        .timeout(Duration::from_secs(15))
        .call()
        .with_context(|| format!("Failed to fetch URL: '{}'", url))?;

    let content_type = response.content_type().to_lowercase();
    let body = response.into_string()
        .with_context(|| format!("Failed to read response body from '{}'", url))?;

    let formatted = if content_type.contains("json") {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&body) {
            serde_json::to_string_pretty(&val).unwrap_or(body)
        } else {
            body
        }
    } else if content_type.contains("html") || body.contains("<html") || body.contains("<!DOCTYPE") {
        html_to_markdown(&body)
    } else {
        body
    };

    let title = format!("--- Web Content: {} ---\n\n", url);
    let full_content = format!("{}{}", title, formatted);

    Ok(ResultStore::process_output(full_content, 350, 25000))
}

/// Queries DuckDuckGo to search the web for documentation and solutions.
pub fn web_search(query: &str, num_results: Option<usize>) -> Result<String> {
    let limit = num_results.unwrap_or(5).clamp(1, 10);
    let encoded_query = url_encode(query);

    // 1. Try DuckDuckGo HTML search
    let html_url = format!("https://html.duckduckgo.com/html/?q={}", encoded_query);
    let resp = ureq::post(&html_url)
        .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .set("Content-Type", "application/x-www-form-urlencoded")
        .timeout(Duration::from_secs(15))
        .send_string(&format!("q={}", encoded_query));

    if let Ok(response) = resp {
        if let Ok(body) = response.into_string() {
            let results = parse_duckduckgo_html(&body, limit);
            if !results.is_empty() {
                let mut out = format!("### Web Search Results for: \"{}\"\n\n", query);
                for (idx, r) in results.iter().enumerate() {
                    out.push_str(&format!("{}. [{}]({})\n   {}\n\n", idx + 1, r.title, r.url, r.snippet));
                }
                return Ok(out);
            }
        }
    }

    // 2. Fallback: DuckDuckGo Instant Answer API
    let api_url = format!("https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=1", encoded_query);
    if let Ok(response) = ureq::get(&api_url)
        .set("User-Agent", "ctrl-cli/0.2.0")
        .timeout(Duration::from_secs(10))
        .call()
    {
        if let Ok(json_str) = response.into_string() {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str) {
                let mut items = Vec::new();

                let heading = val["Heading"].as_str().unwrap_or("");
                let abstract_txt = val["AbstractText"].as_str().unwrap_or("");
                let abstract_url = val["AbstractURL"].as_str().unwrap_or("");

                if !abstract_txt.is_empty() {
                    items.push((heading.to_string(), abstract_url.to_string(), abstract_txt.to_string()));
                }

                if let Some(topics) = val["RelatedTopics"].as_array() {
                    for t in topics {
                        if items.len() >= limit {
                            break;
                        }
                        if let (Some(text), Some(url)) = (t["Text"].as_str(), t["FirstURL"].as_str()) {
                            let title = text.split(" - ").next().unwrap_or(text);
                            items.push((title.to_string(), url.to_string(), text.to_string()));
                        }
                    }
                }

                if !items.is_empty() {
                    let mut out = format!("### Web Search Results for: \"{}\"\n\n", query);
                    for (idx, (title, url, snip)) in items.iter().enumerate() {
                        out.push_str(&format!("{}. [{}]({})\n   {}\n\n", idx + 1, title, url, snip));
                    }
                    return Ok(out);
                }
            }
        }
    }

    Ok(format!("No search results found for query: '{}'. Try refining the search terms.", query))
}

struct SearchItem {
    title: String,
    url: String,
    snippet: String,
}

fn parse_duckduckgo_html(html: &str, limit: usize) -> Vec<SearchItem> {
    let mut items = Vec::new();

    // DuckDuckGo HTML results are wrapped in <div class="result ..."> or have class="result__snippet"
    for block in html.split("class=\"result results_links") {
        if items.len() >= limit {
            break;
        }

        // Find result__a (main title link)
        let link_marker = "class=\"result__a\"";
        let (title, raw_url) = if let Some(pos) = block.find(link_marker) {
            let after = &block[pos..];
            let href_pos = after.find("href=\"").map(|p| p + 6).unwrap_or(0);
            let href_end = after[href_pos..].find('"').map(|p| href_pos + p).unwrap_or(0);
            let url = &after[href_pos..href_end];

            let text_start = after.find('>').map(|p| p + 1).unwrap_or(0);
            let text_end = after[text_start..].find("</a>").map(|p| text_start + p).unwrap_or(0);
            let title = strip_tags(&after[text_start..text_end]);
            (title, url.to_string())
        } else {
            continue;
        };

        // Resolve clean URL if redirect format /l/?uddg=...
        let clean_url = extract_ddg_redirect_url(&raw_url);

        // Find snippet
        let snippet = if let Some(pos) = block.find("class=\"result__snippet\"") {
            let after = &block[pos..];
            let text_start = after.find('>').map(|p| p + 1).unwrap_or(0);
            let text_end = after[text_start..].find("</a>").map(|p| text_start + p).unwrap_or(0);
            strip_tags(&after[text_start..text_end])
        } else {
            String::new()
        };

        if !title.trim().is_empty() && !clean_url.trim().is_empty() {
            items.push(SearchItem {
                title: title.trim().to_string(),
                url: clean_url.trim().to_string(),
                snippet: snippet.trim().to_string(),
            });
        }
    }

    items
}

fn extract_ddg_redirect_url(raw: &str) -> String {
    if let Some(pos) = raw.find("uddg=") {
        let after = &raw[pos + 5..];
        let end = after.find('&').unwrap_or(after.len());
        let encoded = &after[..end];
        url_decode(encoded)
    } else if raw.starts_with("//") {
        format!("https:{}", raw)
    } else {
        raw.to_string()
    }
}

pub fn html_to_markdown(html: &str) -> String {
    let mut cleaned = String::new();
    let mut current_link: Option<String> = None;

    // 1. Remove script, style, svg, noscript
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        cleaned.push_str(&rest[..start]);
        let tag_rest = &rest[start..];
        if let Some(end) = tag_rest.find('>') {
            let full_tag = tag_rest[1..end].trim();
            let tag_content = full_tag.to_lowercase();
            let tag_name = tag_content.split_whitespace().next().unwrap_or("");

            if tag_name == "script" || tag_name == "style" || tag_name == "svg" || tag_name == "noscript" {
                let close_tag = format!("</{}>", tag_name);
                if let Some(close_pos) = tag_rest.to_lowercase().find(&close_tag) {
                    rest = &tag_rest[close_pos + close_tag.len()..];
                    continue;
                }
            }

            if tag_name == "a" {
                let href = extract_attribute(full_tag, "href");
                if let Some(u) = href {
                    if !u.starts_with('#') && !u.starts_with("javascript:") {
                        current_link = Some(u);
                        cleaned.push('[');
                    }
                }
            } else if tag_name == "/a" {
                if let Some(href) = current_link.take() {
                    cleaned.push_str(&format!("]({})", href));
                }
            } else {
                match tag_name {
                    "h1" => cleaned.push_str("\n\n# "),
                    "h2" => cleaned.push_str("\n\n## "),
                    "h3" => cleaned.push_str("\n\n### "),
                    "h4" => cleaned.push_str("\n\n#### "),
                    "h5" | "h6" => cleaned.push_str("\n\n##### "),
                    "p" | "div" | "section" | "article" => cleaned.push_str("\n\n"),
                    "br" => cleaned.push('\n'),
                    "li" => cleaned.push_str("\n- "),
                    "b" | "strong" | "/b" | "/strong" => cleaned.push_str("**"),
                    "i" | "em" | "/i" | "/em" => cleaned.push('*'),
                    "code" | "/code" => cleaned.push('`'),
                    "pre" | "/pre" => cleaned.push_str("\n```\n"),
                    "blockquote" => cleaned.push_str("\n> "),
                    "hr" => cleaned.push_str("\n---\n"),
                    "th" | "td" => cleaned.push_str(" | "),
                    _ => {}
                }
            }

            rest = &tag_rest[end + 1..];
        } else {
            cleaned.push_str(tag_rest);
            break;
        }
    }

    decode_html_entities(&cleaned)
}

fn extract_attribute(tag: &str, attr: &str) -> Option<String> {
    let lower_tag = tag.to_lowercase();
    let needle_dq = format!("{}=\"", attr);
    let needle_sq = format!("{}='", attr);

    if let Some(pos) = lower_tag.find(&needle_dq) {
        let after = &tag[pos + needle_dq.len()..];
        if let Some(end) = after.find('"') {
            return Some(after[..end].to_string());
        }
    } else if let Some(pos) = lower_tag.find(&needle_sq) {
        let after = &tag[pos + needle_sq.len()..];
        if let Some(end) = after.find('\'') {
            return Some(after[..end].to_string());
        }
    }
    None
}

fn strip_tags(html: &str) -> String {
    let mut result = String::new();
    let mut inside = false;
    for ch in html.chars() {
        if ch == '<' {
            inside = true;
        } else if ch == '>' {
            inside = false;
        } else if !inside {
            result.push(ch);
        }
    }
    decode_html_entities(&result)
}

fn decode_html_entities(text: &str) -> String {
    let mut s = text.to_string();
    s = s.replace("&nbsp;", " ");
    s = s.replace("&amp;", "&");
    s = s.replace("&lt;", "<");
    s = s.replace("&gt;", ">");
    s = s.replace("&quot;", "\"");
    s = s.replace("&#39;", "'");
    s = s.replace("&apos;", "'");
    s = s.replace("&#x27;", "'");
    s = s.replace("&#x2F;", "/");

    // Clean multiple consecutive blank lines
    let mut lines = Vec::new();
    let mut consecutive_blanks = 0;
    for line in s.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            consecutive_blanks += 1;
            if consecutive_blanks <= 2 {
                lines.push("");
            }
        } else {
            consecutive_blanks = 0;
            lines.push(line.trim_end());
        }
    }
    lines.join("\n").trim().to_string()
}

fn url_encode(input: &str) -> String {
    let mut encoded = String::new();
    for byte in input.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push('+'),
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
}

fn url_decode(input: &str) -> String {
    let mut bytes = Vec::new();
    let mut chars = input.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars.next().unwrap_or(0);
            let h2 = chars.next().unwrap_or(0);
            let hex_str = format!("{}{}", h1 as char, h2 as char);
            if let Ok(val) = u8::from_str_radix(&hex_str, 16) {
                bytes.push(val);
            }
        } else if b == b'+' {
            bytes.push(b' ');
        } else {
            bytes.push(b);
        }
    }
    String::from_utf8_lossy(&bytes).to_string()
}
