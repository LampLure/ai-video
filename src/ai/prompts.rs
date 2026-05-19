use std::path::PathBuf;
use std::process::{Command, Stdio};

pub const VIDEO_ANALYSIS_PROMPT: &str = "video_analysis_prompt.md";
pub const VIDEO_QA_AGENT_PROMPT: &str = "video_qa_agent_prompt.md";
pub const RESPONSE_SCHEMA_PROMPT: &str = "response_schema_prompt.md";

pub fn prompts_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("prompts")
}

pub fn ensure_prompt_files() -> PathBuf {
    let dir = prompts_dir();
    let _ = std::fs::create_dir_all(&dir);
    ensure_file(VIDEO_ANALYSIS_PROMPT, DEFAULT_VIDEO_ANALYSIS_PROMPT);
    ensure_file(VIDEO_QA_AGENT_PROMPT, DEFAULT_VIDEO_QA_AGENT_PROMPT);
    ensure_file(RESPONSE_SCHEMA_PROMPT, DEFAULT_RESPONSE_SCHEMA_PROMPT);
    dir
}

pub fn prompt_path(file_name: &str) -> PathBuf {
    ensure_prompt_files().join(file_name)
}

pub fn read_prompt(file_name: &str) -> String {
    let path = prompt_path(file_name);
    std::fs::read_to_string(&path).unwrap_or_else(|_| default_prompt(file_name).to_string())
}

pub fn open_prompt_file(file_name: &str) -> Result<(), String> {
    let path = prompt_path(file_name);
    if !path.exists() {
        std::fs::write(&path, default_prompt(file_name)).map_err(|err| err.to_string())?;
    }
    open_path(&path)
}

fn ensure_file(file_name: &str, default_text: &str) {
    let path = prompts_dir().join(file_name);
    if !path.exists() {
        let _ = std::fs::write(path, default_text);
    }
}

fn default_prompt(file_name: &str) -> &'static str {
    match file_name {
        VIDEO_ANALYSIS_PROMPT => DEFAULT_VIDEO_ANALYSIS_PROMPT,
        VIDEO_QA_AGENT_PROMPT => DEFAULT_VIDEO_QA_AGENT_PROMPT,
        RESPONSE_SCHEMA_PROMPT => DEFAULT_RESPONSE_SCHEMA_PROMPT,
        _ => "",
    }
}

fn open_path(path: &PathBuf) -> Result<(), String> {
    let status = if cfg!(windows) {
        Command::new("cmd").args(["/C", "start", "", &path.to_string_lossy()]).stdout(Stdio::null()).stderr(Stdio::null()).status()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(path).stdout(Stdio::null()).stderr(Stdio::null()).status()
    } else {
        Command::new("xdg-open").arg(path).stdout(Stdio::null()).stderr(Stdio::null()).status()
    };
    status.map(|_| ()).map_err(|err| format!("打开提示词文件失败：{err}"))
}

const DEFAULT_VIDEO_ANALYSIS_PROMPT: &str = r#"# 视频简介生成提示词

你是本地视频内容分析模型。程序会提供视频元数据、抽帧时间点、图片缓存文件路径、音频 wav 文件路径。

目标：生成中文、结构化、可搜索的视频简介。

要求：
1. 只根据提供的采样内容和元数据判断，不要编造没有证据的细节。
2. 简介要适合本地视频库搜索。
3. 标签应短、稳定、便于检索。
4. 场景描述应包含时间范围、画面主体、动作、环境、可见文字或音频线索。
5. 画质字段应描述清晰度、亮度、模糊、失真、稳定性等。
6. 音频字段应描述是否有人声、音乐、噪声、对白或旁白。
7. 必须严格遵循 response_schema_prompt.md 中的 JSON 格式。
8. 不要输出 markdown，不要输出解释，只输出 JSON。
"#;

const DEFAULT_VIDEO_QA_AGENT_PROMPT: &str = r#"# 当前视频问答 Agent 限制文本

你正在回答用户针对当前视频的问题。

程序会给你：
- 视频总时长
- 最大可请求图片数量
- 最大可请求音频段数量
- 用户问题

当你需要更多证据时，输出标准 JSON 请求，让程序抽取指定时间段的数据。请求必须符合：

{
  "type": "request_segment",
  "video_path": "程序提供的视频路径",
  "video_hash": "程序提供的视频 hash",
  "frame_times": [0.0],
  "audio_centers": [0.0]
}

规则：
1. frame_times 数量不能超过程序给出的最大图片数。
2. audio_centers 数量不能超过程序给出的最大音频段数。
3. 所有时间点必须在 0 到视频总时长之间。
4. 如果用户问“最后讲了什么”，应优先请求靠近视频结尾的图片和音频。
5. 如果用户问全局内容，应均匀请求少量图片和音频。
6. 如果已有信息足够回答，则直接用中文回答，不要请求数据。
"#;

const DEFAULT_RESPONSE_SCHEMA_PROMPT: &str = r#"# AI 简介 JSON Schema

必须只输出严格 JSON，结构如下：

{
  "title": "string",
  "summary": "string",
  "tags": ["string"],
  "scenes": [
    {
      "start": 0.0,
      "end": 0.0,
      "description": "string",
      "tags": ["string"]
    }
  ],
  "quality": {
    "resolution_quality": "string or null",
    "blur": "string or null",
    "distortion": "string or null",
    "brightness": "string or null"
  },
  "audio": {
    "has_audio": true,
    "speech": "string or null",
    "music": "string or null",
    "noise": "string or null"
  }
}

禁止输出 markdown 代码块。禁止输出 JSON 以外的解释文本。
"#;
