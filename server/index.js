const express = require("express");
const multer = require("multer");
const path = require("path");
const fs = require("fs");
const Database = require("better-sqlite3");

const app = express();
app.use(express.json());
const PORT = process.env.PORT || 3002;
const DATA_DIR = process.env.DATA_DIR || "./data";
const DB_PATH = path.join(DATA_DIR, "events.db");
const UPLOADS_DIR = path.join(DATA_DIR, "uploads");

// Ensure dirs
fs.mkdirSync(UPLOADS_DIR, { recursive: true });

// DB
const db = new Database(DB_PATH);
db.pragma("journal_mode = WAL");
db.exec(`
  CREATE TABLE IF NOT EXISTS events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    peak_db REAL NOT NULL,
    sample_rate INTEGER DEFAULT 44100,
    duration_sec REAL DEFAULT 2.0,
    audio_path TEXT,
    photo_path TEXT,
    created_at TEXT DEFAULT (datetime('now'))
  );

  CREATE TABLE IF NOT EXISTS pi_checkins (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    pi_id TEXT NOT NULL DEFAULT 'default',
    timestamp TEXT NOT NULL,
    uptime_sec REAL,
    cpu_temp_c REAL,
    mem_free_mb REAL,
    disk_free_mb REAL,
    events_sent INTEGER DEFAULT 0,
    created_at TEXT DEFAULT (datetime('now'))
  );

  CREATE TABLE IF NOT EXISTS pi_settings (
    pi_id TEXT PRIMARY KEY DEFAULT 'default',
    threshold_db REAL DEFAULT -10,
    cooldown_sec REAL DEFAULT 5,
    sample_rate INTEGER DEFAULT 44100,
    pre_buffer_sec REAL DEFAULT 1.0,
    post_buffer_sec REAL DEFAULT 1.0,
    enabled INTEGER DEFAULT 1,
    updated_at TEXT DEFAULT (datetime('now'))
  );

  INSERT OR IGNORE INTO pi_settings (pi_id) VALUES ('default');
`);

// Multer for file uploads
const storage = multer.diskStorage({
  destination: (_req, _file, cb) => cb(null, UPLOADS_DIR),
  filename: (_req, file, cb) => {
    const ext = path.extname(file.originalname);
    cb(null, `${Date.now()}-${Math.random().toString(36).slice(2, 8)}${ext}`);
  },
});
const upload = multer({ storage, limits: { fileSize: 20 * 1024 * 1024 } });

// --- API ---

// Receive an event from the Pi
app.post("/api/events", upload.fields([
  { name: "audio", maxCount: 1 },
  { name: "photo", maxCount: 1 },
]), (req, res) => {
  const event = JSON.parse(req.body.event || "{}");
  const audioFile = req.files?.audio?.[0];
  const photoFile = req.files?.photo?.[0];

  const stmt = db.prepare(`
    INSERT INTO events (timestamp, peak_db, sample_rate, duration_sec, audio_path, photo_path)
    VALUES (?, ?, ?, ?, ?, ?)
  `);
  const result = stmt.run(
    event.timestamp || new Date().toISOString(),
    event.peak_db ?? 0,
    event.sample_rate || 44100,
    event.duration_sec || 2.0,
    audioFile?.filename || null,
    photoFile?.filename || null,
  );

  res.json({ ok: true, id: result.lastInsertRowid });
});

// List events (paginated)
app.get("/api/events", (req, res) => {
  const limit = Math.min(parseInt(req.query.limit) || 50, 200);
  const offset = parseInt(req.query.offset) || 0;
  const since = req.query.since; // ISO timestamp

  let query = "SELECT * FROM events";
  const params = [];

  if (since) {
    query += " WHERE timestamp >= ?";
    params.push(since);
  }

  query += " ORDER BY id DESC LIMIT ? OFFSET ?";
  params.push(limit, offset);

  const events = db.prepare(query).all(...params);
  const total = db.prepare("SELECT COUNT(*) as count FROM events").get().count;

  // Add URLs for files
  for (const e of events) {
    if (e.audio_path) e.audio_url = `/uploads/${e.audio_path}`;
    if (e.photo_path) e.photo_url = `/uploads/${e.photo_path}`;
  }

  res.json({ events, total, limit, offset });
});

// Single event
app.get("/api/events/:id", (req, res) => {
  const event = db.prepare("SELECT * FROM events WHERE id = ?").get(req.params.id);
  if (!event) return res.status(404).json({ error: "not found" });
  if (event.audio_path) event.audio_url = `/uploads/${event.audio_path}`;
  if (event.photo_path) event.photo_url = `/uploads/${event.photo_path}`;
  res.json(event);
});

// Metrics: hourly/daily aggregations
app.get("/api/metrics", (req, res) => {
  const period = req.query.period || "hour"; // hour | day
  const since = req.query.since || new Date(Date.now() - 7 * 86400000).toISOString();

  const granularity = period === "day"
    ? "%Y-%m-%d"
    : "%Y-%m-%dT%H:00:00";

  const metrics = db.prepare(`
    SELECT
      strftime('${granularity}', timestamp) as period,
      COUNT(*) as event_count,
      ROUND(AVG(peak_db), 1) as avg_db,
      ROUND(MAX(peak_db), 1) as max_db,
      ROUND(MIN(peak_db), 1) as min_db
    FROM events
    WHERE timestamp >= ?
    GROUP BY period
    ORDER BY period DESC
  `).all(since);

  res.json({ metrics });
});

// Stats summary
app.get("/api/stats", (_req, res) => {
  const stats = db.prepare(`
    SELECT
      COUNT(*) as total_events,
      ROUND(AVG(peak_db), 1) as avg_db,
      ROUND(MAX(peak_db), 1) as max_db,
      MIN(timestamp) as first_event,
      MAX(timestamp) as last_event
    FROM events
  `).get();
  res.json(stats);
});

// --- Pi Check-in ---
app.post("/api/checkin", (req, res) => {
  const data = req.body || {};
  const piId = data.pi_id || "default";

  db.prepare(`
    INSERT INTO pi_checkins (pi_id, timestamp, uptime_sec, cpu_temp_c, mem_free_mb, disk_free_mb, events_sent)
    VALUES (?, ?, ?, ?, ?, ?, ?)
  `).run(
    piId,
    new Date().toISOString(),
    data.uptime_sec ?? null,
    data.cpu_temp_c ?? null,
    data.mem_free_mb ?? null,
    data.disk_free_mb ?? null,
    data.events_sent ?? 0,
  );

  // Return current settings so the Pi can sync
  const settings = db.prepare("SELECT * FROM pi_settings WHERE pi_id = ?").get(piId);
  res.json({ ok: true, settings });
});

// Get latest check-in
app.get("/api/checkin/latest", (_req, res) => {
  const checkin = db.prepare("SELECT * FROM pi_checkins ORDER BY id DESC LIMIT 1").get();
  res.json(checkin || null);
});

// Get check-in history
app.get("/api/checkins", (req, res) => {
  const limit = Math.min(parseInt(req.query.limit) || 50, 200);
  const checkins = db.prepare("SELECT * FROM pi_checkins ORDER BY id DESC LIMIT ?").all(limit);
  res.json(checkins);
});

// --- Pi Settings ---
app.get("/api/settings", (_req, res) => {
  const settings = db.prepare("SELECT * FROM pi_settings WHERE pi_id = 'default'").get();
  res.json(settings);
});

app.patch("/api/settings", (req, res) => {
  const allowed = ["threshold_db", "cooldown_sec", "sample_rate", "pre_buffer_sec", "post_buffer_sec", "enabled"];
  const fields = [];
  const values = [];

  for (const key of allowed) {
    if (req.body[key] !== undefined) {
      fields.push(`${key} = ?`);
      values.push(req.body[key]);
    }
  }

  if (fields.length === 0) {
    return res.status(400).json({ error: "no valid fields" });
  }

  fields.push("updated_at = datetime('now')");
  values.push("default"); // for WHERE

  db.prepare(`UPDATE pi_settings SET ${fields.join(", ")} WHERE pi_id = ?`).run(...values);
  const updated = db.prepare("SELECT * FROM pi_settings WHERE pi_id = 'default'").get();
  res.json(updated);
});

// Delete an event
app.delete("/api/events/:id", (req, res) => {
  const event = db.prepare("SELECT * FROM events WHERE id = ?").get(req.params.id);
  if (!event) return res.status(404).json({ error: "not found" });

  // Clean up files
  if (event.audio_path) {
    const p = path.join(UPLOADS_DIR, event.audio_path);
    if (fs.existsSync(p)) fs.unlinkSync(p);
  }
  if (event.photo_path) {
    const p = path.join(UPLOADS_DIR, event.photo_path);
    if (fs.existsSync(p)) fs.unlinkSync(p);
  }

  db.prepare("DELETE FROM events WHERE id = ?").run(req.params.id);
  res.json({ ok: true });
});

// Static files
app.use("/uploads", express.static(UPLOADS_DIR));
app.use(express.static(path.join(__dirname, "public")));

app.listen(PORT, () => {
  console.log(`Sound Guard server listening on port ${PORT}`);
});
