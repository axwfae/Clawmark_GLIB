//! OpenClaw memory adapter — reads markdown memory files, migrates to clawmark signals
//!
//! Also supports PicoClaw format:
//!   - workspace/memory/*.md (all .md files, no date restriction)
//!
//! Each file becomes one or more signals with preserved timestamps.

use std::path::{Path, PathBuf};
use regex::Regex;

fn is_valid_date(s: &str) -> bool {
    Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap().is_match(s)
}

/// Discovered OpenClaw workspace
pub struct ClawWorkspace {
    pub path: PathBuf,
    pub memory_md: Option<PathBuf>,
    pub daily_files: Vec<DailyFile>,
    pub is_picoclaw: bool,
}

pub struct DailyFile {
    pub path: PathBuf,
    pub date: String, // YYYY-MM-DD
}

/// Detect an OpenClaw workspace at the given path
/// OpenClaw: path = workspace/ → scan MEMORY.md + memory/YYYY-MM-DD.md
/// PicoClaw: path = workspace/ → scan memory/*.md (all .md files)
pub fn detect_workspace(path: &Path, is_picoclaw: bool) -> Option<ClawWorkspace> {
    // OpenClaw: check for MEMORY.md
    let has_memory_md = if is_picoclaw {
        false // PicoClaw doesn't need MEMORY.md
    } else {
        path.join("MEMORY.md").exists()
    };

    // Scan memory/ subdirectory
    let memory_dir = path.join("memory");
    let has_memory_dir = memory_dir.is_dir();

    // OpenClaw: need memory/ dir OR MEMORY.md
    // PicoClaw: only need memory/ dir
    if !has_memory_dir && (!is_picoclaw && !has_memory_md) {
        return None;
    }

    let memory_md = if has_memory_md {
        Some(path.join("MEMORY.md"))
    } else {
        None
    };

    let mut daily_files = Vec::new();
    if has_memory_dir {
        if let Ok(entries) = std::fs::read_dir(&memory_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().map(|e| e == "md").unwrap_or(false) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    // OpenClaw: only YYYY-MM-DD.md files
                    // PicoClaw: any .md file
                    let date_re = Regex::new(r"^(\d{4}-\d{2}-\d{2})\.md$").unwrap();
                    if let Some(caps) = date_re.captures(&name) {
                        daily_files.push(DailyFile {
                            path: p,
                            date: caps[1].to_string(),
                        });
                    } else if is_picoclaw {
                        // PicoClaw: use filename as date
                        daily_files.push(DailyFile {
                            path: p,
                            date: name.trim_end_matches(".md").to_string(),
                        });
                    }
                }
            }
        }
        daily_files.sort_by(|a, b| a.date.cmp(&b.date));
    }

    Some(ClawWorkspace {
        path: path.to_path_buf(),
        memory_md,
        daily_files,
    })
}

/// Migrate an OpenClaw workspace's memory into a relay station database.
/// Returns (signals_created, errors).
pub fn migrate(
    workspace: &ClawWorkspace,
    db: &crate::db::DatabaseManager,
) -> Result<(usize, usize), String> {
    let mut created = 0;
    let mut errors = 0;

    // 1. Migrate MEMORY.md as a single signal (long-term curated memory)
    if let Some(ref memory_path) = workspace.memory_md {
        match std::fs::read_to_string(memory_path) {
            Ok(content) => {
                let content = content.trim();
                if !content.is_empty() {
                    let filename = memory_path.file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| "MEMORY.md".to_string());
                    let gist = format!("openclaw-migration: {} (curated long-term memory)", filename);
                    match db.signal(content, Some(&gist), None, None) {
                        Ok(_) => { created += 1; }
                        Err(e) => {
                            eprintln!("[migrate] Failed to migrate {}: {}", filename, e);
                            errors += 1;
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("[migrate] Failed to read {}: {}", memory_path.display(), e);
                errors += 1;
            }
        }
    }

    // 2. Migrate daily files — each becomes a signal with the date as timestamp
    for daily in &workspace.daily_files {
        match std::fs::read_to_string(&daily.path) {
            Ok(content) => {
                let content = content.trim();
                if content.is_empty() { continue; }

                // Split into sections if the file has markdown headers
                let sections = split_sections(content);

                if sections.len() <= 1 {
                    // Single signal for the whole file
                    let gist = if workspace.is_picoclaw {
                        format!("picoclaw: {}", daily.date)
                    } else {
                        format!("openclaw-daily: {}", daily.date)
                    };
                    let ts = if workspace.is_picoclaw || !is_valid_date(&daily.date) {
                        None
                    } else {
                        Some(format!("{}T23:59:59", daily.date))
                    };
                    match db.signal(content, Some(&gist), None, ts.as_deref()) {
                        Ok(_) => { created += 1; }
                        Err(e) => {
                            eprintln!("[migrate] Failed to migrate {}: {}", daily.date, e);
                            errors += 1;
                        }
                    }
                } else {
                    // Multiple sections — each becomes a threaded signal
                    let mut root_uuid: Option<String> = None;
                    for (i, section) in sections.iter().enumerate() {
                        let gist = match &section.header {
                            Some(h) if workspace.is_picoclaw => format!("picoclaw: {} — {}", daily.date, h),
                            Some(h) => format!("openclaw-daily: {} — {}", daily.date, h),
                            None if workspace.is_picoclaw => format!("picoclaw: {} (section {})", daily.date, i + 1),
                            None => format!("openclaw-daily: {} (section {})", daily.date, i + 1),
                        };
                        let ts = if workspace.is_picoclaw || !is_valid_date(&daily.date) {
                            None
                        } else {
                            Some(format!("{}T{:02}:00:00", daily.date, (i * 2).min(23)))
                        };
                        let parent = root_uuid.as_deref();
                        match db.signal(&section.content, Some(&gist), parent, ts.as_deref()) {
                            Ok(short_uuid) => {
                                if root_uuid.is_none() {
                                    // First section becomes the thread root — resolve full UUID
                                    root_uuid = Some(short_uuid);
                                }
                                created += 1;
                            }
                            Err(e) => {
                                eprintln!("[migrate] Failed to migrate {} section {}: {}", daily.date, i + 1, e);
                                errors += 1;
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("[migrate] Failed to read {}: {}", daily.path.display(), e);
                errors += 1;
            }
        }
    }

    Ok((created, errors))
}

pub struct Section {
    pub header: Option<String>,
    pub content: String,
}

/// Split markdown content by ## headers into sections
pub fn split_sections(content: &str) -> Vec<Section> {
    let mut sections = Vec::new();
    let mut current = String::new();
    let mut current_header: Option<String> = None;

    for line in content.lines() {
        if line.starts_with("## ") && !current.trim().is_empty() {
            sections.push(Section {
                header: current_header.take(),
                content: current.trim().to_string(),
            });
            current = String::new();
        }
        if line.starts_with("## ") {
            current_header = Some(line.trim_start_matches('#').trim().to_string());
        }
        current.push_str(line);
        current.push('\n');
    }

    if !current.trim().is_empty() {
        sections.push(Section {
            header: current_header.take(),
            content: current.trim().to_string(),
        });
    }

    sections
}

/// Summary of detected workspace for display
pub fn workspace_summary(ws: &ClawWorkspace) -> String {
    let mut lines = vec![
        format!("OpenClaw workspace: {}", ws.path.display()),
    ];

    if let Some(ref m) = ws.memory_md {
        let size = std::fs::metadata(m).map(|m| m.len()).unwrap_or(0);
        lines.push(format!("  MEMORY.md: {} bytes", size));
    } else {
        lines.push("  MEMORY.md: not found".to_string());
    }

    lines.push(format!("  Daily logs: {} files", ws.daily_files.len()));
    if let Some(first) = ws.daily_files.first() {
        if let Some(last) = ws.daily_files.last() {
            lines.push(format!("  Date range: {} to {}", first.date, last.date));
        }
    }

lines.join("\n")
}
