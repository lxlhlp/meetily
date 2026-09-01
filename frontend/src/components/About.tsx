import React, { useEffect, useRef, useState } from "react";
import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';
import Image from 'next/image';
import AnalyticsConsentSwitch from "./AnalyticsConsentSwitch";
import { mountUpdatePanel } from '@uplink/updater-sdk/ui';
import { createTauriUpdateBridge } from '@uplink/updater-sdk/tauri';
import { updater } from '@/uplink/updater';
import { useI18n } from '@/i18n';


export function About() {
    const { t } = useI18n();
    const [currentVersion, setCurrentVersion] = useState<string>('0.4.0');
    const updatePanelHostRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        // Get current version on mount
        getVersion().then(setCurrentVersion).catch(console.error);
    }, []);

    // Uplink 更新面板卡片（合规 UX 设置区：同意开关默认不勾选/手动检查/版本与机器码展示；
    // 发现新版本的弹窗与强更遮罩由面板自身 fixed 定位渲染）
    useEffect(() => {
        const host = updatePanelHostRef.current;
        if (!host) return;
        const panel = mountUpdatePanel(host, { bridge: createTauriUpdateBridge(updater) });
        return () => panel.unmount();
    }, []);

    const handleContactClick = async () => {
        try {
            await invoke('open_external_url', { url: 'https://meetily.zackriya.com/#about' });
        } catch (error) {
            console.error('Failed to open link:', error);
        }
    };

    return (
        <div className="p-4 space-y-4 h-[80vh] overflow-y-auto">
            {/* Compact Header */}
            <div className="text-center">
                <div className="mb-3">
                    <Image
                        src="icon_128x128.png"
                        alt="Meetily Logo"
                        width={64}
                        height={64}
                        className="mx-auto"
                    />
                </div>
                {/* <h1 className="text-xl font-bold text-gray-900">Meetily</h1> */}
                <span className="text-sm text-gray-500"> v{currentVersion}</span>
                <p className="text-medium text-gray-600 mt-1">
                    {t('settings.tagline')}
                </p>
            </div>

            {/* Update panel（SDK 内置合规面板） */}
            <div className="bg-gray-50 rounded-lg p-3">
                <div ref={updatePanelHostRef} />
            </div>

            {/* Features Grid - Compact */}
            <div className="space-y-3">
                <h2 className="text-base font-semibold text-gray-800">{t('settings.whatMakesDifferent')}</h2>
                <div className="grid grid-cols-2 gap-2">
                    <div className="bg-gray-50 rounded p-3 hover:bg-gray-100 transition-colors">
                        <h3 className="font-bold text-sm text-gray-900 mb-1">{t('settings.featurePrivacyFirst')}</h3>
                        <p className="text-xs text-gray-600 leading-relaxed">{t('settings.featurePrivacyFirstDesc')}</p>
                    </div>
                    <div className="bg-gray-50 rounded p-3 hover:bg-gray-100 transition-colors">
                        <h3 className="font-bold text-sm text-gray-900 mb-1">{t('settings.featureAnyModel')}</h3>
                        <p className="text-xs text-gray-600 leading-relaxed">{t('settings.featureAnyModelDesc')}</p>
                    </div>
                    <div className="bg-gray-50 rounded p-3 hover:bg-gray-100 transition-colors">
                        <h3 className="font-bold text-sm text-gray-900 mb-1">{t('settings.featureCostSmart')}</h3>
                        <p className="text-xs text-gray-600 leading-relaxed">{t('settings.featureCostSmartDesc')}</p>
                    </div>
                    <div className="bg-gray-50 rounded p-3 hover:bg-gray-100 transition-colors">
                        <h3 className="font-bold text-sm text-gray-900 mb-1">{t('settings.featureWorksEverywhere')}</h3>
                        <p className="text-xs text-gray-600 leading-relaxed">{t('settings.featureWorksEverywhereDesc')}</p>
                    </div>
                </div>
            </div>

            {/* Coming Soon - Compact */}
            <div className="bg-blue-50 rounded p-3">
                <p className="text-s text-blue-800">
                    <span className="font-bold">{t('settings.comingSoon')}</span> {t('settings.comingSoonDesc')}
                </p>
            </div>

            {/* CTA Section - Compact */}
            <div className="text-center space-y-2">
                <h3 className="text-medium font-semibold text-gray-800">{t('settings.readyToPush')}</h3>
                <p className="text-s text-gray-600">
                    {t('settings.readyToPushDescPart1')}<span className="font-bold">{t('settings.readyToPushDescStrong')}</span>{t('settings.readyToPushDescPart2')}
                </p>
                <button
                    onClick={handleContactClick}
                    className="inline-flex items-center px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white text-sm font-medium rounded transition-colors duration-200 shadow-sm hover:shadow-md"
                >
                    {t('settings.chatWithTeam')}
                </button>
            </div>

            {/* Footer - Compact */}
            <div className="pt-2 border-t border-gray-200 text-center">
                <p className="text-xs text-gray-400">
                    {t('settings.builtBy')}
                </p>
            </div>
            <AnalyticsConsentSwitch />
        </div>

    )
}
