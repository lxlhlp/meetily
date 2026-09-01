/**
 * Uplink 更新平台接入（webview 侧；Rust 侧见 src-tauri/vendor/uplink-updater-tauri）。
 *
 * baseUrl 解析：构建期烘焙的 NEXT_PUBLIC_UPLINK_BASE_URL（Next 静态内联，CI 注入）
 * > dev 回退（共享 dev 平台——平台组本地联调可设 NEXT_PUBLIC_UPLINK_BASE_URL=http://localhost:3000 覆盖）。
 * Rust 侧承担协议检查/下载/安装（动态端点 + deviceId），本值用于遥测上报与完整判定（decide）。
 * CSP 注意：遥测/判定经 webview fetch，tauri.conf.json 须放行平台地址 connect-src。
 */
import { UplinkTauriUpdater } from '@uplink/updater-sdk/tauri';
import { resolveBaseUrl, SDK_VERSION } from '@uplink/updater-sdk';

// 打包态解析不到基地址时 resolveBaseUrl 直接 throw（fail-fast），运行时不会返回 null
const baseUrl = resolveBaseUrl(
  {
    explicit: process.env.NEXT_PUBLIC_UPLINK_BASE_URL ?? null,
    devDefault: 'https://uplink.dev.hanfatong.com',
  },
  { isPackaged: process.env.NODE_ENV === 'production' },
) as string;

export const updater = new UplinkTauriUpdater({
  appId: 'meetily-moss',
  baseUrl,
  clientUpdaterVersion: SDK_VERSION,
});

/** 初始化 + 自动检查（红线 4：启动 10s 首检 + 每 24h）；onDiscovered 由 UI 接面板 */
export async function initUplink(onDiscovered: (version: string) => void): Promise<void> {
  await updater.init();
  updater.startAutoCheck(onDiscovered);
}
