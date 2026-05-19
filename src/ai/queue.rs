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
    state: QueueState,
}

impl AnalysisQueue {
    pub fn new() -> Self {
        Self { jobs: VecDeque::new(), current: None, state: QueueState::Idle }
    }

    pub fn load_from(videos: Vec<VideoMeta>, start_index: usize) -> Self {
        let mut jobs = VecDeque::new();
        for video in videos.into_iter().skip(start_index) {
            jobs.push_back(AnalysisJob { video });
        }
        Self { jobs, current: None, state: QueueState::Idle }
    }

    pub fn start(&mut self) {
        if self.current.is_none() {
            self.current = self.jobs.pop_front();
        }
        self.state = if self.current.is_some() { QueueState::Running } else { QueueState::Finished };
    }

    pub fn pause(&mut self) {
        if self.state == QueueState::Running {
            self.state = QueueState::Paused;
        }
    }

    pub fn resume(&mut self) {
        if self.state == QueueState::Paused {
            self.state = QueueState::Running;
        }
    }

    pub fn state(&self) -> QueueState { self.state.clone() }
    pub fn can_user_ask(&self) -> bool { self.state == QueueState::Paused && self.current.is_some() }

    pub fn next_job(&mut self) -> Option<AnalysisJob> {
        if self.state != QueueState::Running { return None; }
        if self.current.is_none() {
            self.current = self.jobs.pop_front();
        }
        if self.current.is_none() {
            self.state = QueueState::Finished;
        }
        self.current.clone()
    }

    pub fn complete_current(&mut self) -> Option<AnalysisJob> {
        let completed = self.current.take();
        self.current = self.jobs.pop_front();
        if self.current.is_none() {
            self.state = QueueState::Finished;
        }
        completed
    }

    pub fn current(&self) -> Option<&AnalysisJob> { self.current.as_ref() }
    pub fn remaining_len(&self) -> usize { self.jobs.len() + usize::from(self.current.is_some()) }
}
