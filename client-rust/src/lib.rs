use std::io::Write;

use anyhow::{Context, Result};
use log::{error, info, warn};
use serde::Serialize;

// --- Audio Analysis ---
pub fn rms_to_db(rms: f32) -> f32 {
    if rms <= 0.0 {
        return f32::NEG_INFINITY;
    }
    20.0 * (rms / 32767.0).log10()
}

pub fn calculate_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

// --- Ring Buffer ---
pub struct RingBuffer {
    chunks: Vec<Vec<i16>>,
    max_chunks: usize,
}

impl RingBuffer {
    pub fn new(max_chunks: usize) -> Self {
        Self {
            chunks: Vec::with_capacity(max_chunks),
            max_chunks,
        }
    }

    pub fn push(&mut self, chunk: Vec<i16>) {
        if self.chunks.len() >= self.max_chunks {
            self.chunks.remove(0);
        }
        self.chunks.push(chunk);
    }

    pub fn drain(&mut self) -> Vec<Vec<i16>> {
        self.chunks.drain(..).collect()
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }
}

// --- Camera ---
pub fn capture_photo() -> Result<Option<Vec<u8>>> {
    // Try rpicam-still first (Bookworm+), then libcamera-jpeg, then fswebcam
    let capture_attempts: Vec<(&str, Vec<&str>)> = vec![
        ("rpicam-still", vec!["--width", "1280", "--height", "720", "--timeout", "1000", "-o", "/dev/stdout"]),
        ("libcamera-jpeg", vec!["--width", "1280", "--height", "720", "--timeout", "1000", "-o", "/dev/stdout"]),
        ("fswebcam", vec!["-r", "1280x720", "--no-banner", "-"]),
    ];

    for (tool, args) in &capture_attempts {
        let output = std::process::Command::new(tool)
            .args(args)
            .output();

        match output {
            Ok(out) if out.status.success() && !out.stdout.is_empty() => {
                info!("Photo captured via {}", tool);
                return Ok(Some(out.stdout));
            }
            _ => {
                warn!("{} failed, trying next...", tool);
            }
        }
    }

    // Last resort: try to temp file (stdout can fail on some Pi configs)
    let tmp = tempfile::NamedTempFile::new().context("temp file")?;
    let tmp_path = tmp.path().to_str().context("path")?.to_string();

    let file_attempts: Vec<(&str, Vec<String>)> = vec![
        ("rpicam-still", vec!["--width".into(), "1280".into(), "--height".into(), "720".into(), "--timeout".into(), "1000".into(), "-o".into(), tmp_path.clone()]),
        ("libcamera-jpeg", vec!["--width".into(), "1280".into(), "--height".into(), "720".into(), "--timeout".into(), "1000".into(), "-o".into(), tmp_path.clone()]),
        ("fswebcam", vec!["-r".into(), "1280x720".into(), "--no-banner".into(), tmp_path.clone()]),
    ];

    for (tool, args) in &file_attempts {
        let output = std::process::Command::new(tool)
            .args(args)
            .output();
        match output {
            Ok(out) if out.status.success() => {
                if let Ok(data) = std::fs::read(&tmp_path) {
                    if !data.is_empty() {
                        info!("Photo captured via {} (temp file)", tool);
                        return Ok(Some(data));
                    }
                }
            }
            _ => continue,
        }
    }

    warn!("All camera tools failed — no photo");
    Ok(None)
}

// --- WAV Writer ---
pub fn write_wav(samples: &[i16], sample_rate: u32, channels: u16) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let data_size = (samples.len() * 2) as u32;
    let file_size = 36 + data_size;

    buf.write_all(b"RIFF")?;
    buf.write_all(&file_size.to_le_bytes())?;
    buf.write_all(b"WAVE")?;

    buf.write_all(b"fmt ")?;
    buf.write_all(&16u32.to_le_bytes())?;
    buf.write_all(&1u16.to_le_bytes())?;
    buf.write_all(&channels.to_le_bytes())?;
    buf.write_all(&sample_rate.to_le_bytes())?;
    let byte_rate = sample_rate * channels as u32 * 2;
    buf.write_all(&byte_rate.to_le_bytes())?;
    let block_align = channels * 2;
    buf.write_all(&block_align.to_le_bytes())?;
    buf.write_all(&16u16.to_le_bytes())?;

    buf.write_all(b"data")?;
    buf.write_all(&data_size.to_le_bytes())?;
    for &sample in samples {
        buf.write_all(&sample.to_le_bytes())?;
    }

    Ok(buf)
}

// --- Event Payload ---
#[derive(Serialize)]
pub struct EventData {
    pub timestamp: String,
    pub peak_db: f32,
    pub sample_rate: u32,
    pub duration_sec: f32,
}

// --- Send Event ---
pub async fn send_event(
    client: &reqwest::Client,
    server_url: &str,
    event: &EventData,
    audio: Vec<u8>,
    photo: Option<Vec<u8>>,
) -> Result<()> {
    let event_json = serde_json::to_string(event)?;

    let mut form = reqwest::multipart::Form::new()
        .text("event", event_json)
        .part(
            "audio",
            reqwest::multipart::Part::bytes(audio)
                .file_name("clip.wav")
                .mime_str("audio/wav")?,
        );

    if let Some(photo_data) = photo {
        form = form.part(
            "photo",
            reqwest::multipart::Part::bytes(photo_data)
                .file_name("photo.jpg")
                .mime_str("image/jpeg")?,
        );
    }

    let resp = client
        .post(server_url)
        .multipart(form)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .context("send event")?;

    info!("Event sent — status {}", resp.status());
    Ok(())
}
