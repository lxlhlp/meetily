import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { TranscriptSegmentData } from "@/types";

export interface MeetingAudioPlayer {
    /** Streamable asset:// URL of the recording, or null when the meeting has no audio file */
    audioSrc: string | null;
    audioRef: RefObject<HTMLAudioElement>;
    isPlaying: boolean;
    currentTime: number;
    duration: number;
    playbackRate: number;
    /** Id of the transcript segment currently being spoken (for highlight sync) */
    activeSegmentId: string | null;

    // Audio element event handlers (wired via JSX so they attach on mount)
    onTimeUpdate: () => void;
    onSeeked: () => void;
    onLoadedMetadata: () => void;
    onPlay: () => void;
    onPause: () => void;
    onEnded: () => void;

    // Controls
    togglePlay: () => void;
    seek: (t: number) => void;
    setRate: (rate: number) => void;
    /** Click a transcript segment -> seek + play */
    onSegmentClick: (id: string, timestamp: number) => void;
}

interface UseMeetingAudioPlayerProps {
    meetingId: string;
    /** Loaded transcript segments, sorted by timestamp ascending */
    segments: TranscriptSegmentData[];
    loadedCount: number;
    hasMore: boolean;
    /** Replaces the loaded transcript window with the page at this offset */
    onJumpToOffset: (offset: number) => Promise<void>;
}

/** Smallest time delta (s) past the last loaded segment that triggers a page jump */
const JUMP_WINDOW_EPSILON = 2;

/**
 * Drives playback of a meeting recording and keeps the transcript highlight in
 * sync with the playhead.
 *
 * Performance notes:
 * - `timeupdate` (~4Hz) only does a binary search (O(log n)) over the sorted
 *   segments and calls `setActiveSegmentId` when the active segment CHANGES,
 *   so memoized segment rows never re-render for the same id.
 * - When the playhead passes the loaded window (or a seek lands before/after
 *   it), the backend is asked for the pagination offset at that timestamp and
 *   the window is replaced with the matching page (single COUNT + one page).
 */
export function useMeetingAudioPlayer({
    meetingId,
    segments,
    loadedCount,
    hasMore,
    onJumpToOffset,
}: UseMeetingAudioPlayerProps): MeetingAudioPlayer {
    const audioRef = useRef<HTMLAudioElement>(null);
    const [audioSrc, setAudioSrc] = useState<string | null>(null);
    const [isPlaying, setIsPlaying] = useState(false);
    const [currentTime, setCurrentTime] = useState(0);
    const [duration, setDuration] = useState(0);
    const [playbackRate, setPlaybackRate] = useState(1);
    const [activeSegmentId, setActiveSegmentId] = useState<string | null>(null);

    // Refs to read latest values inside event handlers without re-subscribing
    const segmentsRef = useRef(segments);
    segmentsRef.current = segments;
    const loadedCountRef = useRef(loadedCount);
    loadedCountRef.current = loadedCount;
    const hasMoreRef = useRef(hasMore);
    hasMoreRef.current = hasMore;
    const activeSegmentIdRef = useRef<string | null>(null);
    // Timestamp of the in-flight page jump (debounce; single jump at a time)
    const jumpInFlightRef = useRef<number | null>(null);

    // Resolve the recording file whenever the meeting changes
    useEffect(() => {
        let cancelled = false;
        setAudioSrc(null);
        setDuration(0);
        setCurrentTime(0);
        setIsPlaying(false);
        activeSegmentIdRef.current = null;
        setActiveSegmentId(null);
        jumpInFlightRef.current = null;

        if (!meetingId) return;

        (async () => {
            try {
                const path = await invoke<string | null>("api_get_meeting_audio", {
                    meetingId,
                });
                if (!cancelled && path) {
                    setAudioSrc(convertFileSrc(path));
                }
            } catch (err) {
                console.error("Failed to resolve meeting audio:", err);
            }
        })();

        return () => {
            cancelled = true;
        };
    }, [meetingId]);

    // Binary search: index of the last segment with timestamp <= t, or -1
    const findSegmentIndex = useCallback((t: number): number => {
        const list = segmentsRef.current;
        let lo = 0;
        let hi = list.length - 1;
        let ans = -1;
        while (lo <= hi) {
            const mid = (lo + hi) >> 1;
            if (list[mid].timestamp <= t) {
                ans = mid;
                lo = mid + 1;
            } else {
                hi = mid - 1;
            }
        }
        return ans;
    }, []);

    const handleTimeUpdate = useCallback(() => {
        const audio = audioRef.current;
        if (!audio) return;
        const t = audio.currentTime;
        setCurrentTime(t);

        const list = segmentsRef.current;
        const idx = findSegmentIndex(t);

        // Determine the active segment (highlight)
        const segment = idx >= 0 ? list[idx] : null;
        const nextId = segment ? segment.id : null;
        if (nextId !== activeSegmentIdRef.current) {
            activeSegmentIdRef.current = nextId;
            setActiveSegmentId(nextId);
        }

        // Determine whether the playhead is outside the loaded window
        // (before the first loaded segment, or past the last one's end)
        let outsideWindow = idx < 0;
        if (!outsideWindow && list.length > 0) {
            const last = list[list.length - 1];
            const lastEnd = last.endTime ?? last.timestamp + JUMP_WINDOW_EPSILON;
            outsideWindow = idx === list.length - 1 && t > lastEnd;
        }

        if (outsideWindow && hasMoreRef.current && onJumpToOffset) {
            const inFlight = jumpInFlightRef.current;
            // Single in-flight jump; allow re-jumping if playback moved far
            if (inFlight === null || Math.abs(t - inFlight) > 30) {
                jumpInFlightRef.current = t;
                void (async () => {
                    try {
                        const offset = await invoke<number>(
                            "api_get_transcript_offset_at",
                            { meetingId, timestamp: t }
                        );
                        await onJumpToOffset(offset);
                    } catch (err) {
                        console.error("Failed to jump transcript page:", err);
                    } finally {
                        jumpInFlightRef.current = null;
                    }
                })();
            }
        }
    }, [findSegmentIndex, meetingId, onJumpToOffset]);

    const seek = useCallback((t: number) => {
        const audio = audioRef.current;
        if (!audio) return;
        audio.currentTime = Math.max(0, Math.min(t, audio.duration || t));
        setCurrentTime(audio.currentTime);
        handleTimeUpdate();
    }, [handleTimeUpdate]);

    const togglePlay = useCallback(() => {
        const audio = audioRef.current;
        if (!audio) return;
        if (audio.paused) {
            void audio.play().catch(() => { /* autoplay/interrupt */ });
        } else {
            audio.pause();
        }
    }, []);

    const setRate = useCallback((rate: number) => {
        setPlaybackRate(rate);
        const audio = audioRef.current;
        if (audio) audio.playbackRate = rate;
    }, []);

    const onSegmentClick = useCallback((_id: string, timestamp: number) => {
        const audio = audioRef.current;
        if (!audio) return;
        audio.currentTime = Math.max(0, timestamp);
        setCurrentTime(audio.currentTime);
        handleTimeUpdate();
        void audio.play().catch(() => { /* autoplay/interrupt */ });
    }, [handleTimeUpdate]);

    return {
        audioSrc,
        audioRef,
        isPlaying,
        currentTime,
        duration,
        playbackRate,
        activeSegmentId,
        onTimeUpdate: handleTimeUpdate,
        onSeeked: handleTimeUpdate,
        onLoadedMetadata: () => {
            const audio = audioRef.current;
            setDuration(audio?.duration ?? 0);
        },
        onPlay: () => setIsPlaying(true),
        onPause: () => setIsPlaying(false),
        onEnded: () => setIsPlaying(false),
        togglePlay,
        seek,
        setRate,
        onSegmentClick,
    };
}
