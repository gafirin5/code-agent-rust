use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SearchResult {
    pub document: String,
    pub section: String,
    pub snippet: String,
    pub score: f64,
}

#[derive(Clone, Debug)]
pub struct KnowledgeChunk {
    pub document: String,
    pub section: String,
    pub snippet: String,
    pub term_frequencies: HashMap<String, u32>,
    pub doc_length: usize,
}

pub const STOP_WORDS: &[&str] = &[
    // English
    "the", "is", "at", "which", "on", "and", "a", "an", "in", "to", "of", "for", "with",
    "or", "as", "by", "that", "this", "it", "from", "be", "are", "was", "were", "all",
    "can", "has", "have", "had", "not", "but", "what", "where", "when", "who", "how",
    // Indonesian
    "dan", "di", "ke", "dari", "yang", "ini", "itu", "untuk", "pada", "adalah", "sebagai",
    "dengan", "atau", "oleh", "juga", "akan", "bisa", "ada", "tidak", "apa", "bagaimana",
    "karena", "agar", "setiap", "dalam", "harus", "serta", "para", "seluruh",
];

/// Tokenizes text into lowercase terms, splitting on non-alphanumeric unicode characters,
/// and filters out stop words and short tokens (< 2 chars).
pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| s.len() >= 2 && !STOP_WORDS.contains(&s.as_str()))
        .collect()
}

/// Tokenizes a search query. If stop word filtering produces an empty list,
/// falls back to all alphanumeric terms to ensure queries like common words still match.
pub fn tokenize_query(query: &str) -> Vec<String> {
    let filtered = tokenize(query);
    if !filtered.is_empty() {
        filtered
    } else {
        query
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect()
    }
}

/// Chunks markdown documents by section headers (`#`, `##`, `###`, `####`).
pub fn chunk_markdown(filename: &str, content: &str) -> Vec<KnowledgeChunk> {
    let mut chunks = Vec::new();
    let mut current_section = String::from("Overview");
    let mut current_lines: Vec<&str> = Vec::new();

    let flush_chunk = |chunks: &mut Vec<KnowledgeChunk>, section: &str, lines: &[&str]| {
        let snippet = lines.join("\n").trim().to_string();
        if !snippet.is_empty() {
            let tokens = tokenize(&snippet);
            let doc_length = tokens.len();
            let mut term_frequencies: HashMap<String, u32> = HashMap::new();
            for t in tokens {
                *term_frequencies.entry(t).or_insert(0) += 1;
            }

            chunks.push(KnowledgeChunk {
                document: filename.to_string(),
                section: section.to_string(),
                snippet,
                term_frequencies,
                doc_length,
            });
        }
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            // Count leading hashes
            let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
            if hash_count <= 4 {
                // Header line: flush existing accumulator if it has content
                if !current_lines.is_empty() {
                    flush_chunk(&mut chunks, &current_section, &current_lines);
                    current_lines.clear();
                }
                current_section = trimmed[hash_count..].trim().to_string();
                current_lines.push(line);
                continue;
            }
        }
        current_lines.push(line);
    }

    if !current_lines.is_empty() {
        flush_chunk(&mut chunks, &current_section, &current_lines);
    }

    chunks
}

/// Pure Rust Okapi BM25 ranking index ($k_1 = 1.2, b = 0.75$).
pub struct Bm25Index {
    pub chunks: Vec<KnowledgeChunk>,
    pub doc_frequencies: HashMap<String, usize>,
    pub avg_doc_length: f64,
}

impl Bm25Index {
    /// Builds an in-process BM25 index from all markdown (`.md`) files in `dir`.
    pub fn build_from_dir(dir: &Path) -> Result<Self> {
        let mut chunks = Vec::new();
        if dir.is_dir() {
            let mut entries: Vec<_> = std::fs::read_dir(dir)?
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path().is_file()
                        && e.path().extension().is_some_and(|ext| ext == "md")
                })
                .collect();
            // Sort deterministically
            entries.sort_by_key(|e| e.path());

            for entry in entries {
                let path = entry.path();
                let filename = path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown.md".to_string());
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let file_chunks = chunk_markdown(&filename, &content);
                    chunks.extend(file_chunks);
                }
            }
        }

        let total_docs = chunks.len();
        let mut doc_frequencies = HashMap::new();
        let mut total_length = 0usize;

        for chunk in &chunks {
            total_length += chunk.doc_length;
            for term in chunk.term_frequencies.keys() {
                *doc_frequencies.entry(term.clone()).or_insert(0) += 1;
            }
        }

        let avg_doc_length = if total_docs > 0 {
            total_length as f64 / total_docs as f64
        } else {
            0.0
        };

        Ok(Self {
            chunks,
            doc_frequencies,
            avg_doc_length,
        })
    }

    /// Searches the BM25 index and returns up to `top_k` ranked `SearchResult`s.
    pub fn search(&self, query: &str, top_k: usize) -> Vec<SearchResult> {
        if top_k == 0 || self.chunks.is_empty() || query.trim().is_empty() {
            return Vec::new();
        }

        let query_tokens = tokenize_query(query);
        if query_tokens.is_empty() {
            return Vec::new();
        }

        // Deduplicate query tokens to compute score per unique query term
        let mut unique_query_terms: Vec<String> = Vec::new();
        for t in query_tokens {
            if !unique_query_terms.contains(&t) {
                unique_query_terms.push(t);
            }
        }

        let k1: f64 = 1.2;
        let b: f64 = 0.75;
        let n = self.chunks.len() as f64;

        let mut scored_chunks: Vec<SearchResult> = Vec::new();

        for chunk in &self.chunks {
            let mut score: f64 = 0.0;
            let doc_len = chunk.doc_length as f64;
            let len_norm = if self.avg_doc_length > 0.0 {
                doc_len / self.avg_doc_length
            } else {
                1.0
            };

            for q_term in &unique_query_terms {
                if let Some(&tf_count) = chunk.term_frequencies.get(q_term) {
                    let tf = tf_count as f64;
                    let df = *self.doc_frequencies.get(q_term).unwrap_or(&0) as f64;

                    // Robertson-Spärck Jones IDF with positive floor protection
                    let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
                    // Term frequency saturation
                    let tf_norm = (tf * (k1 + 1.0)) / (tf + k1 * (1.0 - b + b * len_norm));

                    score += idf * tf_norm;
                }
            }

            if score > 0.0 {
                scored_chunks.push(SearchResult {
                    document: chunk.document.clone(),
                    section: chunk.section.clone(),
                    snippet: chunk.snippet.clone(),
                    score,
                });
            }
        }

        // Sort descending by score; if tied, sort deterministically by document and section
        scored_chunks.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.document.cmp(&b.document))
                .then_with(|| a.section.cmp(&b.section))
        });

        if scored_chunks.len() > top_k {
            scored_chunks.truncate(top_k);
        }

        scored_chunks
    }
}

/// Resolves the `data/knowledge` directory, checking direct workspace path or parent fallback.
pub fn resolve_knowledge_dir(workspace_root: &Path) -> PathBuf {
    let direct = workspace_root.join("data").join("knowledge");
    if direct.is_dir() {
        return direct;
    }
    let parent = workspace_root.join("..").join("data").join("knowledge");
    if parent.is_dir() {
        return parent;
    }
    direct
}

/// Searches the knowledge base in `workspace_root/data/knowledge/` using in-process BM25.
pub fn search_knowledge(
    workspace_root: &Path,
    query: &str,
    top_k: usize,
) -> Result<Vec<SearchResult>> {
    let dir = resolve_knowledge_dir(workspace_root);
    let index = Bm25Index::build_from_dir(&dir)?;
    let top_k = if top_k == 0 { 3 } else { top_k.min(10) };
    Ok(index.search(query, top_k))
}

/// Formats a list of search results into human- and LLM-readable Markdown.
pub fn format_search_results(results: &[SearchResult]) -> String {
    if results.is_empty() {
        return "No relevant documents found in knowledge base.".to_string();
    }

    let mut out = format!(
        "Found {} relevant excerpt(s) in knowledge base:\n\n",
        results.len()
    );
    for (i, res) in results.iter().enumerate() {
        out.push_str(&format!(
            "--- [{}] Document: {} | Section: \"{}\" | Score: {:.2} ---\n{}\n\n",
            i + 1,
            res.document,
            res.section,
            res.score,
            res.snippet.trim()
        ));
    }
    out.trim_end().to_string()
}

/// Top-level agent tool invocation function for `knowledge_search`.
pub fn run_knowledge_search(query: &str, top_k: usize) -> Result<String> {
    let root = crate::tools::filesystem::get_workspace_root();
    let results = search_knowledge(&root, query, top_k)?;
    Ok(format_search_results(&results))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_and_stop_words() {
        let text = "Keamanan sistem dan API Key rahasia untuk semua pengguna";
        let tokens = tokenize(text);
        assert!(tokens.contains(&"keamanan".to_string()));
        assert!(tokens.contains(&"sistem".to_string()));
        assert!(tokens.contains(&"api".to_string()));
        assert!(tokens.contains(&"key".to_string()));
        assert!(tokens.contains(&"rahasia".to_string()));
        assert!(tokens.contains(&"pengguna".to_string()));
        // Stop words like "dan", "untuk" should be filtered out
        assert!(!tokens.contains(&"dan".to_string()));
        assert!(!tokens.contains(&"untuk".to_string()));
    }

    #[test]
    fn test_bm25_empty_query_and_corpus() {
        let index = Bm25Index {
            chunks: Vec::new(),
            doc_frequencies: HashMap::new(),
            avg_doc_length: 0.0,
        };
        let res = index.search("keamanan", 3);
        assert!(res.is_empty());

        let res2 = index.search("", 3);
        assert!(res2.is_empty());
    }

    #[test]
    fn test_chunk_markdown_headers() {
        let content = r#"# Document Title
Preamble text here.

## Section 1: Security
Security guidelines and policy details.

## Section 2: Concurrency
Avoid deadlocks and use standard primitives.
"#;
        let chunks = chunk_markdown("test.md", content);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].section, "Document Title");
        assert_eq!(chunks[1].section, "Section 1: Security");
        assert_eq!(chunks[2].section, "Section 2: Concurrency");
    }

    #[test]
    fn test_bm25_ranking_company_policy_security() {
        let root = Path::new(".");
        let dir = resolve_knowledge_dir(root);
        let index = Bm25Index::build_from_dir(&dir).expect("Failed to build index");
        assert!(!index.chunks.is_empty(), "Chunks should not be empty");

        let results = index.search("keamanan akses sistem API Key", 3);
        assert!(!results.is_empty(), "Expected matches for security query");
        let top = &results[0];
        assert_eq!(top.document, "company_policy.md");
        assert!(top.section.contains("Keamanan") || top.section.contains("Akses Sistem"));
        assert!(top.score > 0.0);
    }

    #[test]
    fn test_bm25_ranking_rust_quality_standards() {
        let root = Path::new(".");
        let dir = resolve_knowledge_dir(root);
        let index = Bm25Index::build_from_dir(&dir).expect("Failed to build index");

        let results = index.search("standar kualitas kode rust zero warnings", 3);
        assert!(!results.is_empty());
        let top = &results[0];
        assert_eq!(top.document, "company_policy.md");
        assert!(top.section.contains("Standar Kualitas Kode") || top.snippet.contains("Zero Warnings"));
    }

    #[test]
    fn test_bm25_ranking_product_faqs_architecture() {
        let root = Path::new(".");
        let dir = resolve_knowledge_dir(root);
        let index = Bm25Index::build_from_dir(&dir).expect("Failed to build index");

        let results = index.search("keunggulan ctrl-cli dibandingkan framework python", 3);
        assert!(!results.is_empty());
        let top = &results[0];
        assert_eq!(top.document, "product_faqs.md");
        assert!(top.snippet.contains("Biner Sangat Ringan") || top.snippet.contains("1.8 MB"));
    }

    #[test]
    fn test_run_knowledge_search_formatting() {
        let out = run_knowledge_search("subagent logging tasklogbuffer", 2).expect("Search should succeed");
        assert!(out.contains("Found") || out.contains("relevant excerpt"));
        assert!(out.contains("product_faqs.md") || out.contains("company_policy.md"));
    }

    #[test]
    fn test_bm25_top_k_zero_returns_empty() {
        let root = Path::new(".");
        let dir = resolve_knowledge_dir(root);
        let index = Bm25Index::build_from_dir(&dir).expect("Failed to build index");
        let results = index.search("keamanan", 0);
        assert!(results.is_empty(), "top_k = 0 must return an empty vector");
    }

    #[test]
    fn test_bm25_build_from_dir_skips_corrupted_file() {
        use std::io::Write;
        let temp_dir = std::env::temp_dir().join(format!("test_bm25_corrupt_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        // Valid markdown file
        let valid_file = temp_dir.join("valid.md");
        std::fs::write(&valid_file, "# Title\nThis is valid content.").unwrap();

        // Corrupted markdown file containing non-UTF8 binary bytes
        let corrupt_file = temp_dir.join("corrupt.md");
        let mut f = std::fs::File::create(&corrupt_file).unwrap();
        f.write_all(&[0xFF, 0xFE, 0xFD, 0x00, 0x80]).unwrap();
        drop(f);

        let index = Bm25Index::build_from_dir(&temp_dir).expect("Should succeed despite corrupted file");
        assert_eq!(index.chunks.len(), 1, "Only valid file should be indexed");
        assert_eq!(index.chunks[0].document, "valid.md");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

