//! Uplink 平台更新模块（`@uplink/updater-sdk/tauri` 的 Rust 侧，crate 形态）。
//!
//! 为什么存在：tauri-plugin-updater 的 JS `check()` 走 tauri.conf.json 静态 endpoints，
//! 无法运行时注入 deviceId（平台协议端点必填项，缺失 400 UP-5001）。本 crate 以
//! `UpdaterExt::updater_builder().endpoints(动态地址)` 自建检查/下载/安装命令；
//! minisign 验签仍在插件 Updater 下载管线内完成（不可绕过）。
//!
//! 接入（0.6.0 起为 crate 依赖，不再「复制模板文件」——升级即 bump 版本号）：
//! 1. `Cargo.toml`：`uplink-updater-tauri = { path = "…" }`（本地路径）或 git 依赖，
//!    并保留 `tauri-plugin-updater = "2"`（本 crate 用其 UpdaterExt 能力）；
//! 2. lib.rs 注册两个插件（updater 插件提供能力，uplink-updater 挂状态与命令）：
//! ```ignore
//! use uplink_updater_tauri as uplink_updater;
//!
//! tauri::Builder::default()
//!     .plugin(tauri_plugin_updater::Builder::new().build())
//!     .plugin(uplink_updater::uplink_updater_plugin(
//!         "http://localhost:3000".into(),  // UPLINK_BASE_URL（生产由构建注入；勿硬编码镜像——红线 2）
//!         "my-app".into(),                  // 平台应用标识
//!         "1.0.0".into(),                   // 更新模块版本（clientUpdaterVersion）
//!     ))
//!     .invoke_handler(tauri::generate_handler![
//!         uplink_updater::uplink_config, uplink_updater::uplink_set_channel,
//!         uplink_updater::uplink_check, uplink_updater::uplink_download,
//!         uplink_updater::uplink_install_and_relaunch,
//!     ])
//! ```
//! 3. tauri.conf.json 仍须配 `plugins.updater.pubkey`（minisign 公钥，与平台登记一致；
//!    endpoints 字段被本 crate 运行时覆盖，填占位 URL 即可）。
//!
//! deviceId 持久化：`{app_data_dir}/uplink-device-id`（与 electron 层 userData 文件同口径）。
//! JS 侧用 `@uplink/updater-sdk/tauri` 的 `UplinkTauriUpdater`（命令名与本 crate 一一对应）。

use std::sync::Mutex;
use tauri::{
    plugin::{Builder as PluginBuilder, TauriPlugin},
    AppHandle, Manager, Resource, ResourceId, Runtime,
};
use tauri_plugin_updater::UpdaterExt;
use url::Url;

/// 平台更新模块配置（JS 层经 uplink_config 读取对齐；单一事实源在本侧）
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UplinkConfig {
    pub base_url: String,
    pub app_id: String,
    pub channel: String,
    pub device_id: String,
    pub current_version: String,
    pub client_updater_version: String,
    pub platform: String,
    pub arch: String,
}

/// 已下载的更新包字节（跨命令暂存经 resources_table，与官方插件 commands 同模式）
struct DownloadedBytes(pub Vec<u8>);
impl Resource for DownloadedBytes {}

#[derive(Default)]
struct UplinkState {
    base_url: String,
    app_id: String,
    channel: Mutex<String>,
    client_updater_version: String,
    device_id: Mutex<Option<String>>,
    /// check 得到的 Update 资源句柄（download 用）
    update_rid: Mutex<Option<ResourceId>>,
    /// download 得到的字节资源句柄（install 用）
    bytes_rid: Mutex<Option<ResourceId>>,
}

/// 平台枚举映射（Tauri OS 常量 → 平台契约 win/mac/linux）
fn platform_of() -> &'static str {
    match std::env::consts::OS {
        "windows" => "win",
        "macos" => "mac",
        _ => "linux",
    }
}

fn arch_of() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        _ => "arm64",
    }
}

/// 稳定设备标识（读不到就生成 UUIDv4 形态并回写 app_data_dir）
fn device_id(app: &AppHandle<impl Runtime>) -> String {
    let state = app.state::<UplinkState>();
    if let Some(id) = state.device_id.lock().unwrap().clone() {
        return id;
    }
    let dir = app.path().app_data_dir().expect("app data dir 不可用");
    let file = dir.join("uplink-device-id");
    let id = std::fs::read_to_string(&file)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            let s = uuid_v4_fallback();
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(&file, &s);
            s
        });
    *state.device_id.lock().unwrap() = Some(id.clone());
    id
}

/// uuid v4 形态生成（设备标识非密钥，熵源降级可接受；避免引入 rand/uuid 依赖）
fn uuid_v4_fallback() -> String {
    let mut b = [0u8; 16];
    #[cfg(not(target_os = "windows"))]
    {
        use std::io::Read;
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            let _ = f.read_exact(&mut b);
        }
    }
    #[cfg(target_os = "windows")]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        for (i, slot) in b.iter_mut().enumerate() {
            *slot = (now.subsec_nanos() as u8)
                .wrapping_mul(i as u8 + 7)
                .wrapping_add(now.as_secs() as u8);
        }
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// 构建协议端点（动态清单；通道在路径段，version/platform/arch/deviceId/clientUpdaterVersion
/// 为查询参数——契约必填项）
fn endpoint(state: &UplinkState, app: &AppHandle<impl Runtime>) -> String {
    let channel = state.channel.lock().unwrap().clone();
    format!(
        "{}/api/update-check/{}/{}/tauri/latest.json?version={}&platform={}&arch={}&deviceId={}&clientUpdaterVersion={}",
        state.base_url.trim_end_matches('/'),
        state.app_id,
        channel,
        app.package_info().version,
        platform_of(),
        arch_of(),
        device_id(app),
        state.client_updater_version,
    )
}

/// 从动态端点构建 Updater
fn build_updater<R: Runtime>(app: &AppHandle<R>) -> Result<tauri_plugin_updater::Updater, String> {
    let url = {
        let state = app.state::<UplinkState>();
        endpoint(state.inner(), app)
    };
    let url = Url::parse(&url).map_err(|e| e.to_string())?;
    app.updater_builder()
        .endpoints(vec![url])
        .map_err(|e| e.to_string())? // endpoints() 在插件 v2.10 返回 Result（URL 构型校验）
        .build()
        .map_err(|e| e.to_string())
}

/// 挂载 uplink 状态插件（能力由 tauri-plugin-updater 提供，须一并注册——见 crate 文档）
pub fn uplink_updater_plugin<R: Runtime>(
    base_url: String,
    app_id: String,
    client_updater_version: String,
) -> TauriPlugin<R> {
    PluginBuilder::<R>::new("uplink-updater")
        .setup(move |app, _api| {
            app.manage(UplinkState {
                base_url,
                app_id,
                channel: Mutex::new("stable".into()),
                client_updater_version,
                device_id: Mutex::new(None),
                update_rid: Mutex::new(None),
                bytes_rid: Mutex::new(None),
            });
            Ok(())
        })
        .build()
}

pub mod commands;

pub use commands::{
    uplink_check, uplink_config, uplink_download, uplink_install_and_relaunch, uplink_set_channel,
    DownloadEvent,
};
