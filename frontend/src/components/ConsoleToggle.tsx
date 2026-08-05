import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { toast } from 'sonner';
import { useI18n } from '@/i18n';

export function ConsoleToggle() {
  const [isLoading, setIsLoading] = useState(false);
  const [consoleVisible, setConsoleVisible] = useState(false);
  const { t } = useI18n();

  const handleOpenLogsFolder = async () => {
    setIsLoading(true);
    try {
      const path = await invoke<string>('open_logs_folder');
      toast.success(path);
    } catch (error) {
      console.error('Failed to open logs folder:', error);
      toast.error(String(error));
    } finally {
      setIsLoading(false);
    }
  };

  const handleToggleConsole = async () => {
    setIsLoading(true);
    try {
      const result = await invoke('toggle_console');
      console.log('Console toggle result:', result);
      setConsoleVisible(!consoleVisible);
    } catch (error) {
      console.error('Failed to toggle console:', error);
    } finally {
      setIsLoading(false);
    }
  };

  const handleShowConsole = async () => {
    setIsLoading(true);
    try {
      const result = await invoke('show_console');
      console.log('Show console result:', result);
      setConsoleVisible(true);
    } catch (error) {
      console.error('Failed to show console:', error);
    } finally {
      setIsLoading(false);
    }
  };

  const handleHideConsole = async () => {
    setIsLoading(true);
    try {
      const result = await invoke('hide_console');
      console.log('Hide console result:', result);
      setConsoleVisible(false);
    } catch (error) {
      console.error('Failed to hide console:', error);
    } finally {
      setIsLoading(false);
    }
  };

  // Only show this component on Windows or macOS
  if (typeof window !== 'undefined') {
    const userAgent = window.navigator.userAgent;
    if (!userAgent.includes('Windows') && !userAgent.includes('Mac')) {
      return null;
    }
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <Label htmlFor="console-toggle">
          {t('settings.developerConsole')}
        </Label>
        <Switch
          id="console-toggle"
          checked={consoleVisible}
          onCheckedChange={(checked) => {
            if (checked) {
              handleShowConsole();
            } else {
              handleHideConsole();
            }
          }}
          disabled={isLoading}
        />
      </div>
      <div className="flex space-x-2">
        <Button
          variant="outline"
          size="sm"
          onClick={handleToggleConsole}
          disabled={isLoading}
        >
          {t('settings.toggleConsole')}
        </Button>
        <Button
          variant="outline"
          size="sm"
          onClick={handleOpenLogsFolder}
          disabled={isLoading}
        >
          {t('settings.openLogsFolder')}
        </Button>
      </div>
      <p className="text-sm text-muted-foreground">
        {t('settings.logsFolderHint')}
      </p>
      <p className="text-sm text-muted-foreground">
        {t('settings.consoleHint')}
      </p>
    </div>
  );
}