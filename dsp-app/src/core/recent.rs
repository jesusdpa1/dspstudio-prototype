use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentRecording {
    pub name: String,
    pub path: PathBuf,
    pub last_opened: u64, // Unix timestamp
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecentStore {
    pub recordings: Vec<RecentRecording>,
}

impl RecentStore {
    pub fn load() -> Self {
        let path = recent_config_path();
        if let Ok(json) = std::fs::read_to_string(path) {
            serde_json::from_str(&json).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) {
        let path = recent_config_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn add(&mut self, name: String, path: PathBuf) {
        // Remove existing if any
        self.recordings.retain(|r| r.path != path);
        
        self.recordings.insert(0, RecentRecording {
            name,
            path,
            last_opened: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        });

        // Limit to top 20
        self.recordings.truncate(20);
        self.save();
    }

    pub fn remove(&mut self, path: &PathBuf) {
        self.recordings.retain(|r| &r.path != path);
        self.save();
    }
}

fn recent_config_path() -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".config/dsp-studio/recent.json");
        }
    }
    // Fallback
    PathBuf::from(".dsp_studio_recent.json")
}
