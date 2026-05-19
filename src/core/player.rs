use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::{Child, Command, Stdio};

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
    external_child: Option<Child>,
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
                let _ = self.stop_external();
                self.current_path = None;
                self.current_position = 0.0;
                self.paused = true;
            }
        }
    }

    pub fn open_external_mpv(&mut self, path: &str, start_seconds: f64) -> Result<()> {
        self.stop_finished_external();
        let start = format!("--start={:.3}", start_seconds.max(0.0));
        let child = Command::new("mpv")
            .arg(path)
            .arg(start)
            .arg("--force-window=yes")
            .arg("--keep-open=yes")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| "failed to launch mpv. Please install mpv and ensure it is in PATH")?;
        self.current_path = Some(path.to_string());
        self.current_position = start_seconds.max(0.0);
        self.paused = false;
        self.external_child = Some(child);
        Ok(())
    }

    pub fn stop_external(&mut self) -> Result<()> {
        if let Some(mut child) = self.external_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        Ok(())
    }

    fn stop_finished_external(&mut self) {
        if let Some(child) = self.external_child.as_mut() {
            if child.try_wait().ok().flatten().is_some() {
                self.external_child = None;
            }
        }
    }
}

pub fn open_with_mpv(path: &str, start_seconds: f64) -> Result<()> {
    let start = format!("--start={:.3}", start_seconds.max(0.0));
    Command::new("mpv")
        .arg(path)
        .arg(start)
        .arg("--force-window=yes")
        .arg("--keep-open=yes")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| "failed to launch mpv. Please install mpv and ensure it is in PATH")?;
    Ok(())
}
