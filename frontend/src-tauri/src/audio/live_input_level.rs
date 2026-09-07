//! Real input-level telemetry for the recording UI waveform.
//!
//! `note_chunk` is tapped from `AudioCapture::process_audio_data` on the audio
//! callback thread (lock-free, atomics only). The pre-enhancement signal is
//! measured so the UI shows what the device actually picks up — this also makes
//! a silently-denied microphone permission visible as a flat line.
//!
//! While recording is active, `start_emitter` pushes a `live-audio-level`
//! event to the frontend every 100ms. Note this is separate from the legacy
//! `audio-levels` event, whose active producers emit simulated data.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};
use log::{info, warn};

use super::recording_state::DeviceType;

/// RMS above this counts as "signal present" (same threshold as level_monitor)
const ACTIVE_THRESHOLD: f32 = 0.001;
const EMIT_INTERVAL_MS: u64 = 100;

#[derive(Debug, Serialize, Clone)]
pub struct LiveDeviceLevel {
    pub rms: f32,
    pub peak: f32,
    pub is_active: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct LiveAudioLevelEvent {
    pub timestamp: u64,
    pub mic: LiveDeviceLevel,
    pub system: LiveDeviceLevel,
}

struct LevelSlot {
    /// f32 bits; max RMS observed since the emitter last drained the slot
    rms: AtomicU32,
    /// f32 bits; max peak observed since the emitter last drained the slot
    peak: AtomicU32,
}

impl LevelSlot {
    const fn new() -> Self {
        Self {
            rms: AtomicU32::new(0),
            peak: AtomicU32::new(0),
        }
    }

    fn note(&self, rms: f32, peak: f32) {
        fetch_max_f32(&self.rms, rms);
        fetch_max_f32(&self.peak, peak);
    }

    fn take(&self) -> (f32, f32) {
        let rms = f32::from_bits(self.rms.swap(0, Ordering::Relaxed));
        let peak = f32::from_bits(self.peak.swap(0, Ordering::Relaxed));
        (rms, peak)
    }
}

/// Atomic max on f32 values (stored as bits); CAS loop, lock-free.
fn fetch_max_f32(slot: &AtomicU32, val: f32) {
    let mut cur = slot.load(Ordering::Relaxed);
    loop {
        if val <= f32::from_bits(cur) {
            return;
        }
        match slot.compare_exchange_weak(
            cur,
            val.to_bits(),
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(actual) => cur = actual,
        }
    }
}

static MIC_SLOT: LevelSlot = LevelSlot::new();
static SYSTEM_SLOT: LevelSlot = LevelSlot::new();
static ACTIVE: AtomicBool = AtomicBool::new(false);
static EMITTER_RUNNING: AtomicBool = AtomicBool::new(false);

/// Record a captured chunk's level. Called on the audio callback thread —
/// must stay lock-free (atomics + one O(n) pass over the samples).
pub fn note_chunk(device_type: &DeviceType, samples: &[f32]) {
    if samples.is_empty() {
        return;
    }

    let mut sum_sq = 0.0f32;
    let mut peak = 0.0f32;
    for &x in samples {
        sum_sq += x * x;
        let a = x.abs();
        if a > peak {
            peak = a;
        }
    }
    let rms = (sum_sq / samples.len() as f32).sqrt();

    match device_type {
        DeviceType::Microphone => MIC_SLOT.note(rms, peak),
        DeviceType::System => SYSTEM_SLOT.note(rms, peak),
    }
}

/// Start emitting `live-audio-level` every 100ms until `stop_emitter` is
/// called. Safe to call repeatedly: a live emitter from a previous session is
/// reused, so restart cycles never spawn duplicates.
pub fn start_emitter<R: Runtime>(app: AppHandle<R>) {
    ACTIVE.store(true, Ordering::SeqCst);
    if EMITTER_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }

    tokio::spawn(async move {
        info!("live-audio-level emitter started");
        // Falling-edge decay so bars ease down instead of snapping to zero
        // on the first silent tick after speech.
        let mut prev_mic = 0.0f32;
        let mut prev_sys = 0.0f32;
        let decay = |fresh: f32, prev: &mut f32| -> f32 {
            if fresh > 0.0 {
                *prev = fresh;
                fresh
            } else {
                *prev *= 0.6;
                if *prev < ACTIVE_THRESHOLD {
                    *prev = 0.0;
                }
                *prev
            }
        };

        while ACTIVE.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(EMIT_INTERVAL_MS)).await;

            let (mic_rms_raw, mic_peak) = MIC_SLOT.take();
            let (sys_rms_raw, sys_peak) = SYSTEM_SLOT.take();
            let event = LiveAudioLevelEvent {
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
                mic: LiveDeviceLevel {
                    rms: decay(mic_rms_raw, &mut prev_mic),
                    peak: mic_peak,
                    is_active: mic_rms_raw > ACTIVE_THRESHOLD,
                },
                system: LiveDeviceLevel {
                    rms: decay(sys_rms_raw, &mut prev_sys),
                    peak: sys_peak,
                    is_active: sys_rms_raw > ACTIVE_THRESHOLD,
                },
            };

            if let Err(e) = app.emit("live-audio-level", &event) {
                warn!("live-audio-level emit failed: {}", e);
                break;
            }
        }
        EMITTER_RUNNING.store(false, Ordering::SeqCst);
        info!("live-audio-level emitter stopped");
    });
}

/// Stop the emitter loop (called from stop_recording paths).
pub fn stop_emitter() {
    ACTIVE.store(false, Ordering::SeqCst);
}
