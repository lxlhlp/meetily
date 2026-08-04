import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { useI18n } from '../i18n';
import { Button } from './ui/button';
import { ConfirmationModal } from './ConfirmationModel/confirmation-modal';

interface SpeakerInfo {
    id: string;
    name: string;
    sampleAudioPath: string;
    sampleText?: string | null;
    meetingCount: number;
    createdAt: string;
}

/**
 * Speaker profile library manager: lists voiceprint profiles created from
 * MOSS transcriptions, lets the user play each profile's sample audio to
 * confirm who it is, then rename/merge/delete.
 *
 * Playback uses a plain <audio> element fed by Tauri's asset protocol
 * (convertFileSrc) - the same mechanism as meeting recording playback.
 * A single shared element guarantees only one sample can be playing at a
 * time (no audio bleeding across profiles on rapid clicks).
 */
export default function SpeakerManager() {
    const { t } = useI18n();
    const [speakers, setSpeakers] = useState<SpeakerInfo[]>([]);
    const [loading, setLoading] = useState(false);
    const [editingId, setEditingId] = useState<string | null>(null);
    const [editName, setEditName] = useState('');
    const [status, setStatus] = useState<{ ok: boolean; message: string } | null>(null);
    const [deleteTarget, setDeleteTarget] = useState<SpeakerInfo | null>(null);
    const [playingPath, setPlayingPath] = useState<string | null>(null);

    const audioRef = useRef<HTMLAudioElement | null>(null);

    const loadSpeakers = useCallback(async () => {
        setLoading(true);
        try {
            const list = await invoke<SpeakerInfo[]>('api_speaker_list');
            setSpeakers(list);
        } catch (err) {
            setStatus({ ok: false, message: String(err) });
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => {
        loadSpeakers();
    }, [loadSpeakers]);

    const handlePlay = (sp: SpeakerInfo) => {
        const audio = audioRef.current;
        if (!audio || !sp.sampleAudioPath) {
            setStatus({ ok: false, message: t('settings.speakerNoSample') });
            return;
        }
        if (playingPath === sp.sampleAudioPath && !audio.paused) {
            // Toggle off: pause the currently playing sample.
            audio.pause();
            setPlayingPath(null);
            return;
        }
        // Reset the source every click so rapid switches never bleed audio
        // and playback always restarts from the beginning.
        audio.pause();
        audio.src = convertFileSrc(sp.sampleAudioPath);
        audio.currentTime = 0;
        // Update state only after play() resolves so a rapid second click
        // (which calls pause()) can't leave playingPath out of sync with
        // the actual audio element state.
        const targetPath = sp.sampleAudioPath;
        audio
            .play()
            .then(() => {
                if (!audio.paused) {
                    setPlayingPath(targetPath);
                }
            })
            .catch((err) => {
                console.error('Sample playback failed:', err);
                setStatus({ ok: false, message: t('settings.speakerPlayFailed') });
                setPlayingPath(null);
            });
        setStatus(null);
    };

    const handleAudioEnded = () => {
        setPlayingPath(null);
    };

    const startEdit = (sp: SpeakerInfo) => {
        setEditingId(sp.id);
        setEditName(sp.name);
    };

    const handleRename = async (id: string) => {
        const name = editName.trim();
        if (!name) return;
        try {
            await invoke('api_speaker_update_name', { id, name });
            setEditingId(null);
            setStatus({ ok: true, message: t('settings.speakerRenamed') });
            await loadSpeakers();
        } catch (err) {
            setStatus({ ok: false, message: String(err) });
        }
    };

    const handleMerge = async (fromId: string, toId: string) => {
        try {
            await invoke('api_speaker_merge', { fromId, toId });
            setStatus({ ok: true, message: t('settings.speakerMerged') });
            await loadSpeakers();
        } catch (err) {
            setStatus({ ok: false, message: String(err) });
        }
    };

    const handleDelete = async (sp: SpeakerInfo) => {
        setDeleteTarget(null);
        try {
            await invoke('api_speaker_delete', { id: sp.id });
            if (playingPath === sp.sampleAudioPath && audioRef.current) {
                audioRef.current.pause();
                setPlayingPath(null);
            }
            setStatus({ ok: true, message: t('settings.speakerDeleted') });
            await loadSpeakers();
        } catch (err) {
            setStatus({ ok: false, message: String(err) });
        }
    };

    return (
        <div className="space-y-3 mx-1">
            <div className="flex items-center justify-between">
                <p className="text-sm font-medium text-gray-700">
                    {t('settings.speakerLibrary')}
                    {speakers.length > 0 && (
                        <span className="text-gray-400 font-normal ml-1">
                            ({speakers.length})
                        </span>
                    )}
                </p>
                <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={loadSpeakers}
                    disabled={loading}
                >
                    {loading ? '...' : t('common.refresh')}
                </Button>
            </div>

            {speakers.length === 0 && !loading && (
                <p className="text-xs text-gray-500">{t('settings.speakerEmpty')}</p>
            )}

            {speakers.length > 0 && (
                <div className="space-y-2">
                    {speakers.map((sp, idx) => (
                        <div
                            key={sp.id}
                            className="flex items-center gap-2 border rounded-md px-2 py-1.5 bg-white"
                        >
                            <button
                                type="button"
                                title={t('settings.speakerPlaySample')}
                                className="shrink-0 w-8 h-8 rounded-full bg-blue-50 text-blue-600 hover:bg-blue-100 flex items-center justify-center"
                                onClick={() => handlePlay(sp)}
                                disabled={!sp.sampleAudioPath}
                            >
                                {playingPath === sp.sampleAudioPath ? (
                                    <svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor">
                                        <rect x="2" y="1" width="3" height="10" rx="1" />
                                        <rect x="7" y="1" width="3" height="10" rx="1" />
                                    </svg>
                                ) : (
                                    <svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor">
                                        <path d="M2 1.5v9l8-4.5-8-4.5z" />
                                    </svg>
                                )}
                            </button>

                            <div className="flex-1 min-w-0">
                                {editingId === sp.id ? (
                                    <input
                                        value={editName}
                                        onChange={(e) => setEditName(e.target.value)}
                                        onKeyDown={(e) => {
                                            if (e.key === 'Enter') handleRename(sp.id);
                                            if (e.key === 'Escape') setEditingId(null);
                                        }}
                                        autoFocus
                                        className="w-full text-sm border rounded px-1.5 py-0.5 focus:ring-1 focus:ring-blue-500"
                                    />
                                ) : (
                                    <p className="text-sm text-gray-800 truncate">{sp.name}</p>
                                )}
                                <p className="text-xs text-gray-400">
                                    {t('settings.speakerMeetings', { count: sp.meetingCount })}
                                </p>
                            </div>

                            {editingId === sp.id ? (
                                <>
                                    <Button type="button" size="sm" onClick={() => handleRename(sp.id)}>
                                        {t('common.save')}
                                    </Button>
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="ghost"
                                        onClick={() => setEditingId(null)}
                                    >
                                        {t('common.cancel')}
                                    </Button>
                                </>
                            ) : (
                                <>
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        onClick={() => startEdit(sp)}
                                    >
                                        {t('settings.speakerRename')}
                                    </Button>
                                    {idx > 0 && (
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            title={t('settings.speakerMergeToFirst')}
                                            onClick={() => handleMerge(sp.id, speakers[0].id)}
                                        >
                                            {t('settings.speakerMerge')}
                                        </Button>
                                    )}
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="ghost"
                                        className="text-red-500 hover:text-red-600"
                                        onClick={() => setDeleteTarget(sp)}
                                    >
                                        {t('common.delete')}
                                    </Button>
                                </>
                            )}
                        </div>
                    ))}
                </div>
            )}

            {status && (
                <p className={`text-sm ${status.ok ? 'text-green-600' : 'text-red-600'}`}>
                    {status.message}
                </p>
            )}

            <p className="text-xs text-gray-500">{t('settings.speakerHint')}</p>

            {/* Single shared audio element: only one sample can play at a
                time, and its native events reset the button on end/error. */}
            <audio
                ref={audioRef}
                className="hidden"
                onEnded={handleAudioEnded}
                onError={() => {
                    setStatus({ ok: false, message: t('settings.speakerPlayFailed') });
                    setPlayingPath(null);
                }}
            />

            <ConfirmationModal
                isOpen={deleteTarget !== null}
                text={deleteTarget
                    ? t('settings.speakerDeleteConfirm', { name: deleteTarget.name })
                    : ''}
                onConfirm={() => {
                    if (deleteTarget) handleDelete(deleteTarget);
                }}
                onCancel={() => setDeleteTarget(null)}
            />
        </div>
    );
}
