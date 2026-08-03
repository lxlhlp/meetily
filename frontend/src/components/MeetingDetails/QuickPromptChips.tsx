"use client";

import { useState, useEffect, useCallback } from 'react';
import { Plus, X } from 'lucide-react';
import { toast } from 'sonner';
import { useI18n } from '@/i18n';

interface QuickPrompt {
  label: string;
  text: string;
}

/// Built-in quick prompts for common summary-context needs.
const PRESET_PROMPTS: QuickPrompt[] = [
  { label: '关注行动项', text: '请重点提取行动项，明确每项任务的负责人和截止时间。' },
  { label: '区分决策与讨论', text: '请明确区分已拍板的决策和仍在讨论中的提议，不要把讨论中的想法写成结论。' },
  { label: '参会人', text: '参会人包括：' },
  { label: '会议背景', text: '会议背景与目标：' },
  { label: '保留原文依据', text: '每个结论都需要附上转写原文片段作为依据。' },
];

const STORAGE_KEY = 'quick-prompt-chips';

interface QuickPromptChipsProps {
  /** Current textarea value (used by "save current as chip") */
  currentValue: string;
  /** Called with the chip text to insert into the prompt box */
  onInsert: (text: string) => void;
}

/**
 * One-click prompt snippets above the AI-summary context box.
 * Presets ship with the app; users can also save their own (localStorage).
 */
export function QuickPromptChips({ currentValue, onInsert }: QuickPromptChipsProps) {
  const { t } = useI18n();
  const [userPrompts, setUserPrompts] = useState<QuickPrompt[]>([]);

  useEffect(() => {
    try {
      const saved = localStorage.getItem(STORAGE_KEY);
      if (saved) setUserPrompts(JSON.parse(saved));
    } catch { /* ignore corrupted data */ }
  }, []);

  const persist = useCallback((prompts: QuickPrompt[]) => {
    setUserPrompts(prompts);
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(prompts));
    } catch { /* storage unavailable */ }
  }, []);

  const saveCurrent = useCallback(() => {
    const text = currentValue.trim();
    if (!text) return;
    const label = text.length > 8 ? `${text.slice(0, 8)}…` : text;
    if (userPrompts.some(p => p.text === text)) {
      toast.info(t('meeting.quickPromptExists'));
      return;
    }
    persist([...userPrompts, { label, text }]);
    toast.success(t('meeting.quickPromptSaved'));
  }, [currentValue, userPrompts, persist, t]);

  const removeUserPrompt = useCallback((index: number) => {
    persist(userPrompts.filter((_, i) => i !== index));
  }, [userPrompts, persist]);

  const chipClass =
    'inline-flex items-center gap-1 px-2 py-0.5 text-xs rounded-full border border-blue-200 bg-blue-50 text-blue-700 hover:bg-blue-100 transition-colors';

  return (
    <div className="flex flex-wrap items-center gap-1.5 px-1 pb-1">
      {PRESET_PROMPTS.map((p) => (
        <button
          key={p.label}
          type="button"
          className={chipClass}
          title={p.text}
          onClick={() => onInsert(p.text)}
        >
          {p.label}
        </button>
      ))}
      {userPrompts.map((p, i) => (
        <span key={`user-${i}`} className={`${chipClass} pr-1`}>
          <button type="button" title={p.text} onClick={() => onInsert(p.text)}>
            {p.label}
          </button>
          <button
            type="button"
            className="text-blue-400 hover:text-red-500"
            title={t('meeting.removeQuickPrompt')}
            onClick={() => removeUserPrompt(i)}
          >
            <X className="h-3 w-3" />
          </button>
        </span>
      ))}
      <button
        type="button"
        className="inline-flex items-center gap-0.5 px-2 py-0.5 text-xs rounded-full border border-dashed border-gray-300 text-gray-500 hover:border-blue-400 hover:text-blue-600 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
        title={t('meeting.saveAsQuickPrompt')}
        disabled={!currentValue.trim()}
        onClick={saveCurrent}
      >
        <Plus className="h-3 w-3" />
        {t('meeting.saveAsQuickPrompt')}
      </button>
    </div>
  );
}
