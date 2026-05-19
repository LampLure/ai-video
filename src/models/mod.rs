use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalModelConfig {
    pub name: String,
    pub endpoint: String,
    pub launch_script: Option<String>,
    pub supports_images: bool,
    pub supports_audio: bool,
}

impl Default for LocalModelConfig {
    fn default() -> Self {
        Self {
            name: "local-llamacpp".to_string(),
            endpoint: "http://127.0.0.1:7080/v1/chat/completions".to_string(),
            launch_script: None,
            supports_images: true,
            supports_audio: false,
        }
    }
}

pub fn app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn models_dir() -> PathBuf { app_dir().join("models") }

pub fn ensure_models_dir() -> PathBuf {
    let dir = models_dir();
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn list_model_scripts() -> Vec<PathBuf> {
    let dir = ensure_models_dir();
    let mut scripts = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() { continue; }
            let ext = path.extension().and_then(|value| value.to_str()).unwrap_or_default().to_ascii_lowercase();
            let is_supported = if cfg!(windows) { ext == "bat" || ext == "cmd" } else { ext == "sh" };
            if is_supported { scripts.push(path); }
        }
    }
    scripts.sort();
    scripts
}

pub fn start_model_script(path: &Path) -> Result<Child, String> {
    if !path.exists() { return Err("模型启动文件不存在".to_string()); }
    let mut command = if cfg!(windows) {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").arg(path);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.arg(path);
        cmd
    };
    command
        .current_dir(path.parent().unwrap_or_else(|| Path::new(".")))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command.spawn().map_err(|err| format!("启动模型失败：{err}"))
}

pub fn stop_model_process(child: &mut Child) {
    let pid = child.id();
    if cfg!(windows) {
        let _ = Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub fn kill_7080_processes() -> Result<(), String> {
    let status = if cfg!(windows) {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-ExecutionPolicy", "Bypass",
                "-Command",
                "Get-NetTCPConnection -LocalPort 7080 -ErrorAction SilentlyContinue | Select-Object -ExpandProperty OwningProcess -Unique | ForEach-Object { Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue }",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
    } else {
        Command::new("sh")
            .arg("-c")
            .arg("pids=$( (lsof -ti tcp:7080 2>/dev/null; fuser 7080/tcp 2>/dev/null | tr ' ' '\n') | sort -u); if [ -n \"$pids\" ]; then kill -9 $pids 2>/dev/null || true; fi")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
    };
    status.map(|_| ()).map_err(|err| format!("杀死 7080 端口进程失败：{err}"))
}

pub fn llama_props_url() -> &'static str { "http://127.0.0.1:7080/props" }

pub fn is_llama_service_ready(timeout: Duration) -> bool {
    reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .ok()
        .and_then(|client| client.get(llama_props_url()).send().ok())
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}
