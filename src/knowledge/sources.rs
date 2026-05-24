//! Public knowledge base with static markdown documents.

use serde::{Deserialize, Serialize};

/// A knowledge document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDocument {
    /// Document ID.
    pub id: String,
    /// Document title.
    pub title: String,
    /// Document content (markdown).
    pub content: String,
    /// Document category.
    pub category: String,
}

/// Public knowledge base loaded from static markdown.
pub struct PublicKnowledgeBase {
    documents: Vec<KnowledgeDocument>,
}

impl PublicKnowledgeBase {
    /// Create a new knowledge base with built-in documents.
    pub fn new() -> Self {
        Self {
            documents: vec![
                KnowledgeDocument {
                    id: "kb-001".into(),
                    title: "Account Security".into(),
                    content: include_str!("../../knowledge/account_security.md").into(),
                    category: "security".into(),
                },
                KnowledgeDocument {
                    id: "kb-002".into(),
                    title: "Transfer Limits".into(),
                    content: include_str!("../../knowledge/transfer_limits.md").into(),
                    category: "transfers".into(),
                },
                KnowledgeDocument {
                    id: "kb-003".into(),
                    title: "Dispute Process".into(),
                    content: include_str!("../../knowledge/dispute_process.md").into(),
                    category: "disputes".into(),
                },
                KnowledgeDocument {
                    id: "kb-004".into(),
                    title: "Fee Schedule".into(),
                    content: include_str!("../../knowledge/fee_schedule.md").into(),
                    category: "fees".into(),
                },
                KnowledgeDocument {
                    id: "kb-005".into(),
                    title: "FAQ".into(),
                    content: include_str!("../../knowledge/faq.md").into(),
                    category: "general".into(),
                },
            ],
        }
    }

    /// Get all documents.
    pub fn documents(&self) -> &[KnowledgeDocument] {
        &self.documents
    }

    /// Search documents by keyword.
    pub fn search(&self, query: &str) -> Vec<&KnowledgeDocument> {
        let query_lower = query.to_lowercase();
        self.documents
            .iter()
            .filter(|doc| {
                doc.title.to_lowercase().contains(&query_lower)
                    || doc.content.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    /// Get a document by ID.
    pub fn get(&self, id: &str) -> Option<&KnowledgeDocument> {
        self.documents.iter().find(|d| d.id == id)
    }
}

impl Default for PublicKnowledgeBase {
    fn default() -> Self {
        Self::new()
    }
}

mod tests {
    use super::*;

    #[test]
    fn test_knowledge_base_loads() {
        let kb = PublicKnowledgeBase::new();
        assert_eq!(kb.documents().len(), 5);
    }

    #[test]
    fn test_search_documents() {
        let kb = PublicKnowledgeBase::new();
        let results = kb.search("security");
        assert!(!results.is_empty());
    }

    #[test]
    fn test_get_document_by_id() {
        let kb = PublicKnowledgeBase::new();
        let doc = kb.get("kb-001");
        assert!(doc.is_some());
        assert_eq!(doc.unwrap().title, "Account Security");
    }

    #[test]
    fn test_get_nonexistent_document() {
        let kb = PublicKnowledgeBase::new();
        assert!(kb.get("kb-999").is_none());
    }

    #[test]
    fn test_default() {
        let kb = PublicKnowledgeBase::default();
        assert_eq!(kb.documents().len(), 5);
    }
}
