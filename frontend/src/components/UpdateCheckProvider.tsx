'use client'

import React, { useEffect, useRef } from 'react';
import { mountUpdatePanel, type UpdatePanelHandle } from '@uplink/updater-sdk/ui';
import { createTauriUpdateBridge } from '@uplink/updater-sdk/tauri';
import { toast } from 'sonner';
import { updater } from '@/uplink/updater';
import { useI18n } from '@/i18n';

/**
 * 应用级更新接线（Uplink SDK 内置合规面板，红线 6 不再手写）：
 * - 根部常驻一个 off-screen 面板宿主：自动检查（启动 10s + 每 24h）发现新版本时由
 *   面板 decide() 分档——notify 弹可关闭通知（稍后提醒 3 天）、force 弹不可关闭遮罩；
 *   弹窗/遮罩为 fixed 定位，不依赖宿主容器位置，设置卡片本体置于视口外。
 * - 设置页 About 标签内另挂一个可见面板卡片（同意开关/手动检查/机器码展示）。
 * - 托盘「检查更新」→ 手动检查：有更新经面板弹窗，无更新 toast 提示。
 */
export function UpdateCheckProvider({ children }: { children: React.ReactNode }) {
  const rootHostRef = useRef<HTMLDivElement>(null);
  const panelRef = useRef<UpdatePanelHandle | null>(null);
  const { t } = useI18n();

  useEffect(() => {
    const host = rootHostRef.current;
    if (!host) return;
    const panel = mountUpdatePanel(host, { bridge: createTauriUpdateBridge(updater) });
    panelRef.current = panel;
    const stopAutoCheck = updater.startAutoCheck((version) => void panel.handleDiscovered(version));
    return () => {
      stopAutoCheck();
      panel.unmount();
      panelRef.current = null;
    };
  }, []);

  useEffect(() => {
    const handleTrayCheck = async () => {
      const outcome = await updater.check();
      if (outcome.kind === 'update' && outcome.version !== undefined) {
        void panelRef.current?.handleDiscovered(outcome.version);
      } else if (outcome.kind === 'no-update') {
        toast.info(t('settings.upToDate'));
      } else {
        toast.error(t('settings.checkUpdateFailed', { error: outcome.message ?? '' }));
      }
    };
    window.addEventListener('check-updates-from-tray', handleTrayCheck);
    return () => window.removeEventListener('check-updates-from-tray', handleTrayCheck);
  }, [t]);

  return (
    <>
      {children}
      {/* 面板宿主：设置卡片置于视口外不可见；面板内弹窗/强更遮罩为 fixed 定位照常显示 */}
      <div ref={rootHostRef} style={{ position: 'fixed', top: 0, left: -10000, width: 560 }} />
    </>
  );
}
