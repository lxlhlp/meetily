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
          text = text.replace(new RegExp(`\\{${k}\\}`, 'g'), String(v));
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
