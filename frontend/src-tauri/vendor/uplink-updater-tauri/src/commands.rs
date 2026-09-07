//! 命令集（独立子模块：`#[tauri::command]` 放 crate 根会与 macro_export 提升
//! 的 `__cmd__*` 宏自我冲突 E0255——与官方 tauri 插件同构，命令入子模块，
//! lib.rs re-export 供 `generate_handler![uplink_updater::uplink_*]` 直取）。

use tauri::{ipc::Channel, AppHandle, Manager, Runtime};
use tauri_plugin_updater::Update;

use super::{
    arch_of, build_updater, device_id, platform_of, read_installed_version, write_installed_version,
    BackfillEntry, DownloadedBytes, LedgerEntry, UplinkConfig, UplinkState,
};

/// 下载进度事件（serde 形状与 JS 层 RustDownloadEvent 对齐）
#[derive(Clone, serde::Serialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum DownloadEvent {
    Started { content_length: Option<u64> },
    Progress { chunk_length: u64 },
    Finished,
}

#[tauri::command]
pub async fn uplink_config(app: AppHandle<impl Runtime>) -> Result<UplinkConfig, String> {
    let state = app.state::<UplinkState>();
    let channel = state.channel.lock().unwrap().clone();
    Ok(UplinkConfig {
        base_url: state.base_url.clone(),
        app_id: state.app_id.clone(),
        channel,
        device_id: device_id(&app),
        current_version: app.package_info().version.to_string(),
        client_updater_version: state.client_updater_version.clone(),
        platform: platform_of().into(),
        arch: arch_of().into(),
    })
}

/// 运行时切换通道（stable/beta/alpha，V1.2 三档 DICT-VER-002 v2；端点路径段与 JS 遥测上下文同步切换）。
/// 注意：JS 层 setChannel V1.2 起升级为「服务端先登记（RULE-SUB-005），成功后才调本命令」。
#[tauri::command]
pub async fn uplink_set_channel(
    app: AppHandle<impl Runtime>,
    channel: String,
) -> Result<(), String> {
    if channel != "stable" && channel != "beta" && channel != "alpha" {
        return Err("通道取值非法（stable/beta/alpha）".into());
    }
    let state = app.state::<UplinkState>();
    *state.channel.lock().unwrap() = channel;
    Ok(())
}

#[tauri::command]
pub async fn uplink_check(
    app: AppHandle<impl Runtime>,
) -> Result<Option<serde_json::Value>, String> {
    let updater = build_updater(&app)?;
    let update = updater.check().await.map_err(|e| e.to_string())?;
    let state = app.state::<UplinkState>();
    match update {
        Some(update) => {
            let version = update.version.clone().to_string();
            let rid = app.resources_table().add(update);
            *state.update_rid.lock().unwrap() = Some(rid);
            *state.last_target_version.lock().unwrap() = Some(version.clone());
            Ok(Some(serde_json::json!({ "version": version })))
        }
        None => {
            *state.update_rid.lock().unwrap() = None;
            *state.last_target_version.lock().unwrap() = None;
            Ok(None)
        }
    }
}

/// 安装版本取值（V1.4 板块十一 RULE-PRM-003 按级短路）：落账（基准=当前内嵌串）→
/// 回填缓存（基准相符）→ 包内嵌串兜底。仅用于关于页显示——判定/遥测自报恒为内嵌串。
#[tauri::command]
pub async fn uplink_get_installed_version(app: AppHandle<impl Runtime>) -> Result<String, String> {
    let embedded = app.package_info().version.to_string();
    let store = read_installed_version(&app);
    if let Some(ledger) = &store.ledger {
        if ledger.embedded_base == embedded {
            return Ok(ledger.version.clone());
        }
    }
    if let Some(backfill) = &store.backfill {
        if backfill.reported_base == embedded {
            return Ok(backfill.version.clone());
        }
    }
    Ok(embedded)
}

/// JS 层经 webview fetch 拿到 manifest.json 的安装版本回填后落盘（第二级数据源；
/// reportedBase 由本侧取当前内嵌串——JS 不感知包版本，保持 Rust 单一事实源）
#[tauri::command]
pub async fn uplink_save_installed_backfill(
    app: AppHandle<impl Runtime>,
    version: String,
) -> Result<(), String> {
    let embedded = app.package_info().version.to_string();
    let mut store = read_installed_version(&app);
    store.backfill = Some(BackfillEntry {
        version,
        reported_base: embedded,
    });
    write_installed_version(&app, &store);
    Ok(())
}

/// 下载已发现版本（minisign 验签在插件下载管线内完成；进度经 Channel 推流）
#[tauri::command]
pub async fn uplink_download(
    app: AppHandle<impl Runtime>,
    on_event: Channel<DownloadEvent>,
) -> Result<(), String> {
    let state = app.state::<UplinkState>();
    let rid = state
        .update_rid
        .lock()
        .unwrap()
        .ok_or("无可下载的更新（请先检查）")?;
    let update: std::sync::Arc<Update> = app
        .resources_table()
        .get::<Update>(rid)
        .map_err(|e| e.to_string())?;
    let update = (*update).clone();

    let mut first_chunk = true;
    let bytes = update
        .download(
            |chunk_length, content_length| {
                if first_chunk {
                    first_chunk = false;
                    let _ = on_event.send(DownloadEvent::Started { content_length });
                }
                let _ = on_event.send(DownloadEvent::Progress {
                    chunk_length: chunk_length as u64,
                });
            },
            || {
                let _ = on_event.send(DownloadEvent::Finished);
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    let bytes_rid = app.resources_table().add(DownloadedBytes(bytes));
    *state.bytes_rid.lock().unwrap() = Some(bytes_rid);
    // V1.4 板块十一 RULE-PRM-003：安装版本落账（应答目标串 + 内嵌基准串）——安装完成
    // 重启后关于页立即显示新串（含晋升包：内嵌=源串、显示=新串），不依赖联网回填
    if let Some(target) = state.last_target_version.lock().unwrap().clone() {
        let mut store = read_installed_version(&app);
        store.ledger = Some(LedgerEntry {
            version: target,
            embedded_base: app.package_info().version.to_string(),
        });
        write_installed_version(&app, &store);
    }
    Ok(())
}

/// 安装并重启（Windows 上 NSIS 安装器会退出当前应用；restart_after_install 由插件处理）
#[tauri::command]
pub async fn uplink_install_and_relaunch(app: AppHandle<impl Runtime>) -> Result<(), String> {
    let state = app.state::<UplinkState>();
    let update_rid = state
        .update_rid
        .lock()
        .unwrap()
        .ok_or("无可安装的更新（请先检查）")?;
    let bytes_rid = state
        .bytes_rid
        .lock()
        .unwrap()
        .ok_or("无可安装的更新（请先下载）")?;
    let update: std::sync::Arc<Update> = app
        .resources_table()
        .get::<Update>(update_rid)
        .map_err(|e| e.to_string())?;
    let bytes = app
        .resources_table()
        .get::<DownloadedBytes>(bytes_rid)
        .map_err(|e| e.to_string())?;

    // v2.10 无 restart_after_install（新版本才引入）：install 后自主 restart
    (*update)
        .clone()
        .install(&bytes.0)
        .map_err(|e| e.to_string())?;
    let _ = app.resources_table().close(bytes_rid);
    let _ = app.resources_table().close(update_rid);
    *state.update_rid.lock().unwrap() = None;
    *state.bytes_rid.lock().unwrap() = None;
    app.restart();
}
