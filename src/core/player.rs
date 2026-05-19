use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlayerCommand {
    Load { path: String },
    Seek { seconds: f64 },
    Pause,
    Resume,
    Stop,
}

#[derive(Debug, Default)]
pub struct PlayerController {
    current_path: Option<String>,
    current_position: f64,
    paused: bool,
}

impl PlayerController {
    pub fn new() -> Self { Self::default() }

    pub fn apply(&mut self, command: PlayerCommand) {
        match command {
            PlayerCommand::Load { path } => {
                self.current_path = Some(path);
                self.current_position = 0.0;
                self.paused = false;
            }
            PlayerCommand::Seek { seconds } => self.current_position = seconds.max(0.0),
            PlayerCommand::Pause => self.paused = true,
            PlayerCommand::Resume => self.paused = false,
            PlayerCommand::Stop => {
                self.current_path = None;
                self.current_position = 0.0;
                self.paused = true;
            }
        }
    }
}
