"use client";

import { useState, useEffect, useCallback } from 'react';
import { Plus, X } from 'lucide-react';
import { toast } from 'sonner';
import { useI18n } from '@/i18n';

interface QuickPrompt {
  label: string;
  text: string;
}

/// Built-in quick prompts are defined inside the component so labels/texts
/// can be translated via t() (see presetPrompts below).

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
  // Built-in quick prompts for common summary-context needs.
  const presetPrompts: QuickPrompt[] = [
    { label: t('meeting.quickPromptActionItems'), text: t('meeting.quickPromptActionItemsText') },
    { label: t('meeting.quickPromptDecisions'), text: t('meeting.quickPromptDecisionsText') },
    { label: t('meeting.quickPromptAttendees'), text: t('meeting.quickPromptAttendeesText') },
    { label: t('meeting.quickPromptBackground'), text: t('meeting.quickPromptBackgroundText') },
    { label: t('meeting.quickPromptEvidence'), text: t('meeting.quickPromptEvidenceText') },
  ];
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
      {presetPrompts.map((p) => (
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
