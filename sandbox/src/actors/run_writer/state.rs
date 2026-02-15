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
        let mut body_blocks: Vec<String> = Vec::new();

        for section_id in section_order {
            if let Some(section) = self.sections.get(section_id) {
                if !section.content.trim().is_empty() {
                    body_blocks.push(section.content.trim().to_string());
                }
            }
        }
        for section_id in section_order {
            if let Some(section) = self.sections.get(section_id) {
                if let Some(ref proposal) = section.proposal {
                    if !proposal.trim().is_empty() {
                        body_blocks.push(format!(
                            "<!-- proposal -->\n{}\n<!-- /proposal -->",
                            proposal.trim()
                        ));
                    }
                }
            }
        }

        if !body_blocks.is_empty() {
            md.push_str(&body_blocks.join("\n\n"));
            md.push('\n');
        }

        md
    }

    pub fn from_markdown(md: &str) -> Result<Self, String> {
        if md.contains("\n## Conductor")
            || md.contains("\n## Researcher")
            || md.contains("\n## Terminal")
            || md.contains("\n## User")
        {
            return Self::from_sectioned_markdown(md);
        }
        Self::from_flat_markdown(md)
    }

    fn from_sectioned_markdown(md: &str) -> Result<Self, String> {
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
            if line.trim() == "<!-- /proposal -->" {
                in_proposal = false;
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

    fn from_flat_markdown(md: &str) -> Result<Self, String> {
        let mut doc = RunDocument::default();
        let lines: Vec<&str> = md.lines().collect();

        let mut in_proposal = false;

        for line in lines {
            if let Some(rest) = line.strip_prefix("# ") {
                doc.objective = rest.trim().to_string();
                continue;
            }
            if line.trim() == "<!-- proposal -->" {
                in_proposal = true;
                continue;
            }
            if line.trim() == "<!-- /proposal -->" {
                in_proposal = false;
                continue;
            }
            if in_proposal {
                let section = doc
                    .sections
                    .get_mut("researcher")
                    .ok_or_else(|| "missing researcher section".to_string())?;
                section.proposal = Some(
                    section
                        .proposal
                        .as_ref()
                        .map(|p| format!("{p}\n{line}"))
                        .unwrap_or_else(|| line.to_string()),
                );
            } else {
                let section = doc
                    .sections
                    .get_mut("conductor")
                    .ok_or_else(|| "missing conductor section".to_string())?;
                if !section.content.is_empty() {
                    section.content.push('\n');
                }
                section.content.push_str(line);
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
