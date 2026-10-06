use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use crate::error::SimError;

/// Data-driven dialogue (design/05 chat + 2006 console-talk): trees of
/// states with player options. Effects that touch the world (trade, missions)
/// are returned as intents for the caller to execute — trees stay pure.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub label: String,
    /// Next state id. None ends the conversation.
    pub next: Option<String>,
    /// World effect tag, e.g. "trade", "quest:find-cat". Caller executes.
    #[serde(default)]
    pub effect: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub text: String,
    pub options: Vec<Choice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dialogue {
    pub id: String,
    pub start: String,
    /// Alternate starts, e.g. cop greeting differs when Wanted.
    #[serde(default)]
    pub starts: BTreeMap<String, String>,
    pub states: BTreeMap<String, Node>,
}

impl Dialogue {
    pub fn validate(&self) -> Result<(), SimError> {
        if !self.states.contains_key(&self.start) {
            return Err(SimError::new("data", "existing start", "dangling"));
        }
        for node in self.states.values() {
            if node.options.is_empty() {
                continue; // terminal flavor text
            }
            for c in &node.options {
                if c.label.trim().is_empty() {
                    return Err(SimError::new("data", "labeled choice", "blank"));
                }
                if let Some(next) = &c.next {
                    if !self.states.contains_key(next) {
                        return Err(SimError::new("data", "existing next", "dangling"));
                    }
                }
            }
        }
        for next in self.starts.values() {
            if !self.states.contains_key(next) {
                return Err(SimError::new("data", "existing alt start", "dangling"));
            }
        }
        Ok(())
    }

    pub fn start_for(&self, key: Option<&str>) -> &str {
        key.and_then(|k| self.starts.get(k)).map(|s| s.as_str()).unwrap_or(&self.start)
    }
}

#[derive(Debug, Clone)]
pub struct Conversation {
    pub dialogue: std::rc::Rc<Dialogue>,
    pub state: String,
    pub done: bool,
    pub last_effect: Option<String>,
}

impl Conversation {
    pub fn new(dialogue: std::rc::Rc<Dialogue>, start_key: Option<&str>) -> Self {
        let state = dialogue.start_for(start_key).to_string();
        Self { dialogue, state, done: false, last_effect: None }
    }

    pub fn text(&self) -> &str {
        &self.dialogue.states[&self.state].text
    }

    pub fn options(&self) -> &[Choice] {
        &self.dialogue.states[&self.state].options
    }

    /// Pick option idx. Returns true when the conversation just ended.
    /// World effects surface via `last_effect` for the caller to execute.
    pub fn choose(&mut self, idx: usize) -> Result<bool, SimError> {
        if self.done {
            return Err(SimError::new("talk_over", "open talk", "ended"));
        }
        let choice = self.dialogue.states[&self.state].options.get(idx)
            .ok_or(SimError::new("no_choice", "listed option", "bad index"))?
            .clone();
        self.last_effect = choice.effect.clone();
        match choice.next {
            Some(next) => {
                self.state = next;
                Ok(false)
            }
            None => {
                self.done = true;
                Ok(true)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shopkeeper() -> std::rc::Rc<Dialogue> {
        let mut states = BTreeMap::new();
        states.insert("hi".to_string(), Node {
            text: "Welcome in. Shelves are stocked, prices are fair.".to_string(),
            options: vec![
                Choice { label: "Trade.".to_string(), next: None, effect: Some("trade".to_string()) },
                Choice { label: "Just looking.".to_string(), next: Some("bye".to_string()), effect: None },
            ],
        });
        states.insert("bye".to_string(), Node {
            text: "Mind the door on your way out.".to_string(),
            options: vec![],
        });
        std::rc::Rc::new(Dialogue { id: "shop".to_string(), start: "hi".to_string(), starts: BTreeMap::new(), states })
    }

    #[test]
    fn walk_to_trade_effect() {
        let d = shopkeeper();
        d.validate().unwrap();
        let mut c = Conversation::new(d.clone(), None);
        assert!(c.text().contains("Welcome"));
        assert_eq!(c.options().len(), 2);
        assert!(!c.choose(1).unwrap()); // "Just looking." -> bye state
        assert!(!c.done);
        // Terminal node: no options, conversation stalls open (caller closes UI).
        assert!(c.choose(0).is_err());
        assert_eq!(c.text(), "Mind the door on your way out.");
        assert!(c.options().is_empty());
    }

    #[test]
    fn trade_choice_ends_with_effect() {
        let d = shopkeeper();
        let mut c = Conversation::new(d.clone(), None);
        assert!(c.choose(0).unwrap());
        assert!(c.done);
        assert_eq!(c.last_effect.as_deref(), Some("trade"));
        assert!(c.choose(0).is_err()); // ended
        assert!(c.choose(9).is_err());
    }

    #[test]
    fn validation_rejects_dangling() {
        let mut states = BTreeMap::new();
        states.insert("hi".to_string(), Node {
            text: "x".to_string(),
            options: vec![Choice { label: "go".to_string(), next: Some("nowhere".to_string()), effect: None }],
        });
        let d = Dialogue { id: "bad".to_string(), start: "hi".to_string(), starts: BTreeMap::new(), states };
        assert!(d.validate().is_err());
        let d2 = Dialogue { id: "bad2".to_string(), start: "missing".to_string(), starts: BTreeMap::new(), states: BTreeMap::new() };
        assert!(d2.validate().is_err());
    }

    #[test]
    fn alt_starts_branch() {
        let mut inner = shopkeeper();
        std::rc::Rc::get_mut(&mut inner).unwrap().starts.insert("wanted".to_string(), "bye".to_string());
        let d = inner;
        assert_eq!(Conversation::new(d.clone(), Some("wanted")).text(), "Mind the door on your way out.");
        assert_eq!(Conversation::new(d.clone(), None).text(), "Welcome in. Shelves are stocked, prices are fair.");
    }
}
