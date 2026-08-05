import React from 'react';
import { Download } from 'lucide-react';
import { toast } from 'sonner';
import { UpdateInfo } from '@/services/updateService';
import { useI18n } from '@/i18n';

let globalShowDialogCallback: (() => void) | null = null;

export function setUpdateDialogCallback(callback: () => void) {
  globalShowDialogCallback = callback;
}

interface UpdateNotificationContentProps {
  updateInfo: UpdateInfo;
  onViewDetails: () => void;
}

function UpdateNotificationContent({ updateInfo, onViewDetails }: UpdateNotificationContentProps) {
  const { t } = useI18n();

  return (
    <div className="flex items-center justify-between gap-4">
      <div className="flex items-center gap-2">
        <Download className="h-4 w-4" />
        <div>
          <p className="font-medium">{t('settings.updateAvailable')}</p>
          <p className="text-sm text-muted-foreground">
            {t('settings.versionAvailable', { version: updateInfo.version || '' })}
          </p>
        </div>
      </div>
      <button
        onClick={(e) => {
          e.stopPropagation();
          onViewDetails();
        }}
        className="text-sm font-medium text-blue-600 hover:text-blue-700 underline"
      >
        {t('settings.viewDetails')}
      </button>
    </div>
  );
}

export function showUpdateNotification(updateInfo: UpdateInfo, onUpdateClick?: () => void) {
  const handleClick = () => {
    if (onUpdateClick) {
      onUpdateClick();
    } else if (globalShowDialogCallback) {
      globalShowDialogCallback();
    }
  };

  toast.info(
    <UpdateNotificationContent updateInfo={updateInfo} onViewDetails={handleClick} />,
    {
      duration: 10000,
      position: 'bottom-center',
    }
  );
}
