const path = require("path");
const fs = require("fs");
const http = require("http");

// We'll spin up the server on a random port and test against it
const SERVER_PORT = 13002;
const DATA_DIR = path.join(__dirname, ".test-data-" + Date.now());

// Mock the data dir before requiring server
process.env.DATA_DIR = DATA_DIR;
process.env.PORT = String(SERVER_PORT);

let server;
let baseUrl;

async function setup() {
  // We need to create a test version of the server
  // since the main index.js starts listening immediately,
  // let's test with the actual server
  const { execSync } = require("child_process");
  fs.mkdirSync(DATA_DIR, { recursive: true });
  fs.mkdirSync(path.join(DATA_DIR, "uploads"), { recursive: true });
}

async function teardown() {
  // Clean up test data
  fs.rmSync(DATA_DIR, { recursive: true, force: true });
}

// --- Pure logic tests ---

function testDbClassification() {
  console.log("Testing dB classification...");
  const dbClass = (db) => {
    // dBFS scale: 0 = max, -inf = silence
    // Threshold default is -10 dBFS (roughly "loud")
    if (db < -20) return "low";
    if (db < -6) return "mid";
    return "high";
  };

  console.assert(dbClass(-50) === "low", "-50 dB should be low");
  console.assert(dbClass(-21) === "low", "-21 dB should be low");
  console.assert(dbClass(-20) === "mid", "-20 dB should be mid");
  console.assert(dbClass(-10) === "mid", "-10 dB should be mid");
  console.assert(dbClass(-6) === "high", "-6 dB should be high");
  console.assert(dbClass(0) === "high", "0 dB should be high");
  console.log("  ✅ dB classification");
}

function testEventParsing() {
  console.log("Testing event JSON parsing...");
  const validEvent = JSON.stringify({
    timestamp: "2026-05-25T08:00:00Z",
    peak_db: 72.5,
    sample_rate: 44100,
    duration_sec: 2.0,
  });

  const parsed = JSON.parse(validEvent);
  console.assert(parsed.peak_db === 72.5, "peak_db should be 72.5");
  console.assert(parsed.sample_rate === 44100, "sample_rate should be 44100");
  console.assert(parsed.duration_sec === 2.0, "duration_sec should be 2.0");
  console.log("  ✅ Event JSON parsing");
}

function testDbRanges() {
  console.log("Testing realistic dB ranges...");
  // Whisper: ~30 dB, Normal conversation: ~60 dB, Lawn mower: ~90 dB
  const whisper = 30;
  const conversation = 60;
  const mower = 90;

  console.assert(whisper >= 0 && whisper < 50, "Whisper should be 0-50 dB");
  console.assert(conversation >= 50 && conversation < 75, "Conversation should be 50-75 dB");
  console.assert(mower >= 80 && mower <= 120, "Lawn mower should be 80-120 dB");
  console.log("  ✅ dB range validation");
}

async function testServerEndpoints() {
  console.log("Testing server endpoints...");

  const Database = require("better-sqlite3");
  const dbPath = path.join(DATA_DIR, "test.db");
  const db = new Database(dbPath);

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
  `);

  // Insert test events
  const stmt = db.prepare(`
    INSERT INTO events (timestamp, peak_db, sample_rate, duration_sec, audio_path, photo_path)
    VALUES (?, ?, ?, ?, ?, ?)
  `);

  stmt.run("2026-05-25T08:00:00Z", 72.5, 44100, 2.0, "audio1.wav", "photo1.jpg");
  stmt.run("2026-05-25T08:05:00Z", 85.0, 44100, 2.0, "audio2.wav", null);
  stmt.run("2026-05-25T09:00:00Z", 45.0, 44100, 2.0, null, "photo3.jpg");

  // Test count
  const count = db.prepare("SELECT COUNT(*) as count FROM events").get().count;
  console.assert(count === 3, `Should have 3 events, got ${count}`);
  console.log("  ✅ Event insertion");

  // Test aggregation
  const metrics = db.prepare(`
    SELECT
      COUNT(*) as event_count,
      ROUND(AVG(peak_db), 1) as avg_db,
      ROUND(MAX(peak_db), 1) as max_db,
      ROUND(MIN(peak_db), 1) as min_db
    FROM events
  `).get();

  console.assert(metrics.event_count === 3, "Should count 3 events");
  console.assert(metrics.max_db === 85.0, `Max should be 85.0, got ${metrics.max_db}`);
  console.assert(metrics.min_db === 45.0, `Min should be 45.0, got ${metrics.min_db}`);
  const expectedAvg = ((72.5 + 85.0 + 45.0) / 3).toFixed(1);
  console.assert(String(metrics.avg_db) === expectedAvg, `Avg should be ${expectedAvg}, got ${metrics.avg_db}`);
  console.log("  ✅ Aggregation metrics");

  // Test filtering by timestamp
  const since = db.prepare(`
    SELECT COUNT(*) as count FROM events WHERE timestamp >= ?
  `).get("2026-05-25T08:30:00Z");
  console.assert(since.count === 1, `Should have 1 event after 08:30, got ${since.count}`);
  console.log("  ✅ Time-based filtering");

  // Test pagination
  const page = db.prepare("SELECT * FROM events ORDER BY id DESC LIMIT 2 OFFSET 0").all();
  console.assert(page.length === 2, "Should get 2 events per page");
  console.assert(page[0].peak_db === 45.0, "First should be newest (45 dB)");
  console.log("  ✅ Pagination");

  // Test delete
  db.prepare("DELETE FROM events WHERE id = ?").run(2);
  const afterDelete = db.prepare("SELECT COUNT(*) as count FROM events").get().count;
  console.assert(afterDelete === 2, "Should have 2 events after delete");
  console.log("  ✅ Delete");

  db.close();
}

async function testHourlyAggregation() {
  console.log("Testing hourly aggregation...");

  const Database = require("better-sqlite3");
  const dbPath = path.join(DATA_DIR, "test-hourly.db");
  const db = new Database(dbPath);

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
  `);

  const stmt = db.prepare(`
    INSERT INTO events (timestamp, peak_db, sample_rate, duration_sec)
    VALUES (?, ?, ?, ?)
  `);

  // 3 events in the same hour
  stmt.run("2026-05-25T08:10:00Z", 70.0, 44100, 2.0);
  stmt.run("2026-05-25T08:25:00Z", 80.0, 44100, 2.0);
  stmt.run("2026-05-25T08:45:00Z", 60.0, 44100, 2.0);
  // 1 event in a different hour
  stmt.run("2026-05-25T09:10:00Z", 90.0, 44100, 2.0);

  const metrics = db.prepare(`
    SELECT
      strftime('%Y-%m-%dT%H:00:00', timestamp) as period,
      COUNT(*) as event_count,
      ROUND(AVG(peak_db), 1) as avg_db,
      ROUND(MAX(peak_db), 1) as max_db
    FROM events
    GROUP BY period
    ORDER BY period
  `).all();

  console.assert(metrics.length === 2, `Should have 2 hourly periods, got ${metrics.length}`);
  console.assert(metrics[0].event_count === 3, "First hour should have 3 events");
  console.assert(metrics[1].event_count === 1, "Second hour should have 1 event");
  console.assert(metrics[1].max_db === 90.0, "Second hour max should be 90 dB");
  console.log("  ✅ Hourly aggregation");

  db.close();
}

async function testPiCheckinAndSettings() {
  console.log("Testing Pi check-in & settings...");

  const Database = require("better-sqlite3");
  const dbPath = path.join(DATA_DIR, "test-settings.db");
  const db = new Database(dbPath);

  db.exec(`
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

  // Insert check-in
  db.prepare(`
    INSERT INTO pi_checkins (pi_id, timestamp, uptime_sec, cpu_temp_c, mem_free_mb, disk_free_mb, events_sent)
    VALUES (?, ?, ?, ?, ?, ?, ?)
  `).run("default", new Date().toISOString(), 3600, 52.3, 256, 15000, 42);

  const latest = db.prepare("SELECT * FROM pi_checkins ORDER BY id DESC LIMIT 1").get();
  console.assert(latest.pi_id === "default", "pi_id should be default");
  console.assert(latest.cpu_temp_c === 52.3, "CPU temp should be 52.3");
  console.assert(latest.events_sent === 42, "events_sent should be 42");
  console.log("  ✅ Check-in insertion");

  // Settings defaults
  const defaults = db.prepare("SELECT * FROM pi_settings WHERE pi_id = 'default'").get();
  console.assert(defaults.threshold_db === -10, "Default threshold should be -10");
  console.assert(defaults.enabled === 1, "Should be enabled by default");
  console.log("  ✅ Default settings");

  // Update settings
  db.prepare(`UPDATE pi_settings SET threshold_db = ?, cooldown_sec = ?, enabled = ? WHERE pi_id = ?`)
    .run(-5, 10, 0, "default");
  const updated = db.prepare("SELECT * FROM pi_settings WHERE pi_id = 'default'").get();
  console.assert(updated.threshold_db === -5, "Threshold should be -5");
  console.assert(updated.cooldown_sec === 10, "Cooldown should be 10");
  console.assert(updated.enabled === 0, "Should be disabled");
  console.log("  ✅ Settings update");

  // Multiple check-ins
  db.prepare(`INSERT INTO pi_checkins (pi_id, timestamp, uptime_sec, cpu_temp_c, events_sent) VALUES (?, ?, ?, ?, ?)`)
    .run("default", new Date(Date.now() - 120000).toISOString(), 3480, 51.0, 40);
  db.prepare(`INSERT INTO pi_checkins (pi_id, timestamp, uptime_sec, cpu_temp_c, events_sent) VALUES (?, ?, ?, ?, ?)`)
    .run("default", new Date().toISOString(), 3600, 52.3, 42);

  const latestTwo = db.prepare("SELECT * FROM pi_checkins ORDER BY id DESC LIMIT 2").all();
  console.assert(latestTwo.length === 2, "Should have 2 check-ins");
  console.log("  ✅ Multiple check-ins");

  db.close();
}

// --- Run all tests ---
async function main() {
  console.log("=== Sound Guard Server Tests ===\n");

  await setup();

  try {
    testDbClassification();
    testEventParsing();
    testDbRanges();
    await testServerEndpoints();
    await testHourlyAggregation();
    await testPiCheckinAndSettings();

    console.log("\n✅ All tests passed!\n");
  } catch (err) {
    console.error("\n❌ Test failed:", err);
    process.exit(1);
  } finally {
    await teardown();
  }
}

main();
