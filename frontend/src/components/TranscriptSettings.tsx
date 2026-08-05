import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Input } from './ui/input';
import { Button } from './ui/button';
import { Label } from './ui/label';
import { Eye, EyeOff, Lock, Unlock } from 'lucide-react';
import { ModelManager } from './WhisperModelManager';
import { ParakeetModelManager } from './ParakeetModelManager';
import { useI18n } from '@/i18n';


export interface TranscriptModelProps {
    provider: 'localWhisper' | 'parakeet' | 'deepgram' | 'elevenLabs' | 'groq' | 'openai' | 'moss';
    model: string;
    apiKey?: string | null;
}

export interface TranscriptSettingsProps {
    transcriptModelConfig: TranscriptModelProps;
    setTranscriptModelConfig: (config: TranscriptModelProps) => void;
    onModelSelect?: () => void;
}

export function TranscriptSettings({ transcriptModelConfig, setTranscriptModelConfig, onModelSelect }: TranscriptSettingsProps) {
    const { t } = useI18n();
    const [apiKey, setApiKey] = useState<string | null>(transcriptModelConfig.apiKey || null);
    const [showApiKey, setShowApiKey] = useState<boolean>(false);
    const [isApiKeyLocked, setIsApiKeyLocked] = useState<boolean>(true);
    const [isLockButtonVibrating, setIsLockButtonVibrating] = useState<boolean>(false);
    const [uiProvider, setUiProvider] = useState<TranscriptModelProps['provider']>(transcriptModelConfig.provider);

    // Sync uiProvider when backend config changes (e.g., after model selection or initial load)
    useEffect(() => {
        setUiProvider(transcriptModelConfig.provider);
    }, [transcriptModelConfig.provider]);

    useEffect(() => {
        if (transcriptModelConfig.provider === 'localWhisper' || transcriptModelConfig.provider === 'parakeet') {
            setApiKey(null);
        }
    }, [transcriptModelConfig.provider]);

    const fetchApiKey = async (provider: string) => {
        try {

            const data = await invoke('api_get_transcript_api_key', { provider }) as string;

            setApiKey(data || '');
        } catch (err) {
            console.error('Error fetching API key:', err);
            setApiKey(null);
        }
    };
    const modelOptions = {
        localWhisper: [], // Model selection handled by ModelManager component
        parakeet: [], // Model selection handled by ParakeetModelManager component
        moss: [], // Server URL + model handled by MossServerSettings component
        deepgram: ['nova-2-phonecall'],
        elevenLabs: ['eleven_multilingual_v2'],
        groq: ['llama-3.3-70b-versatile'],
        openai: ['gpt-4o'],
    };
    const requiresApiKey = transcriptModelConfig.provider === 'deepgram' || transcriptModelConfig.provider === 'elevenLabs' || transcriptModelConfig.provider === 'openai' || transcriptModelConfig.provider === 'groq';

    const handleInputClick = () => {
        if (isApiKeyLocked) {
            setIsLockButtonVibrating(true);
            setTimeout(() => setIsLockButtonVibrating(false), 500);
        }
    };

    const handleWhisperModelSelect = (modelName: string) => {
        // Always update config when model is selected, regardless of current provider
        // This ensures the model is set when user switches back
        setTranscriptModelConfig({
            ...transcriptModelConfig,
            provider: 'localWhisper', // Ensure provider is set correctly
            model: modelName
        });
        // Close modal after selection
        if (onModelSelect) {
            onModelSelect();
        }
    };

    const handleParakeetModelSelect = (modelName: string) => {
        // Always update config when model is selected, regardless of current provider
        // This ensures the model is set when user switches back
        setTranscriptModelConfig({
            ...transcriptModelConfig,
            provider: 'parakeet', // Ensure provider is set correctly
            model: modelName
        });
        // Close modal after selection
        if (onModelSelect) {
            onModelSelect();
        }
    };

    return (
        <div>
            <div>
                {/* <div className="flex justify-between items-center mb-4">
                    <h3 className="text-lg font-semibold text-gray-900">Transcript Settings</h3>
                </div> */}
                <div className="space-y-4 pb-6">
                    <div>
                        <Label className="block text-sm font-medium text-gray-700 mb-1">
                            {t('settings.transcriptModel')}
                        </Label>
                        <div className="flex space-x-2 mx-1">
                            <Select
                                value={uiProvider}
                                onValueChange={(value) => {
                                    const provider = value as TranscriptModelProps['provider'];
                                    setUiProvider(provider);
                                    if (provider !== 'localWhisper' && provider !== 'parakeet') {
                                        fetchApiKey(provider);
                                    }
                                }}
                            >
                                <SelectTrigger className='focus:ring-1 focus:ring-blue-500 focus:border-blue-500'>
                                    <SelectValue placeholder={t('modelSettings.selectProvider')} />
                                </SelectTrigger>
                                <SelectContent>
                                    <SelectItem value="parakeet">{t('settings.parakeetOption')}</SelectItem>
                                    <SelectItem value="localWhisper">{t('settings.whisperOption')}</SelectItem>
                                    <SelectItem value="moss">{t('settings.mossOption')}</SelectItem>
                                    {/* <SelectItem value="deepgram">☁️ Deepgram (Backup)</SelectItem>
                                    <SelectItem value="elevenLabs">☁️ ElevenLabs</SelectItem>
                                    <SelectItem value="groq">☁️ Groq</SelectItem>
                                    <SelectItem value="openai">☁️ OpenAI</SelectItem> */}
                                </SelectContent>
                            </Select>

                            {uiProvider !== 'localWhisper' && uiProvider !== 'parakeet' && uiProvider !== 'moss' && (
                                <Select
                                    value={transcriptModelConfig.model}
                                    onValueChange={(value) => {
                                        const model = value as TranscriptModelProps['model'];
                                        setTranscriptModelConfig({ ...transcriptModelConfig, provider: uiProvider, model });
                                    }}
                                >
                                    <SelectTrigger className='focus:ring-1 focus:ring-blue-500 focus:border-blue-500'>
                                        <SelectValue placeholder={t('modelSettings.selectModel')} />
                                    </SelectTrigger>
                                    <SelectContent>
                                        {modelOptions[uiProvider].map((model) => (
                                            <SelectItem key={model} value={model}>{model}</SelectItem>
                                        ))}
                                    </SelectContent>
                                </Select>
                            )}

                        </div>
                    </div>

                    {uiProvider === 'localWhisper' && (
                        <div className="mt-6">
                            <ModelManager
                                selectedModel={transcriptModelConfig.provider === 'localWhisper' ? transcriptModelConfig.model : undefined}
                                onModelSelect={handleWhisperModelSelect}
                                autoSave={true}
                            />
                        </div>
                    )}

                    {uiProvider === 'parakeet' && (
                        <div className="mt-6">
                            <ParakeetModelManager
                                selectedModel={transcriptModelConfig.provider === 'parakeet' ? transcriptModelConfig.model : undefined}
                                onModelSelect={handleParakeetModelSelect}
                                autoSave={true}
                            />
                        </div>
                    )}

                    {uiProvider === 'moss' && (
                        <div className="mt-6">
                            <MossServerSettings
                                onSaved={(model) => {
                                    setTranscriptModelConfig({
                                        ...transcriptModelConfig,
                                        provider: 'moss',
                                        model,
                                        apiKey: null,
                                    });
                                    if (onModelSelect) {
                                        onModelSelect();
                                    }
                                }}
                            />
                        </div>
                    )}


                    {requiresApiKey && (
                        <div>
                            <Label className="block text-sm font-medium text-gray-700 mb-1">
                                {t('modelSettings.apiKey')}
                            </Label>
                            <div className="relative mx-1">
                                <Input
                                    type={showApiKey ? "text" : "password"}
                                    className={`pr-24 focus:ring-1 focus:ring-blue-500 focus:border-blue-500 ${isApiKeyLocked ? 'bg-gray-100 cursor-not-allowed' : ''
                                        }`}
                                    value={apiKey || ''}
                                    onChange={(e) => setApiKey(e.target.value)}
                                    disabled={isApiKeyLocked}
                                    onClick={handleInputClick}
                                    placeholder={t('modelSettings.enterApiKey')}
                                />
                                {isApiKeyLocked && (
                                    <div
                                        onClick={handleInputClick}
                                        className="absolute inset-0 flex items-center justify-center bg-gray-100 bg-opacity-50 rounded-md cursor-not-allowed"
                                    />
                                )}
                                <div className="absolute inset-y-0 right-0 pr-1 flex items-center">
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon"
                                        onClick={() => setIsApiKeyLocked(!isApiKeyLocked)}
                                        className={`transition-colors duration-200 ${isLockButtonVibrating ? 'animate-vibrate text-red-500' : ''
                                            }`}
                                        title={isApiKeyLocked ? t('modelSettings.unlockToEdit') : t('modelSettings.lockToPreventEditing')}
                                    >
                                        {isApiKeyLocked ? <Lock className="h-4 w-4" /> : <Unlock className="h-4 w-4" />}
                                    </Button>
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon"
                                        onClick={() => setShowApiKey(!showApiKey)}
                                    >
                                        {showApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                                    </Button>
                                </div>
                            </div>
                        </div>
                    )}
                </div>
            </div>
        </div >
    )
}

interface MossServerSettingsProps {
    onSaved: (model: string) => void;
}

interface MossConfig {
    serverUrl: string;
    model: string;
    apiKey?: string | null;
    hotwords?: string | null;
}

const DEFAULT_MOSS_MODEL = 'moss-transcribe-diarize';

/**
 * Configuration form for a self-hosted MOSS-Transcribe-Diarize server.
 * The config is shared by live (in-meeting) transcription and post-meeting
 * retranscription.
 */
function MossServerSettings({ onSaved }: MossServerSettingsProps) {
    const { t } = useI18n();
    const [serverUrl, setServerUrl] = useState('');
    const [model, setModel] = useState(DEFAULT_MOSS_MODEL);
    const [mossApiKey, setMossApiKey] = useState('');
    const [hotwords, setHotwords] = useState('');
    const [testing, setTesting] = useState(false);
    const [saving, setSaving] = useState(false);
    const [status, setStatus] = useState<{ ok: boolean; message: string } | null>(null);

    useEffect(() => {
        invoke<MossConfig | null>('api_get_moss_config')
            .then((cfg) => {
                if (cfg) {
                    setServerUrl(cfg.serverUrl || '');
                    setModel(cfg.model || DEFAULT_MOSS_MODEL);
                    setMossApiKey(cfg.apiKey || '');
                    setHotwords(cfg.hotwords || '');
                }
            })
            .catch((err) => console.error('Failed to load MOSS config:', err));
    }, []);

    const handleTestConnection = async () => {
        setTesting(true);
        setStatus(null);
        try {
            const models = await invoke<string[]>('api_test_moss_connection', {
                serverUrl: serverUrl.trim(),
                apiKey: mossApiKey.trim() || null,
            });
            setStatus({
                ok: true,
                message: models.length > 0
                    ? t('settings.connectedModels', { models: models.join(', ') })
                    : t('settings.connectedModels', { models: '(none)' }),
            });
        } catch (err) {
            setStatus({ ok: false, message: String(err) });
        } finally {
            setTesting(false);
        }
    };

    const handleSave = async () => {
        setSaving(true);
        setStatus(null);
        const trimmedModel = model.trim() || DEFAULT_MOSS_MODEL;
        try {
            await invoke('api_save_moss_config', {
                serverUrl: serverUrl.trim(),
                model: trimmedModel,
                apiKey: mossApiKey.trim() || null,
                hotwords: hotwords.trim() || null,
            });
            // Also persist provider/model selection so recording and
            // retranscription pick up MOSS.
            await invoke('api_save_transcript_config', {
                provider: 'moss',
                model: trimmedModel,
                apiKey: null,
            });
            setStatus({ ok: true, message: t('settings.mossSaved') });
            onSaved(trimmedModel);
        } catch (err) {
            setStatus({ ok: false, message: String(err) });
        } finally {
            setSaving(false);
        }
    };

    return (
        <div className="space-y-3 mx-1">
            <div>
                <Label className="block text-sm font-medium text-gray-700 mb-1">{t('settings.serverUrl')}</Label>
                <Input
                    value={serverUrl}
                    onChange={(e) => setServerUrl(e.target.value)}
                    placeholder="http://192.168.1.10:8000"
                    className="focus:ring-1 focus:ring-blue-500 focus:border-blue-500"
                />
            </div>
            <div>
                <Label className="block text-sm font-medium text-gray-700 mb-1">{t('settings.model')}</Label>
                <Input
                    value={model}
                    onChange={(e) => setModel(e.target.value)}
                    placeholder={DEFAULT_MOSS_MODEL}
                    className="focus:ring-1 focus:ring-blue-500 focus:border-blue-500"
                />
            </div>
            <div>
                <Label className="block text-sm font-medium text-gray-700 mb-1">{t('settings.apiKeyOptional')}</Label>
                <Input
                    type="password"
                    value={mossApiKey}
                    onChange={(e) => setMossApiKey(e.target.value)}
                    placeholder={t('modelSettings.leaveEmptyIfNotRequired')}
                    className="focus:ring-1 focus:ring-blue-500 focus:border-blue-500"
                />
            </div>
            <div>
                <Label className="block text-sm font-medium text-gray-700 mb-1">{t('settings.hotwordsOptional')}</Label>
                <Input
                    value={hotwords}
                    onChange={(e) => setHotwords(e.target.value)}
                    placeholder={t('settings.hotwordsPlaceholder')}
                    className="focus:ring-1 focus:ring-blue-500 focus:border-blue-500"
                />
            </div>

            <div className="flex items-center space-x-2 pt-1">
                <Button
                    type="button"
                    variant="outline"
                    onClick={handleTestConnection}
                    disabled={testing || saving || !serverUrl.trim()}
                >
                    {testing ? '...' : t('common.test')}
                </Button>
                <Button
                    type="button"
                    onClick={handleSave}
                    disabled={testing || saving || !serverUrl.trim()}
                >
                    {saving ? '...' : t('settings.saveAndUseMoss')}
                </Button>
            </div>

            {status && (
                <p className={`text-sm ${status.ok ? 'text-green-600' : 'text-red-600'}`}>
                    {status.message}
                </p>
            )}

            <p className="text-xs text-gray-500 pt-1">
                {t('settings.mossLiveHint')}
            </p>
        </div>
    );
}








