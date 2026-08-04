"use client";

/**
 * Lightweight i18n for Meetily (zero dependencies).
 *
 * Usage:
 *   const { t, locale, setLocale } = useI18n();
 *   t('sidebar.home')                    // → "首页" / "Home"
 *   t('common.deleteConfirm', { name })  // → interpolation with {name}
 *
 * Add new strings to BOTH dictionaries below.
 */

import { createContext, useContext, useState, useEffect, useCallback, ReactNode } from 'react';

export type Locale = 'zh-CN' | 'en';

const STORAGE_KEY = 'ui-locale';

type Dict = Record<string, string>;
type Dictionary = { [section: string]: Dict };

const zhCN: Dictionary = {
  common: {
    back: '返回',
    save: '保存',
    cancel: '取消',
    close: '关闭',
    delete: '删除',
    edit: '编辑',
    settings: '设置',
    loading: '加载中…',
    retry: '重试',
    optional: '可选',
    test: '测试连接',
  },
  sidebar: {
    home: '首页',
    startRecording: '开始录音',
    recordingInProgress: '录音进行中…',
    importAudio: '导入音频',
    meetingNotes: '会议笔记',
    settings: '设置',
    searchPlaceholder: '搜索会议…',
    editMeetingTitle: '编辑会议标题',
    enterMeetingTitle: '输入会议标题',
    meetingDeleted: '会议已删除',
    meetingDeleteFailed: '删除会议失败',
    titleEmpty: '会议标题不能为空',
  },
  home: {
    startRecording: '开始录音',
    stopRecording: '停止录音',
    pauseRecording: '暂停',
    resumeRecording: '继续',
    liveTranscript: '实时字幕',
    noTranscriptYet: '开始录音后，字幕将实时显示在这里',
    selectModelFirst: '请先选择转写模型',
  },
  settings: {
    title: '设置',
    general: '常规',
    recordings: '录音',
    transcription: '转写模型',
    summary: '总结模型',
    beta: '实验功能',
    uiLanguage: '界面语言',
    uiLanguageDesc: '选择应用界面的显示语言',
    transcriptModel: '转写模型',
    serverUrl: '服务器地址',
    model: '模型',
    apiKeyOptional: 'API Key（可选）',
    hotwordsOptional: '热词（可选）',
    hotwordsPlaceholder: '例如：达摩院,OpenMOSS,Meetily',
    saveAndUseMoss: '保存并使用 MOSS',
    mossSaved: 'MOSS 配置已保存',
    connectedModels: '连接成功，可用模型：{models}',
    mossLiveHint: '实时模式按句转写，说话人标签（[S01]、[S02]…）跨句可能漂移；会议结束后运行「精转」可获得全局一致的标签。',
    speakerLibrary: '说话人档案库',
    speakers: '说话人',
    speakerEmpty: '暂无说话人档案。MOSS 精转或导入后会自动为每位说话人建档。',
    speakerPlaySample: '播放示例音频以确认声纹',
    speakerNoSample: '该档案没有示例音频',
    speakerSampleMissing: '示例音频文件不存在，可能已被删除',
    speakerPlayFailed: '音频播放失败，示例文件可能已损坏',
    speakerMeetings: '出现在 {count} 个会议中',
    speakerRename: '改名',
    speakerRenamed: '姓名已更新',
    speakerMerge: '合并到首位',
    speakerMergeToFirst: '将此人合并到列表第一个档案（可能是同一人）',
    speakerMerged: '已合并',
    speakerDeleteConfirm: '确认删除「{name}」的声纹档案？示例音频将一并删除。',
    speakerDeleted: '已删除',
    speakerHint: '点击 ▶ 试听声纹，确认是谁后编辑姓名；下次会议会自动匹配同名。',
    configBackup: '配置备份',
    configExport: '导出配置',
    configImport: '导入配置',
    configExported: '配置已导出到所选文件',
    configImported: '配置已导入，即将刷新页面…',
    configImportConfirm: '导入配置会覆盖当前所有设置（转写引擎、摘要模型、API Key）。确定继续？',
  },
  meeting: {
    retranscribe: '精转',
    retranscribeMeeting: '重新转写会议',
    startRetranscription: '开始精转',
    retranscribing: '精转中…',
    retranscribeFailed: '精转失败',
    language: '语言',
    model: '模型',
    generateSummary: '生成总结',
    regenerateSummary: '重新生成',
    stopGeneration: '停止生成',
    template: '模板',
    promptPlaceholder: '为 AI 总结补充背景，例如参会人、会议主题、目标等…（⌘/Ctrl+Enter 生成）',
    saveAsQuickPrompt: '存为快捷提示',
    quickPromptSaved: '已保存为快捷提示',
    quickPromptExists: '该提示已存在',
    removeQuickPrompt: '删除此快捷提示',
    summaryStopped: '总结生成已停止',
    noTranscripts: '没有可用于生成总结的转写内容',
    mossAutoDetect: 'MOSS 自动检测中英文，通常无需手动选择语言',
    mossOnePass: 'MOSS 单遍转写完整录音，说话人标签全局一致',
    longMeetingHint: 'MOSS 正在转写，长会议可能需要几分钟，请勿关闭窗口…',
    partProgress: 'MOSS 正在转写第 {current}/{total} 部分，长会议可能需要一些时间，请勿关闭窗口…',
  },
  importAudio: {
    title: '导入音频',
    selectFile: '选择音频文件',
    start: '开始导入',
    importing: '导入中…',
  },
};

const en: Dictionary = {
  common: {
    back: 'Back',
    save: 'Save',
    cancel: 'Cancel',
    close: 'Close',
    delete: 'Delete',
    edit: 'Edit',
    settings: 'Settings',
    loading: 'Loading…',
    retry: 'Try Again',
    optional: 'optional',
    test: 'Test Connection',
  },
  sidebar: {
    home: 'Home',
    startRecording: 'Start Recording',
    recordingInProgress: 'Recording in progress...',
    importAudio: 'Import Audio',
    meetingNotes: 'Meeting Notes',
    settings: 'Settings',
    searchPlaceholder: 'Search meetings...',
    editMeetingTitle: 'Edit Meeting Title',
    enterMeetingTitle: 'Enter meeting title',
    meetingDeleted: 'Meeting deleted successfully',
    meetingDeleteFailed: 'Failed to delete meeting',
    titleEmpty: 'Meeting title cannot be empty',
  },
  home: {
    startRecording: 'Start Recording',
    stopRecording: 'Stop Recording',
    pauseRecording: 'Pause',
    resumeRecording: 'Resume',
    liveTranscript: 'Live Transcript',
    noTranscriptYet: 'Start recording to see live transcripts here',
    selectModelFirst: 'Please select a transcription model first',
  },
  settings: {
    title: 'Settings',
    general: 'General',
    recordings: 'Recordings',
    transcription: 'Transcription',
    summary: 'Summary',
    beta: 'Beta',
    uiLanguage: 'Interface Language',
    uiLanguageDesc: 'Choose the display language of the app',
    transcriptModel: 'Transcript Model',
    serverUrl: 'Server URL',
    model: 'Model',
    apiKeyOptional: 'API Key (optional)',
    hotwordsOptional: 'Hotwords (optional)',
    hotwordsPlaceholder: 'e.g. 达摩院,OpenMOSS,Meetily',
    saveAndUseMoss: 'Save & Use MOSS',
    mossSaved: 'MOSS configuration saved.',
    connectedModels: 'Connected. Available models: {models}',
    mossLiveHint: 'Live mode transcribes per utterance with a short delay; speaker labels ([S01], [S02]…) may drift between sentences. Run "Retranscribe" after the meeting for globally consistent speaker labels.',
    speakerLibrary: 'Speaker Profiles',
    speakers: 'Speakers',
    speakerEmpty: 'No speaker profiles yet. MOSS retranscription or import creates one per speaker automatically.',
    speakerPlaySample: 'Play sample audio to confirm the voiceprint',
    speakerNoSample: 'This profile has no sample audio',
    speakerSampleMissing: 'Sample audio file is missing, it may have been deleted',
    speakerPlayFailed: 'Playback failed - the sample file may be corrupted',
    speakerMeetings: 'Appears in {count} meeting(s)',
    speakerRename: 'Rename',
    speakerRenamed: 'Name updated',
    speakerMerge: 'Merge to first',
    speakerMergeToFirst: 'Merge this profile into the first one (likely the same person)',
    speakerMerged: 'Merged',
    speakerDeleteConfirm: 'Delete the voiceprint profile of "{name}"? Its sample audio will also be removed.',
    speakerDeleted: 'Deleted',
    speakerHint: 'Click ▶ to audition the voiceprint, confirm who it is, then edit the name; future meetings match it automatically.',
    configBackup: 'Configuration Backup',
    configExport: 'Export Settings',
    configImport: 'Import Settings',
    configExported: 'Configuration exported to the selected file',
    configImported: 'Configuration imported, reloading page…',
    configImportConfirm: 'Importing overwrites all current settings (transcription engine, summary model, API keys). Continue?',
  },
  meeting: {
    retranscribe: 'Retranscribe',
    retranscribeMeeting: 'Retranscribe Meeting',
    startRetranscription: 'Start Retranscription',
    retranscribing: 'Retranscribing...',
    retranscribeFailed: 'Retranscription Failed',
    language: 'Language',
    model: 'Model',
    generateSummary: 'Generate Summary',
    regenerateSummary: 'Regenerate',
    stopGeneration: 'Stop',
    template: 'Template',
    promptPlaceholder: 'Add context for AI summary. For example people involved, meeting overview, objective etc... (⌘/Ctrl+Enter to generate)',
    saveAsQuickPrompt: 'Save as quick prompt',
    quickPromptSaved: 'Saved as quick prompt',
    quickPromptExists: 'Prompt already exists',
    removeQuickPrompt: 'Remove this quick prompt',
    summaryStopped: 'Summary generation stopped',
    noTranscripts: 'No transcripts available for summary',
    mossAutoDetect: 'MOSS auto-detects Chinese and English; a manual selection is usually unnecessary',
    mossOnePass: 'MOSS transcribes the whole recording in one pass with globally consistent speaker labels',
    longMeetingHint: 'MOSS is transcribing - long meetings may take several minutes, please keep this window open...',
    partProgress: 'MOSS is transcribing part {current}/{total} - long meetings may take a while, please keep this window open...',
  },
  importAudio: {
    title: 'Import Audio',
    selectFile: 'Select audio file',
    start: 'Start Import',
    importing: 'Importing…',
  },
};

const DICTIONARIES: Record<Locale, Dictionary> = { 'zh-CN': zhCN, en };

interface I18nContextValue {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  /** Translate a dot key like 'sidebar.home', with optional {var} interpolation */
  t: (key: string, vars?: Record<string, string | number>) => string;
}

const I18nContext = createContext<I18nContextValue>({
  locale: 'zh-CN',
  setLocale: () => {},
  t: (key) => key,
});

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>('zh-CN');

  useEffect(() => {
    try {
      const saved = localStorage.getItem(STORAGE_KEY);
      if (saved === 'en' || saved === 'zh-CN') setLocaleState(saved);
    } catch { /* storage unavailable */ }
  }, []);

  const setLocale = useCallback((next: Locale) => {
    setLocaleState(next);
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch { /* storage unavailable */ }
  }, []);

  const t = useCallback(
    (key: string, vars?: Record<string, string | number>) => {
      const [section, ...rest] = key.split('.');
      const leaf = rest.join('.');
      let text =
        DICTIONARIES[locale]?.[section]?.[leaf] ??
        DICTIONARIES['en']?.[section]?.[leaf] ??
        key;
      if (vars) {
        for (const [k, v] of Object.entries(vars)) {
          // Escape $ in the replacement value: String.replace treats $$
          // specially ($&, $1, $`, $'). Without escaping, a value like
          // "财务$组" would mangle the output.
          const safe = String(v).replace(/\$/g, '$$$$');
          text = text.replace(new RegExp(`\\{${k}\\}`, 'g'), safe);
        }
      }
      return text;
    },
    [locale]
  );

  return (
    <I18nContext.Provider value={{ locale, setLocale, t }}>
      {children}
    </I18nContext.Provider>
  );
}

export function useI18n() {
  return useContext(I18nContext);
}
