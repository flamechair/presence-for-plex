use discord_rich_presence::activity;
use log::{error, info, warn};
use serde_json::{json, Value};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::media::PlaybackState;

// Timestamps this far out render as a frozen clock in Discord
const PAUSED_OFFSET: i64 = 9999 * 3600;

pub struct DiscordClient {
    client_id: String,
    socket: Option<File>,
    connected: bool,
    last_target: Option<u32>,
}

impl DiscordClient {
    pub fn new(client_id: &str) -> Self {
        Self {
            client_id: client_id.to_string(),
            socket: None,
            connected: false,
            last_target: None,
        }
    }

    /// Reconnect using the last successful target (or auto-scan if never connected).
    pub fn reconnect(&mut self) -> bool {
        match self.last_target {
            Some(i) => self.connect_to(i),
            None => self.connect_auto(),
        }
    }

    /// Auto-discover: scan pipes 0..9, connect to first responder.
    pub fn connect_auto(&mut self) -> bool {
        if self.connected {
            self.disconnect();
        }
        for i in 0..10 {
            let path = PathBuf::from(format!(r"\\?\pipe\discord-ipc-{}", i));
            match Self::open_pipe(&path) {
                Ok(handle) => {
                    self.socket = Some(handle);
                    if self.send_handshake().is_ok() {
                        info!("Connected to Discord (pipe {})", i);
                        self.connected = true;
                        self.last_target = Some(i);
                        return true;
                    }
                    self.socket = None;
                }
                Err(_) => continue,
            }
        }
        error!("No Discord clients found (tried pipes 0-9)");
        false
    }

    /// Target a specific pipe by index (0=stable, 1=ptb, 2=canary).
    pub fn connect_to(&mut self, pipe_index: u32) -> bool {
        if self.connected {
            self.disconnect();
        }
        let path = PathBuf::from(format!(r"\\?\pipe\discord-ipc-{}", pipe_index));
        match Self::open_pipe(&path) {
            Ok(handle) => {
                self.socket = Some(handle);
                if self.send_handshake().is_ok() {
                    info!("Connected to Discord (pipe {})", pipe_index);
                    self.connected = true;
                    self.last_target = Some(pipe_index);
                    true
                } else {
                    self.socket = None;
                    error!("Discord handshake failed for pipe {}", pipe_index);
                    false
                }
            }
            Err(e) => {
                error!(
                    "Discord pipe {} not available: {}",
                    pipe_index, e
                );
                false
            }
        }
    }

    fn open_pipe(path: &PathBuf) -> std::io::Result<File> {
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            OpenOptions::new().access_mode(0x3).open(path)
        }
        #[cfg(not(windows))]
        {
            OpenOptions::new().read(true).write(true).open(path)
        }
    }

    fn send_handshake(&mut self) -> Result<(), String> {
        let payload = json!({"v": 1, "client_id": self.client_id});
        self.send_frame(&payload, 0)?;
        // Read the handshake response (ignore body, just ensure it succeeds)
        self.recv_frame()?;
        Ok(())
    }

    fn send_frame(&mut self, data: &Value, opcode: u32) -> Result<(), String> {
        let data_str = data.to_string();
        let data_bytes = data_str.as_bytes();
        let header = pack(opcode, data_bytes.len() as u32);
        let socket = self.socket.as_mut().ok_or("Not connected".to_string())?;
        socket
            .write_all(&header)
            .map_err(|e| format!("Write error: {}", e))?;
        socket
            .write_all(data_bytes)
            .map_err(|e| format!("Write error: {}", e))?;
        Ok(())
    }

    fn recv_frame(&mut self) -> Result<(u32, Value), String> {
        let mut header = [0u8; 8];
        let socket = self.socket.as_mut().ok_or("Not connected".to_string())?;
        socket
            .read_exact(&mut header)
            .map_err(|e| format!("Read error: {}", e))?;
        let (opcode, length) = unpack(&header)?;
        let mut data = vec![0u8; length as usize];
        socket
            .read_exact(&mut data)
            .map_err(|e| format!("Read error: {}", e))?;
        let response =
            std::str::from_utf8(&data).map_err(|e| format!("UTF-8 error: {}", e))?;
        let json_data: Value =
            serde_json::from_str(response).map_err(|e| format!("JSON error: {}", e))?;
        Ok((opcode, json_data))
    }

    pub fn disconnect(&mut self) {
        if self.connected {
            // Send close opcode (2) before dropping
            let _ = self.send_frame(&json!({}), 2);
            self.socket = None;
            self.connected = false;
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn update(&mut self, p: &Presence) {
        if !self.connected {
            return;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let display = match p.activity_type {
            ActivityType::Listening => activity::StatusDisplayType::State,
            _ => activity::StatusDisplayType::Details,
        };

        let mut b = activity::Activity::new()
            .activity_type(p.activity_type.into())
            .status_display_type(display);

        // Discord rejects strings under 2 chars
        if p.details.chars().count() >= 2 {
            b = b.details(&p.details);
        }
        if p.state.chars().count() >= 2 {
            b = b.state(&p.state);
        }

        if p.show_timestamps {
            b = match p.playback_state {
                PlaybackState::Playing => {
                    let prog = (p.progress_ms / 1000) as i64;
                    let rem = (p.duration_ms.saturating_sub(p.progress_ms) / 1000) as i64;
                    b.timestamps(activity::Timestamps::new().start(now - prog).end(now + rem))
                }
                PlaybackState::Paused | PlaybackState::Buffering => {
                    let dur = (p.duration_ms / 1000) as i64;
                    b.timestamps(
                        activity::Timestamps::new()
                            .start(now + PAUSED_OFFSET)
                            .end(now + PAUSED_OFFSET + dur),
                    )
                }
            };
        }

        let mut assets = activity::Assets::new();
        if let Some(ref url) = p.large_image {
            assets = assets.large_image(url).large_text(&p.large_image_text);
        }
        if p.playback_state == PlaybackState::Paused {
            assets = assets.small_image("paused").small_text("Paused");
        }
        b = b.assets(assets);

        if !p.buttons.is_empty() {
            b = b.buttons(
                p.buttons
                    .iter()
                    .take(2)
                    .map(|btn| activity::Button::new(&btn.label, &btn.url))
                    .collect(),
            );
        }

        let payload = json!({
            "cmd": "SET_ACTIVITY",
            "args": {
                "pid": std::process::id(),
                "activity": b
            },
            "nonce": uuid::Uuid::new_v4().to_string()
        });

        if let Err(e) = self.send_frame(&payload, 1) {
            error!("Presence update failed: {}", e);
            self.disconnect();
        }
    }

    pub fn clear(&mut self) {
        if self.connected {
            let payload = json!({
                "cmd": "SET_ACTIVITY",
                "args": {
                    "pid": std::process::id(),
                    "activity": null
                },
                "nonce": uuid::Uuid::new_v4().to_string()
            });
            let _ = self.send_frame(&payload, 1);
        }
    }
}

// ── Binary protocol helpers ────────────────────────────────────

fn pack(opcode: u32, data_len: u32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8);
    bytes.extend_from_slice(&opcode.to_le_bytes());
    bytes.extend_from_slice(&data_len.to_le_bytes());
    bytes
}

fn unpack(data: &[u8]) -> Result<(u32, u32), String> {
    if data.len() < 8 {
        return Err("Header too short".to_string());
    }
    let opcode = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let length = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    Ok((opcode, length))
}

// ── Public types (unchanged) ───────────────────────────────────

#[derive(Debug, Clone)]
pub struct Presence {
    pub details: String,
    pub state: String,
    pub large_image: Option<String>,
    pub large_image_text: String,
    pub progress_ms: u64,
    pub duration_ms: u64,
    pub show_timestamps: bool,
    pub activity_type: ActivityType,
    pub playback_state: PlaybackState,
    pub buttons: Vec<Button>,
}

#[derive(Debug, Clone, Copy)]
pub enum ActivityType {
    Watching,
    Listening,
}

impl From<ActivityType> for activity::ActivityType {
    fn from(t: ActivityType) -> Self {
        match t {
            ActivityType::Watching => activity::ActivityType::Watching,
            ActivityType::Listening => activity::ActivityType::Listening,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Button {
    pub label: String,
    pub url: String,
}
