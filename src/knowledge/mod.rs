use serde_json::Value;
use std::collections::HashMap;
/// Public knowledge base containing banking product information.
pub struct PublicKnowledgeBase {
    documents: Vec<Document>,
}

impl PublicKnowledgeBase {
    /// Create a new knowledge base with embedded markdown documents.
    pub fn new() -> Self {
        let docs = vec![
            Self::make_doc("product_catalog", include_str!("product_catalog.md")),
            Self::make_doc("fee_schedule", include_str!("fee_schedule.md")),
            Self::make_doc("branch_hours", include_str!("branch_hours.md")),
            Self::make_doc("security_practices", include_str!("security_practices.md")),
            Self::make_doc(
                "account_opening_guide",
                include_str!("account_opening_guide.md"),
            ),
        ];
        Self { documents: docs }
    }

    /// Search the knowledge base for relevant information.
    pub fn search(&self, query: &str, top_k: usize) -> Vec<&Document> {
        // Simple keyword matching for now
        let query_lower = query.to_lowercase();
        let mut scored: Vec<(f64, &Document)> = self
            .documents
            .iter()
            .map(|doc| {
                let content_lower = doc.content.to_lowercase();
                let score = query_lower
                    .split_whitespace()
                    .filter(|word| content_lower.contains(word))
                    .count() as f64;
                (score, doc)
            })
            .filter(|(score, _)| *score > 0.0)
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().take(top_k).map(|(_, doc)| doc).collect()
    }

    fn make_doc(name: &str, content: &str) -> Document {
        Document {
            id: name.to_string(),
            content: content.to_string(),
            metadata: HashMap::new(),
        }
    }
}

impl Default for PublicKnowledgeBase {
    fn default() -> Self {
        Self::new()
    }
}

/// A document in the knowledge base.
#[derive(Debug, Clone)]
pub struct Document {
    pub id: String,
    pub content: String,
    pub metadata: HashMap<String, Value>,
}

mod tests {
    use crate::knowledge::PublicKnowledgeBase;

    #[test]
    fn test_knowledge_base_creation() {
        let kb = PublicKnowledgeBase::new();
        assert_eq!(kb.documents.len(), 5);
    }

    #[test]
    fn test_knowledge_base_default() {
        let kb = PublicKnowledgeBase::default();
        assert_eq!(kb.documents.len(), 5);
    }

    #[test]
    fn test_knowledge_base_search() {
        let kb = PublicKnowledgeBase::new();
        let results = kb.search("savings account", 3);
        assert!(!results.is_empty());
    }

    #[test]
    fn test_knowledge_base_search_no_match() {
        let kb = PublicKnowledgeBase::new();
        let results = kb.search("quantum physics", 3);
        assert!(results.is_empty());
    }

    #[test]
    fn test_knowledge_base_search_top_k() {
        let kb = PublicKnowledgeBase::new();
        let results = kb.search("account", 2);
        assert!(results.len() <= 2);
    }
}
