use std::env;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SampleRate};
use log::{error, info, warn};
use chrono::Utc;

use sound_guard_client::*;

// --- Pi System Stats ---
fn get_uptime() -> Option<f64> {
    std::fs::read_to_string("/proc/uptime").ok().and_then(|s| {
        s.split_whitespace().next()?.parse().ok()
    })
}

fn get_cpu_temp() -> Option<f64> {
    // Raspberry Pi thermal zone
    let paths = [
        "/sys/class/thermal/thermal_zone0/temp",
        "/sys/class/hwmon/hwmon0/temp1_input",
    ];
    for p in &paths {
        if let Ok(s) = std::fs::read_to_string(p) {
            if let Ok(raw) = s.trim().parse::<f64>() {
                return Some(if raw > 1000.0 { raw / 1000.0 } else { raw });
            }
        }
    }
    None
}

fn get_mem_free() -> Option<f64> {
    std::fs::read_to_string("/proc/meminfo").ok().and_then(|s| {
        for line in s.lines() {
            if line.starts_with("MemAvailable:") {
                let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
                return Some(kb / 1024.0); // MB
            }
        }
        None
    })
}

fn get_disk_free() -> Option<f64> {
    std::process::Command::new("df")
        .args(["-BG", "/"])
        .output()
        .ok()
        .and_then(|out| {
            let s = String::from_utf8_lossy(&out.stdout);
            for line in s.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    return parts[3].trim_end_matches('G').parse().ok();
                }
            }
            None
        })
}

#[derive(Clone)]
pub struct Config {
    pub server_url: String,
    pub threshold_db: f32,
    pub cooldown: Duration,
    pub sample_rate: u32,
    pub pre_buffer_sec: f32,
    pub post_buffer_sec: f32,
    pub enabled: bool,
}

impl Config {
    fn from_env() -> Self {
        Self {
            server_url: env::var("SERVER_URL")
                .unwrap_or_else(|_| "http://192.168.1.66:3002/api/events".into()),
            threshold_db: env::var("THRESHOLD_DB")
                .unwrap_or_else(|_| "-10".into())
                .parse()
                .unwrap_or(-10.0),
            cooldown: Duration::from_secs_f32(
                env::var("COOLDOWN_SEC")
                    .unwrap_or_else(|_| "5".into())
                    .parse()
                    .unwrap_or(5.0),
            ),
            sample_rate: env::var("SAMPLE_RATE")
                .unwrap_or_else(|_| "44100".into())
                .parse()
                .unwrap_or(44100),
            pre_buffer_sec: env::var("PRE_BUFFER_SEC")
                .unwrap_or_else(|_| "1.0".into())
                .parse()
                .unwrap_or(1.0),
            post_buffer_sec: env::var("POST_BUFFER_SEC")
                .unwrap_or_else(|_| "1.0".into())
                .parse()
                .unwrap_or(1.0),
            enabled: true,
        }
    }

    fn apply_remote(&mut self, settings: &serde_json::Value) {
        if let Some(v) = settings.get("threshold_db").and_then(|v| v.as_f64()) {
            self.threshold_db = v as f32;
        }
        if let Some(v) = settings.get("cooldown_sec").and_then(|v| v.as_f64()) {
            self.cooldown = Duration::from_secs_f32(v as f32);
        }
        if let Some(v) = settings.get("sample_rate").and_then(|v| v.as_u64()) {
            self.sample_rate = v as u32;
        }
        if let Some(v) = settings.get("pre_buffer_sec").and_then(|v| v.as_f64()) {
            self.pre_buffer_sec = v as f32;
        }
        if let Some(v) = settings.get("post_buffer_sec").and_then(|v| v.as_f64()) {
            self.post_buffer_sec = v as f32;
        }
        if let Some(v) = settings.get("enabled").and_then(|v| v.as_i64()) {
            self.enabled = v != 0;
        }
        info!("Settings synced from server — threshold: {} dB, cooldown: {:?}, enabled: {}",
              self.threshold_db, self.cooldown, self.enabled);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let config = Config::from_env();

    info!(
        "Starting Sound Guard — threshold: {} dB, cooldown: {:?}",
        config.threshold_db, config.cooldown
    );

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .context("no input device found")?;

    info!("Using input device: {}", device.name().unwrap_or_default());

    let supported_config = device
        .supported_input_configs()?
        .find(|c| {
            c.channels() <= 2
                && c.min_sample_rate().0 <= config.sample_rate
                && c.max_sample_rate().0 >= config.sample_rate
                && c.sample_format() == SampleFormat::I16
        })
        .or_else(|| device.supported_input_configs().ok()?.next())
        .context("no supported audio config")?;

    let stream_config = supported_config
        .with_sample_rate(SampleRate(config.sample_rate))
        .config();

    let actual_rate = stream_config.sample_rate.0;
    let actual_channels = stream_config.channels;

    let chunk_size: usize = 1024;
    let max_ring =
        (config.pre_buffer_sec * actual_rate as f32 / chunk_size as f32) as usize;
    let post_count =
        (config.post_buffer_sec * actual_rate as f32 / chunk_size as f32) as usize;

    let ring_buffer: Arc<Mutex<RingBuffer>> =
        Arc::new(Mutex::new(RingBuffer::new(max_ring)));
    let last_trigger: Arc<Mutex<Instant>> =
        Arc::new(Mutex::new(Instant::now() - config.cooldown));
    let triggered: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));

    let rb = ring_buffer.clone();
    let lt = last_trigger.clone();
    let tf = triggered.clone();
    let threshold = config.threshold_db;
    let cooldown = config.cooldown;

    let err_fn = |err: cpal::StreamError| {
        error!("Audio stream error: {}", err);
    };

    let stream = device.build_input_stream(
        &stream_config,
        move |data: &[i16], _: &cpal::InputCallbackInfo| {
            let samples: Vec<i16> = if actual_channels == 1 {
                data.to_vec()
            } else {
                data.chunks(actual_channels as usize)
                    .map(|ch| {
                        let sum: i32 = ch.iter().map(|&s| s as i32).sum();
                        (sum / ch.len() as i32) as i16
                    })
                    .collect()
            };

            let rms = calculate_rms(&samples);
            let db = rms_to_db(rms);

            let mut rb = rb.lock().unwrap();
            rb.push(samples);

            let mut lt = lt.lock().unwrap();
            let mut tf = tf.lock().unwrap();

            if db >= threshold && lt.elapsed() >= cooldown && !*tf {
                info!("Peak detected: {:.1} dB — triggering capture", db);
                *lt = Instant::now();
                *tf = true;
            }
        },
        err_fn,
        None,
    )?;

    stream.play()?;
    info!("Audio stream active — listening...");

    let client = reqwest::Client::new();

    // Shared config that can be updated from server
    let config = Arc::new(Mutex::new(config));
    let events_sent = Arc::new(Mutex::new(0u64));

    // Spawn check-in task (every 60s)
    let checkin_client = client.clone();
    let checkin_config = config.clone();
    let checkin_events = events_sent.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;

            let server_url = { checkin_config.lock().unwrap().server_url.clone() };
            let checkin_url = server_url.replace("/api/events", "/api/checkin");
            let sent = *checkin_events.lock().unwrap();

            // Collect Pi stats
            let uptime = get_uptime();
            let cpu_temp = get_cpu_temp();
            let mem_free = get_mem_free();
            let disk_free = get_disk_free();

            let body = serde_json::json!({
                "pi_id": "default",
                "uptime_sec": uptime,
                "cpu_temp_c": cpu_temp,
                "mem_free_mb": mem_free,
                "disk_free_mb": disk_free,
                "events_sent": sent,
            });

            match checkin_client
                .post(&checkin_url)
                .json(&body)
                .timeout(Duration::from_secs(10))
                .send()
                .await
            {
                Ok(resp) => {
                    if let Ok(data) = resp.json::<serde_json::Value>().await {
                        if let Some(settings) = data.get("settings") {
                            let mut cfg = checkin_config.lock().unwrap();
                            cfg.apply_remote(settings);
                        }
                    }
                    info!("Check-in OK");
                }
                Err(e) => warn!("Check-in failed: {}", e),
            }
        }
    });

    loop {
        let is_enabled = { config.lock().unwrap().enabled };
        let is_triggered = *triggered.lock().unwrap();

        if !is_enabled {
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        }

        if is_triggered {
            let (pre_buf, post_buf, sr) = {
                let cfg = config.lock().unwrap();
                let pb = cfg.pre_buffer_sec;
                let pob = cfg.post_buffer_sec;
                let sr = cfg.sample_rate;
                (pb, pob, sr)
            };

            let pre_audio: Vec<i16> = {
                let mut rb = ring_buffer.lock().unwrap();
                rb.drain().into_iter().flatten().collect()
            };

            let mut post_audio: Vec<i16> = Vec::new();
            let chunk_dur =
                Duration::from_secs_f32(chunk_size as f32 / actual_rate as f32);
            for _ in 0..post_count {
                std::thread::sleep(chunk_dur);
                let mut rb = ring_buffer.lock().unwrap();
                for chunk in rb.drain() {
                    post_audio.extend(chunk);
                }
            }

            let mut all_audio = pre_audio;
            all_audio.extend(post_audio);

            let peak_db = rms_to_db(calculate_rms(&all_audio));

            let photo = capture_photo().ok().flatten();
            if photo.is_none() {
                warn!("No photo captured");
            }

            let event = EventData {
                timestamp: Utc::now().to_rfc3339(),
                peak_db: (peak_db * 10.0).round() / 10.0,
                sample_rate: actual_rate,
                duration_sec: pre_buf + post_buf,
            };

            let wav_data = write_wav(&all_audio, actual_rate, 1)?;

            let server_url = config.lock().unwrap().server_url.clone();
            if let Err(e) =
                send_event(&client, &server_url, &event, wav_data, photo).await
            {
                error!("Failed to send event: {}", e);
            } else {
                *events_sent.lock().unwrap() += 1;
            }

            *triggered.lock().unwrap() = false;
            info!("Event processed — waiting for next trigger");
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
