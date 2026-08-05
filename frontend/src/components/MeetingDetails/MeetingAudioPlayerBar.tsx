"use client";

import { Play, Pause } from "lucide-react";
import { MeetingAudioPlayer } from "@/hooks/meeting-details/useMeetingAudioPlayer";
import { useI18n } from "@/i18n";

function formatTime(seconds: number): string {
    if (!isFinite(seconds) || seconds < 0) return '0:00';
    const total = Math.floor(seconds);
    const h = Math.floor(total / 3600);
    const m = Math.floor((total % 3600) / 60);
    const s = total % 60;
    const mm = h > 0 ? String(m).padStart(2, '0') : String(m);
    return h > 0
        ? `${h}:${mm}:${String(s).padStart(2, '0')}`
        : `${m}:${String(s).padStart(2, '0')}`;
}

const RATES = [1, 1.5, 2];

interface MeetingAudioPlayerBarProps {
    player: MeetingAudioPlayer;
}

/** Compact recording player for the transcript panel.
 *  The `<audio>` element streams via the asset protocol (HTTP Range), so
 *  seeking never re-downloads the whole file. */
export function MeetingAudioPlayerBar({ player }: MeetingAudioPlayerBarProps) {
    const { t } = useI18n();
    const { audioSrc, isPlaying, currentTime, duration, playbackRate } = player;

    return (
        <div className="px-4 py-2 border-b border-gray-200 bg-gray-50">
            <audio
                ref={player.audioRef}
                src={audioSrc ?? undefined}
                preload="metadata"
                onTimeUpdate={player.onTimeUpdate}
                onSeeked={player.onSeeked}
                onLoadedMetadata={player.onLoadedMetadata}
                onPlay={player.onPlay}
                onPause={player.onPause}
                onEnded={player.onEnded}
            />
            <div className="flex items-center gap-2">
                <button
                    onClick={player.togglePlay}
                    aria-label={isPlaying ? t('player.pauseRecording') : t('player.playRecording')}
                    title={isPlaying ? t('common.pause') : t('common.play')}
                    className="flex-shrink-0 w-8 h-8 rounded-full bg-blue-500 hover:bg-blue-600 text-white flex items-center justify-center transition-colors"
                >
                    {isPlaying ? <Pause size={14} /> : <Play size={14} className="ml-0.5" />}
                </button>
                <span className="text-xs text-gray-600 tabular-nums flex-shrink-0">
                    {formatTime(currentTime)}
                </span>
                <input
                    type="range"
                    min={0}
                    max={Math.max(duration, 0.1)}
                    step={0.1}
                    value={Math.min(currentTime, duration || 0)}
                    onChange={(e) => player.seek(Number(e.target.value))}
                    className="flex-1 min-w-0 accent-blue-500"
                    aria-label={t('player.playbackPosition')}
                />
                <span className="text-xs text-gray-400 tabular-nums flex-shrink-0">
                    {formatTime(duration)}
                </span>
                <div className="flex-shrink-0 flex items-center gap-0.5" aria-label={t('player.playbackSpeed')}>
                    {RATES.map((rate) => (
                        <button
                            key={rate}
                            onClick={() => player.setRate(rate)}
                            className={`text-xs px-1.5 py-0.5 rounded transition-colors ${
                                playbackRate === rate
                                    ? 'bg-blue-500 text-white'
                                    : 'text-gray-500 hover:bg-gray-200'
                            }`}
                        >
                            {rate}x
                        </button>
                    ))}
                </div>
            </div>
        </div>
    );
}
