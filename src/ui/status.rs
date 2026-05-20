use std::time::{Duration, Instant};

pub const STATUS_IDLE: &str = "空闲";
pub const STATUS_APP_STARTED: &str = "系统已启动";
pub const STATUS_AI_RECEIVED_USER_MESSAGE: &str = "AI 接收到用户消息";
pub const STATUS_AI_ANALYZING: &str = "AI 分析中";
pub const STATUS_AI_REQUESTED_PROGRAM: &str = "AI 向程序请求视频片段";
pub const STATUS_PROGRAM_REQUEST_OK: &str = "程序请求发送成功";
pub const STATUS_AI_ANSWERING: &str = "AI 回答中";
pub const STATUS_AI_ANSWER_COMPLETE: &str = "AI 回答完成";

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
            current: STATUS_IDLE.to_string(),
            steps: vec![STATUS_APP_STARTED.to_string()],
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
        self.set(STATUS_AI_ANSWERING);
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
                    *line = format!("AI：{}", visible);
                }
            }
            true
        } else {
            self.typing_answer = None;
            self.typing_cursor = 0;
            self.typing_chat_index = None;
            self.typing_last_tick = None;
            self.set(STATUS_AI_ANSWER_COMPLETE);
            false
        }
    }
}

#[derive(Debug)]
pub enum QaEvent {
    Status(String),
    Delta(String),
    Answer(String),
    Error(String),
    Finished,
}

#[derive(Debug)]
pub enum AiUiEvent {
    Status(String),
    Delta(String),
    Message(String),
    Finished,
}
