use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VocabEntry {
    pub id: u64,
    pub phrase: String,
    pub replacement: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Vocabulary {
    pub entries: Vec<VocabEntry>,
}

fn vocabulary_path() -> PathBuf {
    let data_dir = dirs::data_dir().expect("Failed to get data directory");
    data_dir
        .join("com.dictate.app")
        .join("vocabulary.json")
}

pub fn load_vocabulary() -> Vocabulary {
    let path = vocabulary_path();
    if !path.exists() {
        return Vocabulary::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => Vocabulary::default(),
    }
}

pub fn save_vocabulary(vocabulary: &Vocabulary) -> Result<()> {
    let path = vocabulary_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let contents = serde_json::to_string_pretty(vocabulary)?;
    std::fs::write(&path, contents)?;
    Ok(())
}

pub fn add_entry(entry: VocabEntry) -> Result<()> {
    let mut vocabulary = load_vocabulary();
    vocabulary.entries.push(entry);
    save_vocabulary(&vocabulary)
}

pub fn update_entry(id: u64, phrase: String, replacement: String, enabled: bool) -> Result<()> {
    let mut vocabulary = load_vocabulary();
    if let Some(entry) = vocabulary.entries.iter_mut().find(|e| e.id == id) {
        entry.phrase = phrase;
        entry.replacement = replacement;
        entry.enabled = enabled;
    }
    save_vocabulary(&vocabulary)
}

pub fn delete_entry(id: u64) -> Result<()> {
    let mut vocabulary = load_vocabulary();
    vocabulary.entries.retain(|e| e.id != id);
    save_vocabulary(&vocabulary)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectionApplied {
    pub original: String,
    pub replacement: String,
    pub position: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectionResult {
    pub text: String,
    pub corrections: Vec<CorrectionApplied>,
}

fn apply_case(matched: &str, replacement: &str) -> String {
    if matched.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()) && matched.chars().any(|c| c.is_alphabetic()) {
        replacement.to_uppercase()
    } else if matched.chars().next().map_or(false, |c| c.is_uppercase()) {
        let mut chars = replacement.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().to_string() + chars.as_str(),
            None => String::new(),
        }
    } else {
        replacement.to_string()
    }
}

pub fn apply_corrections(text: &str, vocabulary: &Vocabulary) -> CorrectionResult {
    let mut result = text.to_string();
    let mut corrections = Vec::new();

    for entry in &vocabulary.entries {
        if !entry.enabled {
            continue;
        }

        let pattern = format!(r"(?i)\b{}\b", regex::escape(&entry.phrase));
        let re = match Regex::new(&pattern) {
            Ok(re) => re,
            Err(_) => continue,
        };

        // Collect matches first, then apply replacements from end to start
        // to preserve positions
        let mut matches: Vec<(usize, usize, String)> = Vec::new();
        for m in re.find_iter(&result) {
            let matched_text = m.as_str();
            let replacement = apply_case(matched_text, &entry.replacement);
            matches.push((m.start(), m.end(), replacement));
        }

        // Apply from end to start so positions remain valid
        for (start, end, replacement) in matches.iter().rev() {
            corrections.push(CorrectionApplied {
                original: result[*start..*end].to_string(),
                replacement: replacement.clone(),
                position: *start,
            });
            result.replace_range(*start..*end, replacement);
        }
    }

    // Sort corrections by position
    corrections.sort_by_key(|c| c.position);

    CorrectionResult {
        text: result,
        corrections,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(phrase: &str, replacement: &str) -> VocabEntry {
        VocabEntry {
            id: 1,
            phrase: phrase.to_string(),
            replacement: replacement.to_string(),
            enabled: true,
        }
    }

    fn make_vocab(entries: Vec<VocabEntry>) -> Vocabulary {
        Vocabulary { entries }
    }

    #[test]
    fn test_basic_single_replacement() {
        let vocab = make_vocab(vec![make_entry("teh", "the")]);
        let result = apply_corrections("I went to teh store", &vocab);
        assert_eq!(result.text, "I went to the store");
        assert_eq!(result.corrections.len(), 1);
        assert_eq!(result.corrections[0].original, "teh");
        assert_eq!(result.corrections[0].replacement, "the");
    }

    #[test]
    fn test_word_boundary_safety() {
        let vocab = make_vocab(vec![make_entry("app", "application")]);
        let result = apply_corrections("The application is a great app", &vocab);
        assert_eq!(result.text, "The application is a great application");
        assert_eq!(result.corrections.len(), 1);
        assert_eq!(result.corrections[0].original, "app");
    }

    #[test]
    fn test_case_preservation_lowercase() {
        let vocab = make_vocab(vec![make_entry("teh", "the")]);
        let result = apply_corrections("teh quick brown fox", &vocab);
        assert_eq!(result.text, "the quick brown fox");
    }

    #[test]
    fn test_case_preservation_title_case() {
        let vocab = make_vocab(vec![make_entry("teh", "the")]);
        let result = apply_corrections("Teh quick brown fox", &vocab);
        assert_eq!(result.text, "The quick brown fox");
    }

    #[test]
    fn test_case_preservation_all_caps() {
        let vocab = make_vocab(vec![make_entry("teh", "the")]);
        let result = apply_corrections("TEH QUICK BROWN FOX", &vocab);
        assert_eq!(result.text, "THE QUICK BROWN FOX");
    }

    #[test]
    fn test_multiple_replacements() {
        let vocab = make_vocab(vec![
            make_entry("teh", "the"),
            VocabEntry {
                id: 2,
                phrase: "recieve".to_string(),
                replacement: "receive".to_string(),
                enabled: true,
            },
        ]);
        let result = apply_corrections("I recieve teh package", &vocab);
        assert_eq!(result.text, "I receive the package");
        assert_eq!(result.corrections.len(), 2);
    }

    #[test]
    fn test_disabled_entries_skipped() {
        let vocab = make_vocab(vec![VocabEntry {
            id: 1,
            phrase: "teh".to_string(),
            replacement: "the".to_string(),
            enabled: false,
        }]);
        let result = apply_corrections("I went to teh store", &vocab);
        assert_eq!(result.text, "I went to teh store");
        assert_eq!(result.corrections.len(), 0);
    }

    #[test]
    fn test_empty_vocabulary() {
        let vocab = make_vocab(vec![]);
        let result = apply_corrections("Hello world", &vocab);
        assert_eq!(result.text, "Hello world");
        assert_eq!(result.corrections.len(), 0);
    }
}
