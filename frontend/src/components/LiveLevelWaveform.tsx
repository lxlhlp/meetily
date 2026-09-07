'use client';

import { useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';

// Payload emitted by the Rust `live_input_level` module every 100ms while
// recording is active. Levels are raw pre-enhancement RMS/peak per device.
interface LiveDeviceLevel {
  rms: number;
  peak: number;
  is_active: boolean;
}

interface LiveAudioLevelEvent {
  timestamp: number;
  mic: LiveDeviceLevel;
  system: LiveDeviceLevel;
}

const BAR_COUNT = 3;
const MIN_BAR_HEIGHT = 4;  // px — idle/floor height, matches the old static look
const MAX_BAR_HEIGHT = 28; // px

// Same log curve as AudioLevelMeter: quiet speech still shows visible movement
function levelToLog(level: number): number {
  const clamped = Math.max(0, Math.min(1, level));
  return clamped > 0 ? Math.log10(clamped * 9 + 1) : 0;
}

function levelToHeight(level: number): number {
  return MIN_BAR_HEIGHT + levelToLog(level) * (MAX_BAR_HEIGHT - MIN_BAR_HEIGHT);
}

function levelToPercent(level: number): number {
  return Math.round(levelToLog(level) * 100);
}

/**
 * Recording-bar waveform driven by real captured audio levels
 * (`live-audio-level` events). Bars reflect the louder of mic/system input,
 * so a flat line during speech means the app is not actually picking up
 * sound (e.g. silently-denied microphone permission).
 */
export function LiveLevelWaveform({
  isRecording,
  isPaused,
}: {
  isRecording: boolean;
  isPaused: boolean;
}) {
  const [levels, setLevels] = useState<number[]>(() => new Array(BAR_COUNT).fill(0));
  const [latest, setLatest] = useState<{ mic: number; system: number }>({ mic: 0, system: 0 });

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    const setup = async () => {
      try {
        unlisten = await listen<LiveAudioLevelEvent>('live-audio-level', (event) => {
          if (cancelled) return;
          const { mic, system } = event.payload;
          const combined = Math.max(mic.rms, system.rms);
          setLevels((prev) => [...prev.slice(1), combined]);
          setLatest({ mic: mic.rms, system: system.rms });
        });
      } catch (err) {
        console.error('Failed to listen to live-audio-level:', err);
      }
    };
    setup();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // Clear history when a new recording starts so stale bars don't linger
  const prevRecording = useRef(isRecording);
  useEffect(() => {
    if (isRecording && !prevRecording.current) {
      setLevels(new Array(BAR_COUNT).fill(0));
    }
    prevRecording.current = isRecording;
  }, [isRecording]);

  return (
    <div
      className="flex items-center space-x-1 mx-4 h-7"
      title={`🎤 ${levelToPercent(latest.mic)}% · 🔊 ${levelToPercent(latest.system)}%`}
    >
      {levels.map((level, index) => (
        <div
          key={index}
          className={`w-1 rounded-full transition-all duration-150 ${isPaused ? 'bg-orange-500' : 'bg-red-500'
            }`}
          style={{
            height: isRecording && !isPaused ? `${levelToHeight(level)}px` : `${MIN_BAR_HEIGHT}px`,
            opacity: isPaused ? 0.6 : 1,
          }}
        />
      ))}
    </div>
  );
}
