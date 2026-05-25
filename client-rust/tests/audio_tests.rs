use sound_guard_client::*;

#[test]
fn test_rms_to_db_silence() {
    assert!(rms_to_db(0.0).is_infinite() && rms_to_db(0.0).is_sign_negative());
}

#[test]
fn test_rms_to_db_max() {
    let db = rms_to_db(32767.0);
    assert!((db - 0.0).abs() < 0.01, "max amplitude should be ~0 dB, got {}", db);
}

#[test]
fn test_rms_to_db_half() {
    let db = rms_to_db(16383.5);
    assert!((db - (-6.02)).abs() < 0.1, "half amplitude should be ~-6 dB, got {}", db);
}

#[test]
fn test_rms_to_db_quarter() {
    let db = rms_to_db(8191.75);
    assert!((db - (-12.04)).abs() < 0.1, "quarter amplitude should be ~-12 dB, got {}", db);
}

#[test]
fn test_calculate_rms_silence() {
    let samples = [0i16; 1024];
    let rms = calculate_rms(&samples);
    assert!((rms - 0.0).abs() < 0.001);
}

#[test]
fn test_calculate_rms_full_scale() {
    let samples = [32767i16; 1024];
    let rms = calculate_rms(&samples);
    assert!((rms - 32767.0).abs() < 0.01);
}

#[test]
fn test_calculate_rms_known_signal() {
    // Square wave: alternating +10000, -10000
    let samples: Vec<i16> = (0..1024).map(|i| if i % 2 == 0 { 10000 } else { -10000 }).collect();
    let rms = calculate_rms(&samples);
    assert!((rms - 10000.0).abs() < 1.0, "RMS of square wave should be amplitude, got {}", rms);
}

#[test]
fn test_ring_buffer_push_drain() {
    let mut rb = RingBuffer::new(3);
    rb.push(vec![1, 2]);
    rb.push(vec![3, 4]);
    rb.push(vec![5, 6]);
    assert_eq!(rb.len(), 3);

    let drained = rb.drain();
    assert_eq!(drained.len(), 3);
    assert_eq!(drained[0], vec![1, 2]);
    assert_eq!(drained[2], vec![5, 6]);
    assert_eq!(rb.len(), 0);
}

#[test]
fn test_ring_buffer_overflow() {
    let mut rb = RingBuffer::new(2);
    rb.push(vec![1]);
    rb.push(vec![2]);
    rb.push(vec![3]); // should evict first
    assert_eq!(rb.len(), 2);
    let drained = rb.drain();
    assert_eq!(drained[0], vec![2]);
    assert_eq!(drained[1], vec![3]);
}

#[test]
fn test_write_wav_header() {
    let samples = [0i16; 44100]; // 1 second of silence
    let wav = write_wav(&samples, 44100, 1).unwrap();

    // Check RIFF header
    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(&wav[8..12], b"WAVE");
    assert_eq!(&wav[12..16], b"fmt ");
    assert_eq!(&wav[36..40], b"data");

    // Check data size
    let data_size = u32::from_le_bytes(wav[40..44].try_into().unwrap());
    assert_eq!(data_size, 88200); // 44100 samples * 2 bytes
}

#[test]
fn test_write_wav_stereo() {
    let samples = [100i16, -100, 200, -200];
    let wav = write_wav(&samples, 44100, 2).unwrap();

    // Check channels in fmt
    let channels = u16::from_le_bytes(wav[22..24].try_into().unwrap());
    assert_eq!(channels, 2);

    // Check sample data
    let first_sample = i16::from_le_bytes(wav[44..46].try_into().unwrap());
    assert_eq!(first_sample, 100);
}

#[test]
fn test_write_wav_non_empty() {
    let samples: Vec<i16> = (0..100).map(|i| (i as f32 * 327.67) as i16).collect();
    let wav = write_wav(&samples, 22050, 1).unwrap();

    // Should be header (44 bytes) + 200 bytes of data
    assert_eq!(wav.len(), 44 + 200);

    // Sample rate should be correct
    let sr = u32::from_le_bytes(wav[24..28].try_into().unwrap());
    assert_eq!(sr, 22050);
}

#[test]
fn test_db_threshold_detection() {
    // Simulate what the audio callback does — check that typical sounds
    // would trigger at a reasonable threshold
    let quiet: Vec<i16> = (0..1024).map(|_| 100).collect(); // very quiet
    let moderate: Vec<i16> = (0..1024).map(|_| 10000).collect(); // moderate
    let loud: Vec<i16> = (0..1024).map(|_| 30000).collect(); // loud

    let quiet_db = rms_to_db(calculate_rms(&quiet));
    let moderate_db = rms_to_db(calculate_rms(&moderate));
    let loud_db = rms_to_db(calculate_rms(&loud));

    // Our dB is relative to max (0 dB = 32767)
    // quiet=100/32767 → ~-50 dB, moderate=10000/32767 → ~-10 dB, loud=30000/32767 → ~-0.8 dB
    assert!(quiet_db < -40.0, "quiet signal should be < -40 dB, got {}", quiet_db);

    assert!(moderate_db > -15.0 && moderate_db < -5.0, "moderate signal should be ~-10 dB, got {}", moderate_db);

    assert!(loud_db > -3.0, "loud signal should be > -3 dB, got {}", loud_db);
}
