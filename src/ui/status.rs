use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct AiStatusState {
    pub current: String,
    pub steps: Vec<String>,
    pub typing_answer: Option<String>,
    pub typing_cursor: usize,
    pub typing_chat_index: Option<usize>,
    pub typing_last_tick: Option<Instant>,
}

impl Default for AiStatusState {
    fn default() -> Self {
        Self {
            current: "idle".to_string(),
            steps: vec!["app started".to_string()],
            typing_answer: None,
            typing_cursor: 0,
            typing_chat_index: None,
            typing_last_tick: None,
        }
    }
}

impl AiStatusState {
    pub fn set(&mut self, status: impl Into<String>) {
        let status = status.into();
        self.current = status.clone();
        if self.steps.last().map(|last| last != &status).unwrap_or(true) {
            self.steps.push(status);
        }
        if self.steps.len() > 30 {
            let remove_count = self.steps.len() - 30;
            self.steps.drain(0..remove_count);
        }
    }

    pub fn latest_steps_text(&self) -> String {
        let start = self.steps.len().saturating_sub(6);
        self.steps[start..].join(" -> ")
    }

    pub fn is_typing(&self) -> bool {
        self.typing_answer.is_some()
    }

    pub fn begin_typewriter(&mut self, chat_index: usize, answer: String) {
        self.typing_chat_index = Some(chat_index);
        self.typing_answer = Some(answer);
        self.typing_cursor = 0;
        self.typing_last_tick = None;
        self.set("ai answering");
    }

    pub fn tick_typewriter(&mut self, chat_log: &mut [String]) -> bool {
        let Some(answer) = self.typing_answer.as_ref() else { return false; };
        let now = Instant::now();
        if let Some(last) = self.typing_last_tick {
            if now.duration_since(last) < Duration::from_millis(18) {
                return true;
            }
        }
        self.typing_last_tick = Some(now);
        let total = answer.chars().count();
        if self.typing_cursor < total {
            self.typing_cursor = (self.typing_cursor + 2).min(total);
            let visible: String = answer.chars().take(self.typing_cursor).collect();
            if let Some(index) = self.typing_chat_index {
                if let Some(line) = chat_log.get_mut(index) {
                    *line = format!("AI: {}", visible);
                }
            }
            true
        } else {
            self.typing_answer = None;
            self.typing_cursor = 0;
            self.typing_chat_index = None;
            self.typing_last_tick = None;
            self.set("ai answer complete");
            false
        }
    }
}

#[derive(Debug)]
pub enum QaEvent {
    Status(String),
    Answer(String),
    Error(String),
    Finished,
}
