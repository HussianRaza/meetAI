/// Port of OpenOats/KnowledgeBase.swift:547 — markdown-aware chunker.
/// Splits text into 80-500 word chunks preserving header breadcrumbs.

const TARGET_MIN: usize = 80;
const TARGET_MAX: usize = 500;
const OVERLAP: usize = TARGET_MAX / 5; // 20% = 100 words

#[derive(Debug, Clone)]
pub struct Chunk {
    pub text: String,
    pub breadcrumb: String,
    pub chunk_index: usize,
}

pub fn chunk_markdown(text: &str) -> Vec<Chunk> {
    let lines: Vec<&str> = text.lines().collect();

    struct Section {
        headers: Vec<String>,
        lines: Vec<String>,
    }

    let mut sections: Vec<Section> = Vec::new();
    let mut current = Section {
        headers: vec![],
        lines: vec![],
    };

    for line in &lines {
        if line.starts_with('#') {
            if !current.lines.is_empty() {
                sections.push(Section {
                    headers: current.headers.clone(),
                    lines: current.lines.clone(),
                });
            }
            let trimmed = line.trim_start_matches('#');
            let level = line.len() - trimmed.len();
            let header_text = trimmed.trim().to_string();

            let mut new_headers = current.headers.clone();
            if level <= new_headers.len() {
                new_headers.truncate(level - 1);
            }
            new_headers.push(header_text);

            current = Section {
                headers: new_headers,
                lines: vec![],
            };
        } else {
            current.lines.push(line.to_string());
        }
    }
    if !current.lines.is_empty() {
        sections.push(Section {
            headers: current.headers,
            lines: current.lines,
        });
    }

    let mut raw: Vec<(String, String)> = Vec::new(); // (text, breadcrumb)
    let mut pending_text = String::new();
    let mut pending_header = String::new();

    for section in sections {
        let section_text = section.lines.join("\n").trim().to_string();
        if section_text.is_empty() {
            continue;
        }
        let breadcrumb = section.headers.join(" > ");
        let word_count = section_text.split_whitespace().count();

        if word_count < TARGET_MIN {
            if pending_text.is_empty() {
                pending_text = section_text;
                pending_header = breadcrumb;
            } else {
                pending_text.push_str("\n\n");
                pending_text.push_str(&section_text);
                if !breadcrumb.is_empty() {
                    pending_header = breadcrumb;
                }
            }
            if pending_text.split_whitespace().count() >= TARGET_MIN {
                raw.push((pending_text.clone(), pending_header.clone()));
                pending_text.clear();
                pending_header.clear();
            }
        } else if word_count > TARGET_MAX {
            if !pending_text.is_empty() {
                raw.push((pending_text.clone(), pending_header.clone()));
                pending_text.clear();
                pending_header.clear();
            }
            split_large(&section_text, &breadcrumb, &mut raw);
        } else {
            if !pending_text.is_empty() {
                raw.push((pending_text.clone(), pending_header.clone()));
                pending_text.clear();
                pending_header.clear();
            }
            raw.push((section_text, breadcrumb));
        }
    }
    if !pending_text.is_empty() {
        raw.push((pending_text, pending_header));
    }

    // Fallback: no headers in document
    if raw.is_empty() && !text.trim().is_empty() {
        let word_count = text.split_whitespace().count();
        if word_count <= TARGET_MAX {
            raw.push((text.trim().to_string(), String::new()));
        } else {
            split_large(text.trim(), "", &mut raw);
        }
    }

    raw.into_iter()
        .enumerate()
        .map(|(i, (text, breadcrumb))| Chunk {
            text,
            breadcrumb,
            chunk_index: i,
        })
        .collect()
}

fn split_large(text: &str, breadcrumb: &str, out: &mut Vec<(String, String)>) {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut start = 0;
    while start < words.len() {
        let end = (start + TARGET_MAX).min(words.len());
        out.push((words[start..end].join(" "), breadcrumb.to_string()));
        if end == words.len() {
            break;
        }
        start += TARGET_MAX - OVERLAP;
    }
}
