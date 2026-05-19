use crate::core::video_manager::VideoMeta;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisJob {
    pub video: VideoMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum QueueState {
    Idle,
    Running,
    Paused,
    Finished,
}

#[derive(Debug, Default)]
pub struct AnalysisQueue {
    jobs: VecDeque<AnalysisJob>,
    current: Option<AnalysisJob>,
    state: Option<QueueState>,
}

impl AnalysisQueue {
    pub fn new() -> Self { Self { jobs: VecDeque::new(), current: None, state: Some(QueueState::Idle) } }

    pub fn load_from(videos: Vec<VideoMeta>, start_index: usize) -> Self {
        let mut jobs = VecDeque::new();
        for video in videos.into_iter().skip(start_index) {
            jobs.push_back(AnalysisJob { video });
        }
        Self { jobs, current: None, state: Some(QueueState::Idle) }
    }

    pub fn start(&mut self) { self.state = Some(QueueState::Running); }
    pub fn pause(&mut self) { self.state = Some(QueueState::Paused); }
    pub fn state(&self) -> QueueState { self.state.clone().unwrap_or(QueueState::Idle) }
    pub fn can_user_ask(&self) -> bool { self.state() == QueueState::Paused }

    pub fn next_job(&mut self) -> Option<AnalysisJob> {
        if self.state() != QueueState::Running { return None; }
        self.current = self.jobs.pop_front();
        if self.current.is_none() { self.state = Some(QueueState::Finished); }
        self.current.clone()
    }

    pub fn current(&self) -> Option<&AnalysisJob> { self.current.as_ref() }
}
