//! Empirical Challenge Test Harness for pure Rust in-process BM25 Engine
//! Target: `ctrl-cli/src/tools/knowledge.rs`
//!
//! Verifies:
//! 1. BM25 scoring formula, monotonicity, asymptotic saturation, and length normalization.
//! 2. RSJ smoothed IDF formula, rarity weighting, and non-negativity across all document frequencies.
//! 3. Document ranking accuracy and consistency across Indonesian and English queries.
//! 4. Boundary cases, zero division, empty corpus, empty queries, stopword-only queries, and unicode/emojis.
//! 5. Extreme scale robustness: high term frequency, long documents, concurrent reads across threads.
//! 6. Latency benchmark: sub-millisecond per-query response time (< 1.0 ms) across 1,000 iterations.
//! 7. End-to-end tool formatting and integration functions.

pub mod tools {
    pub mod filesystem {
        use std::path::PathBuf;
        pub fn get_workspace_root() -> PathBuf {
            let mut curr = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            for _ in 0..5 {
                if curr.join("data").join("knowledge").exists() {
                    return curr;
                }
                if let Some(parent) = curr.parent() {
                    curr = parent.to_path_buf();
                } else {
                    break;
                }
            }
            PathBuf::from(".")
        }
    }
}

#[path = "../src/tools/knowledge.rs"]
mod knowledge;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use knowledge::{
    chunk_markdown, format_search_results, resolve_knowledge_dir, run_knowledge_search,
    search_knowledge, tokenize, tokenize_query, Bm25Index, KnowledgeChunk, SearchResult, STOP_WORDS,
};

/// Helper: find workspace root directory containing `data/knowledge/`.
fn find_workspace_root() -> PathBuf {
    let mut curr = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for _ in 0..5 {
        if curr.join("data").join("knowledge").exists() {
            return curr;
        }
        if let Some(parent) = curr.parent() {
            curr = parent.to_path_buf();
        } else {
            break;
        }
    }
    PathBuf::from(".")
}

// ============================================================================
// SUITE 1: MATHEMATICAL RIGOR, SCORING FORMULA & MONOTONICITY
// ============================================================================

#[test]
fn challenge_bm25_term_frequency_strict_monotonicity() {
    // Monotonicity property: as TF increases for a fixed document length and corpus,
    // the BM25 score MUST strictly increase: S(tf_1) < S(tf_2) for tf_1 < tf_2.
    let tfs = [1, 2, 3, 5, 8, 13, 20, 50, 100, 250, 500, 1000];
    let fixed_doc_len = 1500usize;
    let mut chunks = Vec::new();

    for (i, &tf) in tfs.iter().enumerate() {
        let mut term_frequencies = HashMap::new();
        term_frequencies.insert("performance".to_string(), tf as u32);
        term_frequencies.insert("padding".to_string(), (fixed_doc_len - tf) as u32);

        chunks.push(KnowledgeChunk {
            document: format!("doc_{}.md", i),
            section: format!("TF_{}", tf),
            snippet: format!("Performance repeated {} times", tf),
            term_frequencies,
            doc_length: fixed_doc_len,
        });
    }

    let mut doc_frequencies = HashMap::new();
    doc_frequencies.insert("performance".to_string(), tfs.len());
    doc_frequencies.insert("padding".to_string(), tfs.len());

    let index = Bm25Index {
        chunks,
        doc_frequencies,
        avg_doc_length: fixed_doc_len as f64,
    };

    let results = index.search("performance", tfs.len());
    assert_eq!(results.len(), tfs.len());

    // Results must be sorted descending: highest TF at index 0, lowest at the end
    for i in 0..results.len() - 1 {
        let score_current = results[i].score;
        let score_next = results[i + 1].score;
        assert!(
            score_current > score_next,
            "Monotonicity violation: rank {} score {:.6} <= rank {} score {:.6}",
            i,
            score_current,
            i + 1,
            score_next
        );
    }

    // Explicitly check that S(1) < S(2) < S(3) < ...
    let mut scores_by_tf: Vec<(usize, f64)> = Vec::new();
    for &tf in &tfs {
        let single_res = index
            .search("performance", tfs.len())
            .into_iter()
            .find(|r| r.section == format!("TF_{}", tf))
            .expect("Should find result for TF");
        scores_by_tf.push((tf, single_res.score));
    }

    for window in scores_by_tf.windows(2) {
        let (tf1, s1) = window[0];
        let (tf2, s2) = window[1];
        assert!(
            s1 < s2,
            "Strict monotonicity failure: S(tf={}) = {:.6} >= S(tf={}) = {:.6}",
            tf1,
            s1,
            tf2,
            s2
        );
    }
}

#[test]
fn challenge_bm25_diminishing_returns_saturation() {
    // Asymptotic saturation property:
    // Marginal score gain Delta(tf -> tf+1) must strictly decrease as TF grows:
    // Delta(1->2) > Delta(2->3) > Delta(3->4)...
    // And score must asymptotically approach (k1 + 1) * IDF = 2.2 * IDF.
    let doc_len = 1000usize;
    let mut chunks = Vec::new();

    for tf in 1..=10u32 {
        let mut tf_map = HashMap::new();
        tf_map.insert("latency".to_string(), tf);
        chunks.push(KnowledgeChunk {
            document: "doc.md".to_string(),
            section: format!("TF_{}", tf),
            snippet: "latency".to_string(),
            term_frequencies: tf_map,
            doc_length: doc_len,
        });
    }

    let mut df_map = HashMap::new();
    df_map.insert("latency".to_string(), 10);

    let index = Bm25Index {
        chunks,
        doc_frequencies: df_map,
        avg_doc_length: doc_len as f64,
    };

    let mut scores = Vec::new();
    for tf in 1..=10 {
        let r = index
            .search("latency", 10)
            .into_iter()
            .find(|x| x.section == format!("TF_{}", tf))
            .unwrap();
        scores.push(r.score);
    }

    let mut marginal_gains = Vec::new();
    for i in 0..scores.len() - 1 {
        marginal_gains.push(scores[i + 1] - scores[i]);
    }

    for i in 0..marginal_gains.len() - 1 {
        assert!(
            marginal_gains[i] > marginal_gains[i + 1],
            "Diminishing returns failure: delta({}->{}) = {:.6} <= delta({}->{}) = {:.6}",
            i + 1,
            i + 2,
            marginal_gains[i],
            i + 2,
            i + 3,
            marginal_gains[i + 1]
        );
    }

    // Theoretical maximum score check:
    // IDF = ln((10 - 10 + 0.5)/(10 + 0.5) + 1.0) = ln(0.5/10.5 + 1) = ln(11.0 / 10.5)
    let n = 10.0f64;
    let df = 10.0f64;
    let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
    let max_possible_score = idf * (1.2 + 1.0);

    for s in &scores {
        assert!(
            *s < max_possible_score,
            "Score {:.6} exceeded theoretical upper bound {:.6}",
            s,
            max_possible_score
        );
    }
}

#[test]
fn challenge_bm25_document_length_normalization_penalty() {
    // Length penalization: For identical term frequency of query terms,
    // a shorter document must strictly receive a higher score than a longer document.
    let lengths = [20usize, 50, 100, 200, 500, 1000];
    let mut chunks = Vec::new();

    for (i, &len) in lengths.iter().enumerate() {
        let mut tf = HashMap::new();
        tf.insert("security".to_string(), 2); // Identical TF = 2
        chunks.push(KnowledgeChunk {
            document: format!("doc_{}.md", i),
            section: format!("LEN_{}", len),
            snippet: "security".to_string(),
            term_frequencies: tf,
            doc_length: len,
        });
    }

    let avg_len: f64 = lengths.iter().sum::<usize>() as f64 / lengths.len() as f64;
    let mut df = HashMap::new();
    df.insert("security".to_string(), lengths.len());

    let index = Bm25Index {
        chunks,
        doc_frequencies: df,
        avg_doc_length: avg_len,
    };

    let results = index.search("security", lengths.len());
    assert_eq!(results.len(), lengths.len());

    // Shorter docs must score strictly higher
    for i in 0..results.len() - 1 {
        assert!(
            results[i].score > results[i + 1].score,
            "Length normalization violation: rank {} score {:.6} <= rank {} score {:.6}",
            i,
            results[i].score,
            i + 1,
            results[i + 1].score
        );
    }
}

#[test]
fn challenge_bm25_idf_rarity_and_positive_floor() {
    // Robertson-Spärck Jones smoothed IDF must reward rare terms and ALWAYS remain strictly positive (> 0.0),
    // even when a term appears in 100% of corpus documents (df = N).
    let n_docs = 20usize;
    let mut chunks = Vec::new();

    for i in 0..n_docs {
        let mut tf = HashMap::new();
        // "rare" appears in 1 doc
        if i == 0 {
            tf.insert("rare_term".to_string(), 1);
        }
        // "medium" appears in 5 docs
        if i < 5 {
            tf.insert("medium_term".to_string(), 1);
        }
        // "ubiquitous" appears in all 20 docs (df = N)
        tf.insert("ubiquitous_term".to_string(), 1);

        chunks.push(KnowledgeChunk {
            document: format!("doc_{}.md", i),
            section: format!("Sec_{}", i),
            snippet: "content".to_string(),
            term_frequencies: tf,
            doc_length: 50,
        });
    }

    let mut df = HashMap::new();
    df.insert("rare_term".to_string(), 1);
    df.insert("medium_term".to_string(), 5);
    df.insert("ubiquitous_term".to_string(), n_docs);

    let index = Bm25Index {
        chunks,
        doc_frequencies: df,
        avg_doc_length: 50.0,
    };

    let r_rare = index.search("rare_term", 1);
    let r_medium = index.search("medium_term", 1);
    let r_ubiquitous = index.search("ubiquitous_term", 1);

    assert_eq!(r_rare.len(), 1);
    assert_eq!(r_medium.len(), 1);
    assert_eq!(r_ubiquitous.len(), 1);

    // Strict rarity ordering
    assert!(
        r_rare[0].score > r_medium[0].score,
        "Rarity failure: rare score {:.6} <= medium score {:.6}",
        r_rare[0].score,
        r_medium[0].score
    );
    assert!(
        r_medium[0].score > r_ubiquitous[0].score,
        "Rarity failure: medium score {:.6} <= ubiquitous score {:.6}",
        r_medium[0].score,
        r_ubiquitous[0].score
    );

    // Positive floor protection: even df = N MUST produce score > 0.0 (no negative IDF!)
    assert!(
        r_ubiquitous[0].score > 0.0,
        "Positive floor failure: ubiquitous score must be > 0, got {:.6}",
        r_ubiquitous[0].score
    );
}

#[test]
fn challenge_bm25_query_commutativity_and_deduplication() {
    // 1. Commutativity: "rust concurrency" vs "concurrency rust" must yield identical scores
    let root = find_workspace_root();
    let dir = resolve_knowledge_dir(&root);
    let index = Bm25Index::build_from_dir(&dir).expect("Must build index");

    let res_a = index.search("rust concurrency", 3);
    let res_b = index.search("concurrency rust", 3);

    assert_eq!(res_a.len(), res_b.len());
    for (a, b) in res_a.iter().zip(res_b.iter()) {
        assert_eq!(a.document, b.document);
        assert_eq!(a.section, b.section);
        assert!(
            (a.score - b.score).abs() < 1e-9,
            "Query commutativity mismatch: {:.9} vs {:.9}",
            a.score,
            b.score
        );
    }

    // 2. Deduplication: "rust" vs "rust rust rust" must yield identical scores
    let res_single = index.search("rust", 3);
    let res_repeated = index.search("rust rust rust", 3);

    assert_eq!(res_single.len(), res_repeated.len());
    for (s, r) in res_single.iter().zip(res_repeated.iter()) {
        assert_eq!(s.document, r.document);
        assert_eq!(s.section, r.section);
        assert!(
            (s.score - r.score).abs() < 1e-9,
            "Query deduplication mismatch: single={:.9} vs repeated={:.9}",
            s.score,
            r.score
        );
    }
}

// ============================================================================
// SUITE 2: ZERO DIVISION, BOUNDARY CONDITIONS & CRASH RESISTANCE
// ============================================================================

#[test]
fn challenge_bm25_zero_division_resilience() {
    // Case 1: Empty chunks corpus
    let empty_index = Bm25Index {
        chunks: Vec::new(),
        doc_frequencies: HashMap::new(),
        avg_doc_length: 0.0,
    };
    assert!(empty_index.search("test", 5).is_empty());
    assert!(empty_index.search("", 5).is_empty());

    // Case 2: Corpus with 0 avg_doc_length (documents with 0 tokens)
    let zero_len_chunk = KnowledgeChunk {
        document: "empty.md".to_string(),
        section: "Empty".to_string(),
        snippet: "...".to_string(),
        term_frequencies: HashMap::new(),
        doc_length: 0,
    };
    let zero_avg_index = Bm25Index {
        chunks: vec![zero_len_chunk],
        doc_frequencies: HashMap::new(),
        avg_doc_length: 0.0,
    };
    // Must not panic or divide by zero
    let res = zero_avg_index.search("anything", 3);
    assert!(res.is_empty());

    // Case 3: Chunk doc_length is 0 but avg_doc_length > 0
    let mut non_zero_chunks = Vec::new();
    let mut non_zero_tf = HashMap::new();
    non_zero_tf.insert("hello".to_string(), 1);
    non_zero_chunks.push(KnowledgeChunk {
        document: "doc1.md".to_string(),
        section: "Sec1".to_string(),
        snippet: "hello".to_string(),
        term_frequencies: non_zero_tf,
        doc_length: 5,
    });
    non_zero_chunks.push(KnowledgeChunk {
        document: "doc2.md".to_string(),
        section: "Sec2".to_string(),
        snippet: "".to_string(),
        term_frequencies: HashMap::new(),
        doc_length: 0,
    });

    let mut df = HashMap::new();
    df.insert("hello".to_string(), 1);

    let mixed_index = Bm25Index {
        chunks: non_zero_chunks,
        doc_frequencies: df,
        avg_doc_length: 2.5,
    };
    let r = mixed_index.search("hello", 5);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].document, "doc1.md");
    assert!(r[0].score > 0.0 && !r[0].score.is_nan() && !r[0].score.is_infinite());
}

#[test]
fn challenge_bm25_empty_and_degenerate_queries() {
    let root = find_workspace_root();
    let dir = resolve_knowledge_dir(&root);
    let index = Bm25Index::build_from_dir(&dir).expect("Must build index");

    // Empty query strings
    assert!(index.search("", 5).is_empty());
    assert!(index.search("   ", 5).is_empty());
    assert!(index.search("\t\n\r  \n", 5).is_empty());

    // Punctuation and symbols only
    assert!(index.search("!@#$%^&*()_+", 5).is_empty());
    assert!(index.search("... --- ... ???", 5).is_empty());
    assert!(index.search(";;; === {}", 5).is_empty());

    // Stop words only (English)
    assert!(index.search("the", 5).is_empty());
    assert!(index.search("and or but", 5).is_empty());
    assert!(index.search("in on at to for with", 5).is_empty());
    assert!(index.search("what where when who how", 5).is_empty());

    // Stop words only (Indonesian)
    assert!(index.search("dan", 5).is_empty());
    assert!(index.search("di ke dari", 5).is_empty());
    assert!(index.search("yang ini itu untuk pada adalah sebagai", 5).is_empty());

    // Single character queries
    assert!(index.search("a", 5).is_empty());
    assert!(index.search("x", 5).is_empty());
    assert!(index.search("1", 5).is_empty());

    // Non-existent vocabulary
    assert!(index.search("xyzzy999foobarqux000nonexistent", 5).is_empty());

    // Emojis & non-latin unicode symbols
    assert!(index.search("🚀🦀🔥", 5).is_empty());
    assert!(index.search("∑∏∆√", 5).is_empty());
}

#[test]
fn challenge_bm25_top_k_boundaries() {
    let root = find_workspace_root();
    let dir = resolve_knowledge_dir(&root);
    let index = Bm25Index::build_from_dir(&dir).expect("Must build index");

    // top_k = 0 returns 0 results
    let r_zero = index.search("agent", 0);
    assert!(r_zero.is_empty(), "top_k=0 must return empty vector");

    // top_k very large (e.g. usize::MAX) returns all matching results
    let r_all = index.search("agent", usize::MAX);
    assert!(!r_all.is_empty());

    // top_k = 1 returns exactly 1 result
    let r_1 = index.search("agent", 1);
    assert_eq!(r_1.len(), 1);
    assert_eq!(r_1[0].document, r_all[0].document);
    assert_eq!(r_1[0].section, r_all[0].section);

    // top_k = 2 returns at most 2 results
    let r_2 = index.search("agent", 2);
    assert!(r_2.len() <= 2);

    // top_k very large (e.g. usize::MAX)
    let r_max = index.search("agent", usize::MAX);
    assert_eq!(r_max.len(), r_all.len());
}

// ============================================================================
// SUITE 3: DOCUMENT RANKING CONSISTENCY ACROSS INDONESIAN & ENGLISH QUERIES
// ============================================================================

#[test]
fn challenge_bm25_ground_truth_ranking_accuracy() {
    let root = find_workspace_root();
    let dir = resolve_knowledge_dir(&root);
    let index = Bm25Index::build_from_dir(&dir).expect("Must build index");

    struct TestCase {
        query: &'static str,
        expected_doc: &'static str,
        expected_section_substring: &'static str,
        expected_snippet_keyword: &'static str,
        lang: &'static str,
    }

    let cases = vec![
        TestCase {
            query: "kebijakan kerahasiaan API Key dan credential cloud",
            expected_doc: "company_policy.md",
            expected_section_substring: "Keamanan",
            expected_snippet_keyword: "Kerahasiaan API Key",
            lang: "Indonesian",
        },
        TestCase {
            query: "standar kualitas kode rust zero warnings clippy",
            expected_doc: "company_policy.md",
            expected_section_substring: "Standar Kualitas Kode",
            expected_snippet_keyword: "Zero Warnings",
            lang: "Indonesian",
        },
        TestCase {
            query: "etika background subagents isolasi TaskLogBuffer REPL",
            expected_doc: "company_policy.md",
            expected_section_substring: "Etika Subagent",
            expected_snippet_keyword: "TaskLogBuffer",
            lang: "Indonesian",
        },
        TestCase {
            query: "keunggulan ctrl-cli dibandingkan framework python",
            expected_doc: "product_faqs.md",
            expected_section_substring: "Q1",
            expected_snippet_keyword: "Biner Sangat Ringan",
            lang: "Indonesian",
        },
        TestCase {
            query: "cara agen membaca dokumen data knowledge read_file grep_files",
            expected_doc: "product_faqs.md",
            expected_section_substring: "Q3",
            expected_snippet_keyword: "read_file",
            lang: "Indonesian",
        },
        TestCase {
            query: "lokasi penyimpanan persona baru SKILL.md prompts",
            expected_doc: "product_faqs.md",
            expected_section_substring: "Q4",
            expected_snippet_keyword: "SKILL.md",
            lang: "Indonesian",
        },
        TestCase {
            query: "lightweight memory footprint without garbage collection",
            expected_doc: "product_faqs.md",
            expected_section_substring: "Q1",
            expected_snippet_keyword: "overhead garbage collection",
            lang: "English",
        },
        TestCase {
            query: "safe concurrency Arc Mutex Condvar cooperative cancellation",
            expected_doc: "company_policy.md",
            expected_section_substring: "Standar Kualitas Kode",
            expected_snippet_keyword: "cooperative cancellation",
            lang: "English",
        },
        TestCase {
            query: "destructive file mutation permission gate confirmation",
            expected_doc: "company_policy.md",
            expected_section_substring: "Keamanan",
            expected_snippet_keyword: "Permission Gate",
            lang: "English",
        },
        TestCase {
            query: "background subagent logging output task isolation",
            expected_doc: "product_faqs.md",
            expected_section_substring: "Q2",
            expected_snippet_keyword: "TaskLogBuffer",
            lang: "English",
        },
    ];

    for case in cases {
        let results = index.search(case.query, 3);
        assert!(
            !results.is_empty(),
            "[{}] Query '{}' returned 0 results",
            case.lang,
            case.query
        );

        let top = &results[0];
        assert_eq!(
            top.document, case.expected_doc,
            "[{}] Query '{}' top document mismatch. Expected: {}, got: {}",
            case.lang, case.query, case.expected_doc, top.document
        );
        assert!(
            top.section.contains(case.expected_section_substring),
            "[{}] Query '{}' section mismatch. Expected substring '{}', got '{}'",
            case.lang,
            case.query,
            case.expected_section_substring,
            top.section
        );
        assert!(
            top.snippet.contains(case.expected_snippet_keyword),
            "[{}] Query '{}' snippet did not contain expected keyword '{}'",
            case.lang,
            case.query,
            case.expected_snippet_keyword
        );
        assert!(
            top.score > 0.0,
            "[{}] Query '{}' top score must be positive, got {:.6}",
            case.lang,
            case.query,
            top.score
        );
    }
}

// ============================================================================
// SUITE 4: EXTREME SCALE, MEMORY & CONCURRENCY STRESS
// ============================================================================

#[test]
fn challenge_bm25_extreme_scale_and_concurrent_queries() {
    // 1. Extreme scale document: 10,000 words
    let mut large_tf = HashMap::new();
    large_tf.insert("scalability".to_string(), 500);
    large_tf.insert("concurrency".to_string(), 300);
    large_tf.insert("garbage".to_string(), 9200);

    let large_chunk = KnowledgeChunk {
        document: "large.md".to_string(),
        section: "Massive Section".to_string(),
        snippet: "Scalability test".to_string(),
        term_frequencies: large_tf,
        doc_length: 10000,
    };

    let mut df = HashMap::new();
    df.insert("scalability".to_string(), 1);
    df.insert("concurrency".to_string(), 1);
    df.insert("garbage".to_string(), 1);

    let large_index = Arc::new(Bm25Index {
        chunks: vec![large_chunk],
        doc_frequencies: df,
        avg_doc_length: 10000.0,
    });

    let res = large_index.search("scalability concurrency", 5);
    assert_eq!(res.len(), 1);
    assert!(!res[0].score.is_nan());
    assert!(!res[0].score.is_infinite());
    assert!(res[0].score > 0.0);

    // 2. High-concurrency stress test: 10 threads running 100 queries each
    let mut handles = Vec::new();
    for thread_idx in 0..10 {
        let index_clone = Arc::clone(&large_index);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                let q = if i % 2 == 0 { "scalability" } else { "concurrency" };
                let out = index_clone.search(q, 1);
                assert_eq!(out.len(), 1);
                assert!(out[0].score > 0.0);
            }
            thread_idx
        }));
    }

    for h in handles {
        let idx = h.join().expect("Thread should not panic");
        assert!(idx < 10);
    }
}

// ============================================================================
// SUITE 5: PERFORMANCE & SUB-MILLISECOND LATENCY BENCHMARK
// ============================================================================

#[test]
fn challenge_bm25_sub_millisecond_latency_benchmark() {
    let root = find_workspace_root();
    let dir = resolve_knowledge_dir(&root);

    // Benchmark 1: Index build time from disk
    let build_start = Instant::now();
    let index = Bm25Index::build_from_dir(&dir).expect("Must build index");
    let build_elapsed = build_start.elapsed();
    assert!(
        build_elapsed.as_millis() < 50,
        "Index build took {:?}, must be under 50ms",
        build_elapsed
    );

    // Benchmark 2: 1,000 queries sequential latency
    let test_queries = [
        "keamanan sistem dan API Key",
        "standar kualitas kode rust zero warnings",
        "keunggulan ctrl-cli dibandingkan framework python",
        "subagent background tasklogbuffer",
        "cara membaca data knowledge read_file",
        "persona SKILL prompts",
        "permission gate confirmation",
        "concurrency deadlock Arc Mutex",
    ];

    let iterations = 1000;
    let query_start = Instant::now();
    for i in 0..iterations {
        let q = test_queries[i % test_queries.len()];
        let res = index.search(q, 3);
        assert!(!res.is_empty());
    }
    let total_elapsed = query_start.elapsed();
    let avg_per_query = total_elapsed / iterations as u32;

    println!(
        "\nBM25 Benchmark: 1,000 queries executed in {:?}. Average per query: {:?}",
        total_elapsed, avg_per_query
    );

    assert!(
        avg_per_query.as_micros() < 1000,
        "Average query latency {:?} exceeded 1.0ms SLA target",
        avg_per_query
    );
}

// ============================================================================
// SUITE 6: TOOL REGISTRATION, FORMATTING & INTEGRATION
// ============================================================================

#[test]
fn challenge_bm25_format_search_results_structure() {
    // 1. Empty results formatting
    let empty_fmt = format_search_results(&[]);
    assert_eq!(
        empty_fmt,
        "No relevant documents found in knowledge base."
    );

    // 2. Populated results formatting
    let sample = vec![
        SearchResult {
            document: "company_policy.md".to_string(),
            section: "1. Keamanan & Akses Sistem".to_string(),
            snippet: "Kerahasiaan API Key harus dijaga.".to_string(),
            score: 1.8543,
        },
        SearchResult {
            document: "product_faqs.md".to_string(),
            section: "Q1: Keunggulan".to_string(),
            snippet: "Biner sangat ringan ~1.8 MB.".to_string(),
            score: 0.9412,
        },
    ];

    let formatted = format_search_results(&sample);
    assert!(formatted.contains("Found 2 relevant excerpt(s) in knowledge base:"));
    assert!(formatted.contains("--- [1] Document: company_policy.md | Section: \"1. Keamanan & Akses Sistem\" | Score: 1.85 ---"));
    assert!(formatted.contains("Kerahasiaan API Key harus dijaga."));
    assert!(formatted.contains("--- [2] Document: product_faqs.md | Section: \"Q1: Keunggulan\" | Score: 0.94 ---"));
    assert!(formatted.contains("Biner sangat ringan ~1.8 MB."));
}

#[test]
fn challenge_bm25_search_knowledge_and_run_knowledge_search() {
    let root = find_workspace_root();

    // Direct search_knowledge API
    let results = search_knowledge(&root, "keamanan sistem", 2).expect("Search should succeed");
    assert!(!results.is_empty());
    assert!(results.len() <= 2);
    assert_eq!(results[0].document, "company_policy.md");

    // Top-level run_knowledge_search
    let formatted = run_knowledge_search("keamanan sistem", 2).expect("Run tool should succeed");
    assert!(formatted.contains("Found"));
    assert!(formatted.contains("company_policy.md"));
    assert!(formatted.contains("Score:"));
}

#[test]
fn challenge_bm25_tokenization_and_chunking() {
    // 1. Tokenization filters STOP_WORDS and terms with length < 2
    let sample = "The quick brown fox jumps over a lazy dog dan juga kucing";
    let tokens = tokenize(sample);
    assert!(!tokens.contains(&"the".to_string()));
    assert!(!tokens.contains(&"a".to_string()));
    assert!(!tokens.contains(&"dan".to_string()));
    assert!(!tokens.contains(&"juga".to_string()));
    assert!(tokens.contains(&"quick".to_string()));
    assert!(tokens.contains(&"brown".to_string()));
    assert!(tokens.contains(&"kucing".to_string()));
    assert!(!STOP_WORDS.is_empty());

    // 2. tokenize_query fallback when all tokens are stop words
    let query_fallback = tokenize_query("the dan ini");
    assert_eq!(query_fallback, vec!["the", "dan", "ini"]);

    // 3. chunk_markdown correctly parses headers up to ####
    let doc = "# Title\nBody 0\n## Sub 1\nBody 1\n### Sub 2\nBody 2\n#### Sub 3\nBody 3\n##### Sub 4 Ignored\nBody 4";
    let chunks = chunk_markdown("test.md", doc);
    assert_eq!(chunks.len(), 4);
    assert_eq!(chunks[0].section, "Title");
    assert_eq!(chunks[1].section, "Sub 1");
    assert_eq!(chunks[2].section, "Sub 2");
    assert_eq!(chunks[3].section, "Sub 3");
    assert!(chunks[3].snippet.contains("##### Sub 4 Ignored"));
}

