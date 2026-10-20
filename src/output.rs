use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    pub total: usize,
    pub returned: usize,
    pub truncated: bool,
    pub next_cursor: Option<String>,
}

pub fn paginate<T>(items: Vec<T>, limit: usize, cursor: Option<&str>) -> Result<(Vec<T>, Page)> {
    let offset = match cursor {
        None => 0,
        Some(cursor) => cursor
            .strip_prefix("o:")
            .and_then(|n| n.parse::<usize>().ok())
            .ok_or_else(|| {
                anyhow!(
                    "invalid cursor `{cursor}`; pass the next_cursor value from a previous page"
                )
            })?,
    };
    let total = items.len();
    let page_items: Vec<T> = items.into_iter().skip(offset).take(limit).collect();
    let end = offset + page_items.len();
    let truncated = end < total;
    Ok((
        page_items,
        Page {
            total,
            returned: end - offset.min(end),
            truncated,
            next_cursor: truncated.then(|| format!("o:{end}")),
        },
    ))
}

pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

pub trait AgentText {
    fn agent_text(&self) -> String;
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum MatchKind {
    Exact,
    Fuzzy { score: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
    Name,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paginate_reports_truncation_and_cursor() {
        let items: Vec<usize> = (0..600).collect();
        let (first, page) = paginate(items.clone(), 250, None).unwrap();
        assert_eq!(first.len(), 250);
        assert_eq!(
            page,
            Page {
                total: 600,
                returned: 250,
                truncated: true,
                next_cursor: Some("o:250".to_string())
            }
        );
        let (second, page2) = paginate(items.clone(), 250, page.next_cursor.as_deref()).unwrap();
        let (third, page3) = paginate(items, 250, page2.next_cursor.as_deref()).unwrap();
        assert_eq!(third.len(), 100);
        assert!(!page3.truncated);
        assert_eq!(page3.next_cursor, None);
        let all: Vec<usize> = first.into_iter().chain(second).chain(third).collect();
        assert_eq!(all, (0..600).collect::<Vec<_>>());
    }

    #[test]
    fn paginate_rejects_bad_cursor() {
        assert!(paginate(vec![1, 2, 3], 2, Some("page-2")).is_err());
    }

    #[test]
    fn estimate_tokens_counts_chars() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
        assert_eq!(estimate_tokens(&"é".repeat(8)), 2);
    }
}
