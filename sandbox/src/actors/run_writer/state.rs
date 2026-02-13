//! RunWriterActor state types.

use ractor::ActorRef;
use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::actors::event_store::EventStoreMsg;

use super::messages::SectionState;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DocumentSection {
    pub content: String,
    pub state: SectionState,
    pub proposal: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunDocument {
    pub objective: String,
    pub sections: HashMap<String, DocumentSection>,
}

impl Default for RunDocument {
    fn default() -> Self {
        let mut sections = HashMap::new();
        sections.insert(
            "conductor".to_string(),
            DocumentSection {
                content: String::new(),
                state: SectionState::Pending,
                proposal: None,
            },
        );
        sections.insert(
            "researcher".to_string(),
            DocumentSection {
                content: String::new(),
                state: SectionState::Pending,
                proposal: None,
            },
        );
        sections.insert(
            "terminal".to_string(),
            DocumentSection {
                content: String::new(),
                state: SectionState::Pending,
                proposal: None,
            },
        );
        sections.insert(
            "user".to_string(),
            DocumentSection {
                content: String::new(),
                state: SectionState::Pending,
                proposal: None,
            },
        );
        Self {
            objective: String::new(),
            sections,
        }
    }
}

impl RunDocument {
    pub fn new(objective: impl Into<String>) -> Self {
        Self {
            objective: objective.into(),
            ..Default::default()
        }
    }

    pub fn to_markdown(&self) -> String {
        let mut md = format!("# {}\n\n", self.objective);

        let section_order = ["conductor", "researcher", "terminal", "user"];
        for section_id in section_order {
            if let Some(section) = self.sections.get(section_id) {
                let title = section_id[..1].to_uppercase() + &section_id[1..];
                md.push_str(&format!("## {title}\n"));

                if let Some(ref proposal) = section.proposal {
                    md.push_str("<!-- proposal -->\n");
                    md.push_str(proposal);
                    if !proposal.ends_with('\n') {
                        md.push('\n');
                    }
                } else if !section.content.is_empty() {
                    md.push_str(&section.content);
                    if !section.content.ends_with('\n') {
                        md.push('\n');
                    }
                }
                md.push('\n');
            }
        }

        md
    }

    pub fn from_markdown(md: &str) -> Result<Self, String> {
        let mut doc = RunDocument::default();
        let lines: Vec<&str> = md.lines().collect();

        let mut current_section: Option<String> = None;
        let mut in_proposal = false;

        for line in lines.iter() {
            if let Some(rest) = line.strip_prefix("# ") {
                doc.objective = rest.to_string();
                continue;
            }

            if let Some(rest) = line.strip_prefix("## ") {
                let section_name = rest.to_lowercase();
                current_section = Some(section_name);
                in_proposal = false;
                continue;
            }

            if line.trim() == "<!-- proposal -->" {
                in_proposal = true;
                continue;
            }

            if let Some(ref section_id) = current_section {
                if let Some(section) = doc.sections.get_mut(section_id) {
                    if in_proposal {
                        section.proposal = Some(
                            section
                                .proposal
                                .as_ref()
                                .map(|p| format!("{p}\n{line}"))
                                .unwrap_or_else(|| line.to_string()),
                        );
                    } else {
                        if !section.content.is_empty() {
                            section.content.push('\n');
                        }
                        section.content.push_str(line);
                    }
                }
            }
        }

        for section in doc.sections.values_mut() {
            section.content = section.content.trim().to_string();
            if let Some(ref mut proposal) = section.proposal {
                *proposal = proposal.trim().to_string();
            }
        }

        Ok(doc)
    }
}

pub struct RunWriterState {
    pub run_id: String,
    pub desktop_id: String,
    pub session_id: String,
    pub thread_id: String,
    pub objective: String,
    pub event_store: ActorRef<EventStoreMsg>,
    pub document_path: PathBuf,
    pub document_path_relative: String,
    pub revision: u64,
    pub document: RunDocument,
}
