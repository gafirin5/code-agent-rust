use std::borrow::Cow;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skill {
    pub id: Cow<'static, str>,
    pub name: Cow<'static, str>,
    pub description: Cow<'static, str>,
    pub system_prompt: Cow<'static, str>,
}

impl Skill {
    pub const fn new(
        id: &'static str,
        name: &'static str,
        description: &'static str,
        system_prompt: &'static str,
    ) -> Self {
        Self {
            id: Cow::Borrowed(id),
            name: Cow::Borrowed(name),
            description: Cow::Borrowed(description),
            system_prompt: Cow::Borrowed(system_prompt),
        }
    }
}

impl From<crate::tools::skills::SkillMetadata> for Skill {
    fn from(meta: crate::tools::skills::SkillMetadata) -> Self {
        Self {
            id: Cow::Owned(meta.name.clone()),
            name: Cow::Owned(meta.name),
            description: Cow::Owned(meta.description),
            system_prompt: Cow::Owned(meta.prompt_template),
        }
    }
}

pub fn get_available_skills() -> Vec<Skill> {
    vec![
        Skill::new(
            "rust-expert",
            "🦀 Rust Expert",
            "Spesialis Rust idiomatik, borrow checker, & performa tinggi",
            "You are an elite Rust systems programming expert. Write idiomatic, memory-safe, and high-performance Rust code. Master the borrow checker, use zero-cost abstractions, prefer Result/Option error handling, and explain safety invariants clearly.",
        ),
        Skill::new(
            "code-reviewer",
            "🔍 Code Reviewer",
            "Audit kode menyeluruh untuk bug, celah keamanan, & code smells",
            "You are a senior principal engineer and code reviewer. Rigorously review code for logical bugs, potential memory leaks, security vulnerabilities, edge cases, and maintainability issues. Provide constructive feedback with corrected, refactored code.",
        ),
        Skill::new(
            "web-frontend",
            "🎨 Web Frontend UI/UX",
            "Desain antarmuka modern, HTML/CSS/Tailwind responsif & estetik",
            "You are a modern frontend architect and UI/UX designer. Create visually aesthetic, accessible, modern, and responsive user interfaces using modern HTML, CSS, Tailwind, or component frameworks with clean layouts.",
        ),
        Skill::new(
            "api-architect",
            "🏗️ API & Backend Architect",
            "Desain REST/GraphQL API, skema database, dan autentikasi",
            "You are an enterprise backend and API architect. Design clean, scalable, RESTful or GraphQL APIs with robust validation, secure authentication (JWT/OAuth), clear error response schemas, and efficient database modeling.",
        ),
        Skill::new(
            "security-auditor",
            "🛡️ Security Auditor",
            "Audit keamanan aplikasi (OWASP Top 10, sanitasi input, mitigasi exploit)",
            "You are an application security specialist. Identify security vulnerabilities adhering to OWASP Top 10 guidelines (e.g. injection, broken auth, XSS, SSRF). Explain attack vectors and provide hardened, secure code patches.",
        ),
        Skill::new(
            "debugger",
            "🐞 Debugger & Trace Doctor",
            "Analisis error, stack trace, dan pemecahan masalah sistematis",
            "You are a master software debugger. Analyze stack traces, runtime errors, and unexpected behavior systematically. Perform root-cause analysis, explain why the bug happens, and provide the exact minimal fix.",
        ),
        Skill::new(
            "test-engineer",
            "🧪 Test Engineer & TDD",
            "Pembuatan unit test, integration test, mock, dan edge cases",
            "You are a test-driven development (TDD) engineer. Write comprehensive unit and integration test suites covering edge cases, boundary conditions, error paths, and mocks to ensure 100% reliability.",
        ),
        Skill::new(
            "refactor",
            "🧹 Clean Code & Refactoring",
            "Pembersihan kode, prinsip SOLID/DRY, dan arsitektur modular",
            "You are a software craftsperson focused on clean code, SOLID principles, and DRY architecture. Refactor tangled or bloated code into elegant, modular, and readable components without breaking functionality.",
        ),
    ]
}
